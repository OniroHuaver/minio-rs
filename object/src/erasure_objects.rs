//! ErasureObjects — Storage API layer implementation
//!
//! Implements the ObjectAPI trait and orchestrates the full six-layer write/read path.

use std::sync::Arc;

use base::error::{MinioError, MinioResult};
use base::format::{VersionType, XlMeta, XlMetaEntry, XlMetaVersionHeader};
use storage::StorageAPI;
use uuid::Uuid;

use crate::object_api::{ListObjectsResult, ObjectInfo, ObjectAPI};
use crate::set::ErasureSet;

/// EC-based object storage implementation
///
/// Manages a group of ErasureSets and implements ObjectAPI.
pub struct ErasureObjects {
    /// EC disk set (Phase 1: single set; Phase 2: multiple sets)
    /// Actual path: `{disk}/{bucket}/{object}/xl.meta`
    set: Arc<ErasureSet>,
}

impl ErasureObjects {
    /// Create from disk list
    pub fn new(disks: Vec<Arc<dyn StorageAPI>>) -> MinioResult<Self> {
        let set = Arc::new(ErasureSet::new(disks)?);
        Ok(Self { set })
    }

    /// Use custom EC parameters
    pub fn with_params(
        disks: Vec<Arc<dyn StorageAPI>>,
        data_blocks: usize,
        parity_blocks: usize,
    ) -> MinioResult<Self> {
        let set = Arc::new(ErasureSet::with_params(disks, data_blocks, parity_blocks)?);
        Ok(Self { set })
    }

    /// Check write quorum
    async fn check_write_quorum(&self) -> MinioResult<()> {
        if !self.set.has_write_quorum().await {
            let online = self.set.online_disks().await.len();
            return Err(MinioError::InsufficientWriteQuorum {
                required: self.set.params().write_quorum(),
                actual: online,
            });
        }
        Ok(())
    }

    /// Check read quorum
    async fn check_read_quorum(&self) -> MinioResult<()> {
        if !self.set.has_read_quorum().await {
            let online = self.set.online_disks().await.len();
            return Err(MinioError::InsufficientReadQuorum {
                required: self.set.params().read_quorum(),
                actual: online,
            });
        }
        Ok(())
    }

    /// Object path prefix (without version ID).
    ///
    /// Returns only the object key — the bucket is already carried by the
    /// `volume` parameter of the storage API.
    fn object_path(_bucket: &str, object: &str) -> String {
        object.to_string()
    }
}

#[async_trait::async_trait]
impl ObjectAPI for ErasureObjects {
    // ---- Bucket operations ----

    async fn make_bucket(&self, bucket: &str) -> MinioResult<()> {
        let disks = self.set.online_disks().await;
        let mut successes = Vec::new();
        let mut errors: Vec<(usize, MinioError)> = Vec::new();

        for (i, disk) in disks.iter().enumerate() {
            match disk.make_volume(bucket).await {
                Ok(()) => successes.push(i),
                Err(e) => errors.push((i, e)),
            }
        }

        if !errors.is_empty() {
            // Rollback: delete volume from all successful disks
            for &i in &successes {
                let _ = disks[i].delete_volume(bucket).await;
            }
            return Err(errors.into_iter().next().unwrap().1);
        }
        Ok(())
    }

    async fn delete_bucket(&self, bucket: &str) -> MinioResult<()> {
        for disk in self.set.online_disks().await {
            let _ = disk.delete_volume(bucket).await; // best effort
        }
        Ok(())
    }

    async fn list_buckets(&self) -> MinioResult<Vec<String>> {
        // Get bucket list from the first online disk
        let online = self.set.online_disks().await;
        if let Some(disk) = online.first() {
            let entries = disk.list_dir("", "", 0).await?;
            let buckets: Vec<String> = entries
                .into_iter()
                .filter(|e| !e.starts_with('.')) // exclude hidden directories
                .collect();
            Ok(buckets)
        } else {
            Err(MinioError::Internal("no online disk".into()))
        }
    }

    async fn bucket_exists(&self, bucket: &str) -> MinioResult<bool> {
        let online = self.set.online_disks().await;
        let read_quorum = self.set.params().read_quorum();
        let mut found = 0usize;
        for disk in &online {
            match disk.file_exists(bucket, "").await {
                Ok(true) => found += 1,
                _ => {}
            }
            if found >= read_quorum {
                return Ok(true);
            }
        }
        Ok(false)
    }

    // ---- Object operations ----

