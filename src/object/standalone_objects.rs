//! StandaloneObjects — single-disk ObjectAPI implementation
//!
//! Used for development and testing with a single disk (no EC).
//! Stores data directly without erasure coding.

use std::sync::Arc;

use crate::base::error::{MinioError, MinioResult};
use crate::base::format::{ObjectPart, XlMeta, XlMetaEntry, XlMetaVersionHeader, VersionType};
use crate::object::object_api::{ListObjectsResult, ObjectAPI, ObjectInfo};
use crate::storage::StorageAPI;

/// Single-disk object store — no EC, no quorum.
pub struct StandaloneObjects {
    disk: Arc<dyn StorageAPI>,
}

impl StandaloneObjects {
    pub fn new(disk: Arc<dyn StorageAPI>) -> Self {
        Self { disk }
    }

    /// Object-relative path: `{object}/xl.meta`.
    ///
    /// The bucket is passed as `volume` to the storage API, so it must NOT be
    /// repeated here.  Otherwise the on-disk path becomes
    /// `{disk}/{bucket}/{bucket}/{object}/xl.meta` (bucket duplicated).
    fn meta_path(object: &str) -> String {
        format!("{object}/xl.meta")
    }

    fn data_path(object: &str) -> String {
        format!("{object}/data")
    }

    fn build_meta(&self, _object: &str, data: &[u8], metadata: &[(String, String)]) -> MinioResult<XlMeta> {
        let mod_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let etag = {
            use sha2::{Digest, Sha256};
            let mut h = Sha256::new();
            h.update(data);
            format!("{:x}", h.finalize())
        };

        let mut header = XlMetaVersionHeader::new(uuid::Uuid::now_v7().to_string());
        header.mod_time = mod_time;
        header.erasure_algorithm = 0;
        header.erasure_m = 1;
        header.erasure_n = 1;
        header.erasure_block_size = 4 * 1024 * 1024;
        header.parts = vec![ObjectPart {
            number: 1,
            etag: etag.clone(),
            size: data.len() as i64,
            actual_size: data.len() as i64,
            index: 0,
        }];
        // Split metadata: Content-Type → meta_sys, user meta (x-amz-meta-*) → meta_user
        let mut meta_sys = Vec::new();
        let mut meta_user = Vec::new();
        for (k, v) in metadata {
            if k.eq_ignore_ascii_case("Content-Type") {
                meta_sys.push(("content-type".to_string(), v.as_bytes().to_vec()));
            } else {
                meta_user.push((k.clone(), v.as_bytes().to_vec()));
            }
        }
        header.meta_sys = meta_sys;
        header.meta_user = meta_user;
        header.signature = header.compute_signature()?;

        Ok(XlMeta {
            versions: vec![XlMetaEntry::Object {
                header,
                data: None,
            }],
        })
    }

    fn read_meta(&self, meta_bytes: &[u8]) -> MinioResult<(XlMetaVersionHeader, bool)> {
        let meta = XlMeta::from_bytes(meta_bytes)?;
        for entry in meta.versions.iter().rev() {
            match entry {
                XlMetaEntry::Object { header, .. } => {
                    return Ok((header.clone(), false));
                }
                XlMetaEntry::Delete { .. } => {
                    return Err(MinioError::ObjectNotFound("deleted".into()));
                }
                _ => {}
            }
        }
        Err(MinioError::ObjectNotFound("no valid version".into()))
    }
}

/// Map a DiskIO NotFound error to ObjectNotFound for cleaner HTTP responses.
fn map_not_found(e: MinioError, bucket: &str, object: &str) -> MinioError {
    match &e {
        MinioError::DiskIO(io) if io.kind() == std::io::ErrorKind::NotFound => {
            MinioError::ObjectNotFound(format!("{bucket}/{object}"))
        }
        _ => e,
    }
}

#[async_trait::async_trait]
impl ObjectAPI for StandaloneObjects {
    // ---- Bucket ops ----

    async fn make_bucket(&self, bucket: &str) -> MinioResult<()> {
        self.disk.make_volume(bucket).await
    }

    async fn delete_bucket(&self, bucket: &str) -> MinioResult<()> {
        self.disk.delete_volume(bucket).await
    }

    async fn list_buckets(&self) -> MinioResult<Vec<String>> {
        let entries = self.disk.list_dir("", "", 0).await?;
        Ok(entries.into_iter().filter(|e| !e.starts_with('.')).collect())
    }

    async fn bucket_exists(&self, bucket: &str) -> MinioResult<bool> {
        self.disk.file_exists(bucket, "").await
    }

    // ---- Object ops ----