    /// PUT object — full six-layer write path
    async fn put_object(
        &self,
        bucket: &str,
        object: &str,
        data: &[u8],
        metadata: &[(String, String)],
    ) -> MinioResult<ObjectInfo> {
        self.check_write_quorum().await?;

        let path = Self::object_path(bucket, object);
        let version_id = Uuid::now_v7().to_string();

        // (1) EC encode
        let shards = self.set.erasure().encode(data)?;
        // (2) Bitrot wrap + parallel write shards
        let shard_successes = self
            .set
            .write_shards(bucket, &path, &version_id, &shards)
            .await?;

        if shard_successes < self.set.params().write_quorum() {
            return Err(MinioError::InsufficientWriteQuorum {
                required: self.set.params().write_quorum(),
                actual: shard_successes,
            });
        }

        // (3) Build xl.meta (same version_id as shard path)
        let header = self.set.build_version_header(&version_id, data, metadata)?;
        let meta = XlMeta {
            versions: vec![XlMetaEntry::Object {
                header,
                data: None,
            }],
        };

        // (4) Write xl.meta
        let meta_successes = self.set.write_xl_meta(bucket, &path, &meta).await?;
        if meta_successes < self.set.params().write_quorum() {
            return Err(MinioError::InsufficientWriteQuorum {
                required: self.set.params().write_quorum(),
                actual: meta_successes,
            });
        }

        // Take value from first version's header, unwrap safe: just pushed
        let header = match &meta.versions[0] {
            XlMetaEntry::Object { header, .. } => header,
            _ => unreachable!(),
        };

        Ok(ObjectInfo {
            bucket: bucket.to_string(),
            name: object.to_string(),
            version_id: header.version_id.clone(),
            size: data.len() as i64,
            etag: header.parts.first().map_or(String::new(), |p| p.etag.clone()),
            mod_time: header.mod_time,
            content_type: header
                .meta_sys
                .iter()
                .find(|(k, _)| k == "content-type")
                .map(|(_, v)| String::from_utf8_lossy(v).to_string())
                .unwrap_or_else(|| "application/octet-stream".to_string()),
            user_metadata: header
                .meta_user
                .iter()
                .map(|(k, v)| (k.clone(), String::from_utf8_lossy(v).to_string()))
                .collect(),
        })
    }

    /// GET object — full six-layer read path
    async fn get_object(&self, bucket: &str, object: &str) -> MinioResult<(Vec<u8>, ObjectInfo)> {
        self.check_read_quorum().await?;

        let path = Self::object_path(bucket, object);

        // (1) Read xl.meta (multi-disk signature alignment)
        let meta = self.set.read_xl_meta_quorum(bucket, &path).await?;

        // (2) Select latest valid version
        let object_entry = meta
            .versions
            .iter()
            .filter_map(|v| match v {
                XlMetaEntry::Object { header, data } => Some((header, data)),
                _ => None,
            })
            .max_by_key(|(h, _)| h.mod_time)
            .ok_or_else(|| {
                MinioError::ObjectNotFound(format!("{bucket}/{object} has no valid version"))
            })?;

        let (header, inline_data) = object_entry;

        // Small object inline data
        if let Some(data) = inline_data {
            let info = ObjectInfo {
                bucket: bucket.to_string(),
                name: object.to_string(),
                version_id: header.version_id.clone(),
                size: data.len() as i64,
                etag: header.parts.first().map_or(String::new(), |p| p.etag.clone()),
                mod_time: header.mod_time,
                content_type: header
                    .meta_sys
                    .iter()
                    .find(|(k, _)| k == "content-type")
                    .map(|(_, v)| String::from_utf8_lossy(v).to_string())
                    .unwrap_or_else(|| "application/octet-stream".to_string()),
                user_metadata: header
                    .meta_user
                    .iter()
                    .map(|(k, v)| (k.clone(), String::from_utf8_lossy(v).to_string()))
                    .collect(),
            };
            return Ok((data.clone(), info));
        }

        // (3) Read shards (with Bitrot verification)
        let shards = self
            .set
            .read_shards(bucket, &path, &header.version_id)
            .await?;

        // (4) EC decode
        let data = self.set.erasure().decode(&shards)?;

        // Trim zero-padding added by EC alignment
        let actual_size: usize = header.parts.iter().map(|p| p.actual_size as usize).sum();
        let data = if data.len() > actual_size {
            data[..actual_size].to_vec()
        } else {
            data
        };

        let info = ObjectInfo {
            bucket: bucket.to_string(),
            name: object.to_string(),
            version_id: header.version_id.clone(),
            size: data.len() as i64,
            etag: header.parts.first().map_or(String::new(), |p| p.etag.clone()),
            mod_time: header.mod_time,
            content_type: header
                .meta_sys
                .iter()
                .find(|(k, _)| k == "content-type")
                .map(|(_, v)| String::from_utf8_lossy(v).to_string())
                .unwrap_or_else(|| "application/octet-stream".to_string()),
            user_metadata: header
                .meta_user
                .iter()
                .map(|(k, v)| (k.clone(), String::from_utf8_lossy(v).to_string()))
                .collect(),
        };
        Ok((data, info))
    }

    /// GET object range read
    async fn get_object_range(
        &self,
        bucket: &str,
        object: &str,
        offset: i64,
        length: i64,
    ) -> MinioResult<(Vec<u8>, ObjectInfo)> {
        // Simplified: read entire object first, then slice
        // TODO: 优化为按分片边界精准读取，避免全量解码。
        // 当前实现对大文件只读取最后 1 KiB 的场景仍然需要完整 I/O。
        // Phase 2 optimization: precise read at shard boundaries
        let (data, info) = self.get_object(bucket, object).await?;

        let start = offset.min(data.len() as i64).max(0) as usize;
        let end = (start as i64 + length).min(data.len() as i64).max(0) as usize;
        Ok((data[start..end].to_vec(), info))
    }