    async fn put_object(
        &self,
        bucket: &str,
        object: &str,
        data: &[u8],
        metadata: &[(String, String)],
    ) -> MinioResult<ObjectInfo> {
        let meta = self.build_meta(object, data, metadata)?;
        let meta_bytes = meta.to_bytes()?;

        self.disk.write_all(bucket, &Self::data_path(object), data).await?;
        self.disk.write_all(bucket, &Self::meta_path(object), &meta_bytes).await?;

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
            content_type: metadata.iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("Content-Type"))
                .map(|(_, v)| v.clone())
                .unwrap_or_else(|| "application/octet-stream".into()),
            user_metadata: metadata.iter()
                .filter(|(k, _)| !k.eq_ignore_ascii_case("Content-Type"))
                .cloned()
                .collect(),
        })
    }

    async fn get_object(&self, bucket: &str, object: &str) -> MinioResult<(Vec<u8>, ObjectInfo)> {
        let meta_bytes = self.disk.read_all(bucket, &Self::meta_path(object)).await
            .map_err(|e| map_not_found(e, bucket, object))?;
        let (header, _) = self.read_meta(&meta_bytes)?;

        let data = self.disk.read_all(bucket, &Self::data_path(object)).await
            .map_err(|e| map_not_found(e, bucket, object))?;

        let info = ObjectInfo {
            bucket: bucket.to_string(),
            name: object.to_string(),
            version_id: header.version_id.clone(),
            size: data.len() as i64,
            etag: header.parts.first().map_or(String::new(), |p| p.etag.clone()),
            mod_time: header.mod_time,
            content_type: header.meta_sys.iter()
                .find(|(k, _)| k == "content-type")
                .map(|(_, v)| String::from_utf8_lossy(v).to_string())
                .unwrap_or_else(|| "application/octet-stream".into()),
            user_metadata: header.meta_user.iter()
                .map(|(k, v)| (k.clone(), String::from_utf8_lossy(v).to_string()))
                .collect(),
        };
        Ok((data, info))
    }

    async fn get_object_range(
        &self,
        bucket: &str,
        object: &str,
        offset: i64,
        length: i64,
    ) -> MinioResult<(Vec<u8>, ObjectInfo)> {
        let (data, info) = self.get_object(bucket, object).await?;
        let start = offset.min(data.len() as i64).max(0) as usize;
        let end = (start as i64 + length).min(data.len() as i64).max(0) as usize;
        Ok((data[start..end].to_vec(), info))
    }

    async fn stat_object(&self, bucket: &str, object: &str) -> MinioResult<ObjectInfo> {
        let meta_bytes = self.disk.read_all(bucket, &Self::meta_path(object)).await
            .map_err(|e| map_not_found(e, bucket, object))?;
        let (header, _) = self.read_meta(&meta_bytes)?;

        Ok(ObjectInfo {
            bucket: bucket.to_string(),
            name: object.to_string(),
            version_id: header.version_id.clone(),
            size: header.parts.iter().map(|p| p.actual_size).sum(),
            etag: header.parts.first().map_or(String::new(), |p| p.etag.clone()),
            mod_time: header.mod_time,
            content_type: header.meta_sys.iter()
                .find(|(k, _)| k == "content-type")
                .map(|(_, v)| String::from_utf8_lossy(v).to_string())
                .unwrap_or_else(|| "application/octet-stream".into()),
            user_metadata: header.meta_user.iter()
                .map(|(k, v)| (k.clone(), String::from_utf8_lossy(v).to_string()))
                .collect(),
        })
    }

    async fn delete_object(&self, bucket: &str, object: &str) -> MinioResult<()> {
        let mod_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let version_id = uuid::Uuid::now_v7().to_string();
        let mut header = XlMetaVersionHeader::new(version_id.clone());
        header.mod_time = mod_time;
        header.r#type = VersionType::Delete as u8;
        header.signature = header.compute_signature()?;

        // Read existing meta, append delete marker
        let mut meta = match self.disk.read_all(bucket, &Self::meta_path(object)).await {
            Ok(bytes) => XlMeta::from_bytes(&bytes)?,
            Err(_) => XlMeta { versions: Vec::new() },
        };

        meta.versions.push(XlMetaEntry::Delete {
            version_id,
            mod_time,
            signature: header.signature,
            flags: 0,
        });

        let meta_bytes = meta.to_bytes()?;
        self.disk.write_all(bucket, &Self::meta_path(object), &meta_bytes).await?;
        Ok(())
    }

    async fn list_objects(
        &self,
        bucket: &str,
        prefix: &str,
        delimiter: &str,
        max_keys: usize,
    ) -> MinioResult<ListObjectsResult> {
        let entries = self.disk.list_dir(bucket, prefix, max_keys).await?;
        let mut objects = Vec::new();
        let mut prefixes = Vec::new();

        for entry in &entries {
            let obj_path = format!("{prefix}{entry}");
            let meta_path = format!("{obj_path}/xl.meta");
            match self.disk.stat_file(bucket, &meta_path).await {
                Ok(stat) if !stat.is_dir => {
                    // Read xl.meta; skip entries whose latest version is a DeleteMarker
                    if let Ok(bytes) = self.disk.read_all(bucket, &meta_path).await {
                        if let Ok(meta) = XlMeta::from_bytes(&bytes) {
                            // Latest version first: if it's a Delete marker, the object is deleted
                            let is_deleted = meta.versions.iter()
                                .rev()
                                .next()
                                .map(|v| matches!(v, XlMetaEntry::Delete { .. }))
                                .unwrap_or(false);
                            if is_deleted {
                                continue;
                            }
                            if let Some(XlMetaEntry::Object { header, .. }) = meta.versions.iter()
                                .rev()
                                .find(|v| matches!(v, XlMetaEntry::Object { .. }))
                            {
                                objects.push(ObjectInfo {
                                    bucket: bucket.to_string(),
                                    name: format!("{prefix}{entry}"),
                                    version_id: header.version_id.clone(),
                                    size: header.parts.iter().map(|p| p.actual_size).sum(),
                                    etag: header.parts.first().map_or(String::new(), |p| p.etag.clone()),
                                    mod_time: header.mod_time,
                                    content_type: "application/octet-stream".into(),
                                    user_metadata: Vec::new(),
                                });
                                continue;
                            }
                        }
                    }
                    // Fallback
                    objects.push(ObjectInfo {
                        bucket: bucket.to_string(),
                        name: format!("{prefix}{entry}"),
                        version_id: String::new(),
                        size: stat.size,
                        etag: String::new(),
                        mod_time: stat.mod_time,
                        content_type: "application/octet-stream".into(),
                        user_metadata: Vec::new(),
                    });
                }
                Ok(_) => {} // directory entry — skip
                Err(_) => {
                    if !delimiter.is_empty() {
                        let p = format!("{prefix}{entry}/");
                        if !prefixes.contains(&p) {
                            prefixes.push(p);
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