    /// HEAD object (metadata only)
    async fn stat_object(&self, bucket: &str, object: &str) -> MinioResult<ObjectInfo> {
        self.check_read_quorum().await?;

        let path = Self::object_path(bucket, object);
        let meta = self.set.read_xl_meta_quorum(bucket, &path).await?;

        let (header, _) = meta
            .versions
            .iter()
            .filter_map(|v| match v {
                XlMetaEntry::Object { header, data } => Some((header, data)),
                _ => None,
            })
            .max_by_key(|(h, _)| h.mod_time)
            .ok_or_else(|| {
                MinioError::ObjectNotFound(format!("{bucket}/{object} has no valid version"))
            })?;

        Ok(ObjectInfo {
            bucket: bucket.to_string(),
            name: object.to_string(),
            version_id: header.version_id.clone(),
            size: header.parts.iter().map(|p| p.actual_size).sum(),
            etag: header.parts.first().map_or(String::new(), |p| p.etag.clone()),
            mod_time: header.mod_time,
            content_type: header
                .meta_sys
                .iter()
                .find(|(k, _)| k == "content-type")
                .map(|(_, v)| String::from_utf8_lossy(v).to_string())
                .unwrap_or_else(|| "application/octet-stream".to_string()),
            user_metadata: header
                .meta_user
                .iter()
                .map(|(k, v)| (k.clone(), String::from_utf8_lossy(v).to_string()))
                .collect(),
        })
    }

    /// DELETE object — write DeleteMarker
    async fn delete_object(&self, bucket: &str, object: &str) -> MinioResult<()> {
        self.check_write_quorum().await?;

        let path = Self::object_path(bucket, object);
        let mod_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as i64;

        let version_id = Uuid::now_v7().to_string();
        let mut header = XlMetaVersionHeader::new(version_id.clone());
        header.mod_time = mod_time;
        header.r#type = VersionType::Delete as u8;
        header.signature = header.compute_signature()?;

        // Read existing xl.meta to preserve version history
        let mut meta = match self.set.read_xl_meta_quorum(bucket, &path).await {
            Ok(m) => m,
            Err(_) => XlMeta {
                versions: Vec::new(),
            },
        };

        // Append DeleteMarker to existing versions (don't overwrite)
        meta.versions.push(XlMetaEntry::Delete {
            version_id,
            mod_time,
            signature: header.signature,
            flags: 0,
        });

        let successes = self.set.write_xl_meta(bucket, &path, &meta).await?;
        if successes < self.set.params().write_quorum() {
            return Err(MinioError::InsufficientWriteQuorum {
                required: self.set.params().write_quorum(),
                actual: successes,
            });
        }
        Ok(())
    }

    /// LIST objects
    async fn list_objects(
        &self,
        bucket: &str,
        prefix: &str,
        delimiter: &str,
        max_keys: usize,
    ) -> MinioResult<ListObjectsResult> {
        let online = self.set.online_disks().await;
        let disk = online.first().ok_or_else(|| {
            MinioError::Internal("no online disk".into())
        })?;

        // List entries from disk directory
        let entries = disk.list_dir(bucket, prefix, max_keys).await?;

        let mut objects = Vec::new();
        let mut prefixes = Vec::new();

        for entry in &entries {
            if entry.ends_with('/') && !delimiter.is_empty() {
                // entry ending with '/' -> common prefix
                let prefix_path = format!("{prefix}{entry}");
                if !prefixes.contains(&prefix_path) {
                    prefixes.push(prefix_path);
                }
            } else {
                // Check if entry is an object directory (contains xl.meta)
                let meta_path = format!("{prefix}{entry}/xl.meta");
                match disk.stat_file(bucket, &meta_path).await {
                    Ok(_) => {
                        // xl.meta exists -> this is an object
                        // Simplified: construct basic ObjectInfo without reading full metadata
                        // TODO: read xl.meta for accurate metadata (size, etag, mod_time, etc.)
                        objects.push(ObjectInfo {
                            bucket: bucket.to_string(),
                            name: format!("{prefix}{entry}"),
                            version_id: String::new(),
                            size: 0,
                            etag: String::new(),
                            mod_time: 0,
                            content_type: "application/octet-stream".to_string(),
                            user_metadata: Vec::new(),
                        });
                    }
                    Err(_) => {
                        // No xl.meta -> treat as common prefix (when delimiter is set)
                        if !delimiter.is_empty() {
                            let prefix_path = format!("{prefix}{entry}/");
                            if !prefixes.contains(&prefix_path) {
                                prefixes.push(prefix_path);
                            }
                        }
                    }
                }
            }
        }

        Ok(ListObjectsResult {
            objects,
            common_prefixes: prefixes,
            is_truncated: false,
            next_marker: String::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_object_path_format() {
        let path = ErasureObjects::object_path("mybucket", "photos/sunset.jpg");
        assert_eq!(path, "photos/sunset.jpg");
    }
}
