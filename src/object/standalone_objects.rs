//! StandaloneObjects — single-disk ObjectAPI implementation
//!
//! Used for development and testing with a single disk (no EC).
//! Stores data directly without erasure coding.

use std::sync::Arc;

use crate::base::error::{MinioError, MinioResult};
use crate::base::format::{ObjectPart, VersionType, XlMeta, XlMetaEntry, XlMetaVersionHeader};
use crate::object::object_api::{
    CompletedPart, DeleteObjectsResult, ListObjectsResult, MetadataDirective, MultipartInfo,
    ObjectAPI, ObjectInfo, VersioningConfig, VersioningStatus,
};
use crate::storage::StorageAPI;

/// Metadata persisted for a multipart upload in staging.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct UploadMeta {
    upload_id: String,
    bucket: String,
    object: String,
    initiated: i64,
    metadata: Vec<(String, String)>,
    parts: Vec<UploadPartMeta>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct UploadPartMeta {
    number: u32,
    etag: String,
    size: i64,
}

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

    fn build_meta(
        &self,
        _object: &str,
        data: &[u8],
        metadata: &[(String, String)],
    ) -> MinioResult<XlMeta> {
        let mod_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as i64;

        let etag = {
            use md5::{Digest, Md5};
            let mut h = Md5::new();
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
            versions: vec![XlMetaEntry::Object { header, data: None }],
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

    // ---- Multipart helpers ----

    fn upload_meta_path(upload_id: &str) -> String {
        format!("{}/upload.meta", upload_id)
    }

    fn part_path(upload_id: &str, part_number: u32) -> String {
        format!("{}/part.{:05}", upload_id, part_number)
    }

    async fn read_upload_meta(&self, upload_id: &str) -> MinioResult<UploadMeta> {
        let data = self
            .disk
            .read_all(
                crate::base::constants::MULTIPART_DIR,
                &Self::upload_meta_path(upload_id),
            )
            .await
            .map_err(|_| MinioError::NoSuchUpload(upload_id.to_string()))?;
        serde_json::from_slice(&data)
            .map_err(|e| MinioError::Internal(format!("invalid upload.meta: {e}")))
    }

    async fn write_upload_meta(&self, meta: &UploadMeta) -> MinioResult<()> {
        let data =
            serde_json::to_vec(meta).map_err(|e| MinioError::Internal(format!("json: {e}")))?;
        self.disk
            .write_all(
                crate::base::constants::MULTIPART_DIR,
                &Self::upload_meta_path(&meta.upload_id),
                &data,
            )
            .await
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
        if self.disk.file_exists(bucket, "").await? {
            return Err(MinioError::BucketAlreadyExists(bucket.to_string()));
        }
        self.disk.make_volume(bucket).await
    }

    async fn delete_bucket(&self, bucket: &str) -> MinioResult<()> {
        self.disk.delete_volume(bucket).await
    }

    async fn list_buckets(&self) -> MinioResult<Vec<String>> {
        let entries = self.disk.list_dir("", "", 0).await?;
        Ok(entries
            .into_iter()
            .filter(|e| !e.starts_with('.'))
            .collect())
    }

    async fn bucket_exists(&self, bucket: &str) -> MinioResult<bool> {
        self.disk.file_exists(bucket, "").await
    }

    async fn get_bucket_versioning(&self, bucket: &str) -> MinioResult<Option<VersioningConfig>> {
        let path = format!(".minio.sys/buckets/{bucket}/versioning.json");
        match self.disk.read_all("", &path).await {
            Ok(data) => serde_json::from_slice(&data)
                .map_err(|e| MinioError::Internal(format!("versioning config: {e}"))),
            Err(MinioError::DiskIO(e)) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    async fn set_bucket_versioning(&self, bucket: &str, status: &str) -> MinioResult<()> {
        let config = VersioningConfig {
            status: match status {
                "Enabled" => VersioningStatus::Enabled,
                "Suspended" => VersioningStatus::Suspended,
                _ => {
                    return Err(MinioError::Internal(format!(
                        "invalid versioning status: {status}"
                    )));
                }
            },
        };
        let path = format!(".minio.sys/buckets/{bucket}/versioning.json");
        let data =
            serde_json::to_vec(&config).map_err(|e| MinioError::Internal(format!("json: {e}")))?;
        self.disk.write_all("", &path, &data).await
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

        self.disk
            .write_all(bucket, &Self::data_path(object), data)
            .await?;
        self.disk
            .write_all(bucket, &Self::meta_path(object), &meta_bytes)
            .await?;

        let header = match &meta.versions[0] {
            XlMetaEntry::Object { header, .. } => header,
            _ => unreachable!(),
        };

        Ok(ObjectInfo {
            bucket: bucket.to_string(),
            name: object.to_string(),
            version_id: header.version_id.clone(),
            size: data.len() as i64,
            etag: header
                .parts
                .first()
                .map_or(String::new(), |p| p.etag.clone()),
            mod_time: header.mod_time,
            content_type: metadata
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("Content-Type"))
                .map(|(_, v)| v.clone())
                .unwrap_or_else(|| "application/octet-stream".into()),
            user_metadata: metadata
                .iter()
                .filter(|(k, _)| !k.eq_ignore_ascii_case("Content-Type"))
                .cloned()
                .collect(),
        })
    }

    async fn copy_object(
        &self,
        src_bucket: &str,
        src_object: &str,
        dst_bucket: &str,
        dst_object: &str,
        metadata: &[(String, String)],
        directive: MetadataDirective,
    ) -> MinioResult<ObjectInfo> {
        let (data, src_info) = self.get_object(src_bucket, src_object).await?;
        let final_metadata = match directive {
            MetadataDirective::Copy => {
                let mut meta = vec![("Content-Type".to_string(), src_info.content_type)];
                for (k, v) in &src_info.user_metadata {
                    meta.push((k.clone(), v.clone()));
                }
                meta
            }
            MetadataDirective::Replace => metadata.to_vec(),
        };
        self.put_object(dst_bucket, dst_object, &data, &final_metadata)
            .await
    }

    async fn new_multipart_upload(
        &self,
        bucket: &str,
        object: &str,
        metadata: &[(String, String)],
    ) -> MinioResult<MultipartInfo> {
        let upload_id = uuid::Uuid::now_v7().to_string();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as i64;

        let meta = UploadMeta {
            upload_id: upload_id.clone(),
            bucket: bucket.to_string(),
            object: object.to_string(),
            initiated: now,
            metadata: metadata.to_vec(),
            parts: Vec::new(),
        };
        self.write_upload_meta(&meta).await?;

        Ok(MultipartInfo {
            upload_id,
            bucket: bucket.to_string(),
            object: object.to_string(),
            initiated: now,
        })
    }

    async fn put_object_part(
        &self,
        _bucket: &str,
        _object: &str,
        upload_id: &str,
        part_number: u32,
        data: &[u8],
    ) -> MinioResult<String> {
        let mut meta = self.read_upload_meta(upload_id).await?;

        // Compute ETag for the part (MD5 hex, per S3 single-part convention)
        let etag = {
            use md5::{Digest, Md5};
            let mut h = Md5::new();
            h.update(data);
            format!("{:x}", h.finalize())
        };

        // Write part data
        self.disk
            .write_all(
                crate::base::constants::MULTIPART_DIR,
                &Self::part_path(upload_id, part_number),
                data,
            )
            .await?;

        // Update upload.meta
        meta.parts.retain(|p| p.number != part_number);
        meta.parts.push(UploadPartMeta {
            number: part_number,
            etag: etag.clone(),
            size: data.len() as i64,
        });
        self.write_upload_meta(&meta).await?;

        Ok(etag)
    }

    async fn complete_multipart_upload(
        &self,
        bucket: &str,
        object: &str,
        upload_id: &str,
        parts: &[CompletedPart],
    ) -> MinioResult<ObjectInfo> {
        let meta = self.read_upload_meta(upload_id).await?;

        // Validate: all requested parts must exist in the uploaded parts
        let uploaded: std::collections::HashMap<u32, &UploadPartMeta> =
            meta.parts.iter().map(|p| (p.number, p)).collect();

        let mut object_parts = Vec::new();
        let mut total_size: i64 = 0;
        let mut prev_number: u32 = 0;

        for cp in parts {
            if cp.part_number <= prev_number {
                return Err(MinioError::InvalidPart(format!(
                    "part numbers must be in ascending order"
                )));
            }
            prev_number = cp.part_number;

            let up = uploaded.get(&cp.part_number).ok_or_else(|| {
                MinioError::InvalidPart(format!("part {} not uploaded", cp.part_number))
            })?;

            if up.etag != cp.etag {
                return Err(MinioError::InvalidPart(format!(
                    "part {} etag mismatch: expected {}, got {}",
                    cp.part_number, up.etag, cp.etag
                )));
            }

            // Minimum part size check (except last part — the one with highest PartNumber)
            let max_part_number = parts.iter().map(|p| p.part_number).max().unwrap_or(0);
            if cp.part_number < max_part_number && up.size < 5 * 1024 * 1024 {
                return Err(MinioError::EntityTooSmall);
            }

            total_size += up.size;
            object_parts.push(ObjectPart {
                number: cp.part_number,
                etag: cp.etag.clone(),
                size: up.size,
                actual_size: up.size,
                index: (cp.part_number - 1) as i32,
            });
        }

        // Concatenate all parts into final data
        let mut data = Vec::with_capacity(total_size as usize);
        for cp in parts {
            let part_bytes = self
                .disk
                .read_all(
                    crate::base::constants::MULTIPART_DIR,
                    &Self::part_path(upload_id, cp.part_number),
                )
                .await
                .map_err(|_| {
                    MinioError::InvalidPart(format!("cannot read part {}", cp.part_number))
                })?;
            data.extend_from_slice(&part_bytes);
        }

        // Build xl.meta with parts list
        let mod_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as i64;

        // S3 multipart ETag = MD5(concat(binary_MD5(part1), ..., binary_MD5(partN))) + "-N"
        let etag = {
            use md5::{Digest, Md5};
            let mut concat = Vec::with_capacity(16 * parts.len());
            for part in &meta.parts {
                let bin = hex::decode(&part.etag).unwrap_or_default();
                if bin.len() == 16 {
                    concat.extend_from_slice(&bin);
                }
            }
            let mut h = Md5::new();
            h.update(&concat);
            format!("{:x}-{}", h.finalize(), parts.len())
        };

        let mut header = XlMetaVersionHeader::new(uuid::Uuid::now_v7().to_string());
        header.mod_time = mod_time;
        header.erasure_algorithm = 0;
        header.erasure_m = 1;
        header.erasure_n = 1;
        header.erasure_block_size = 4 * 1024 * 1024;
        header.parts = object_parts;

        let mut meta_sys = Vec::new();
        let mut meta_user = Vec::new();
        for (k, v) in &meta.metadata {
            if k.eq_ignore_ascii_case("Content-Type") {
                meta_sys.push(("content-type".to_string(), v.as_bytes().to_vec()));
            } else {
                meta_user.push((k.clone(), v.as_bytes().to_vec()));
            }
        }
        header.meta_sys = meta_sys;
        header.meta_user = meta_user;
        header.signature = header.compute_signature()?;

        let xl_meta = XlMeta {
            versions: vec![XlMetaEntry::Object {
                header: header.clone(),
                data: None,
            }],
        };

        // Write final object
        self.disk
            .write_all(bucket, &Self::data_path(object), &data)
            .await?;
        self.disk
            .write_all(bucket, &Self::meta_path(object), &xl_meta.to_bytes()?)
            .await?;

        // Clean up staging — all uploaded parts + upload.meta
        for part in &meta.parts {
            let _ = self
                .disk
                .delete(
                    crate::base::constants::MULTIPART_DIR,
                    &Self::part_path(upload_id, part.number),
                )
                .await;
        }
        let _ = self
            .disk
            .delete(
                crate::base::constants::MULTIPART_DIR,
                &Self::upload_meta_path(upload_id),
            )
            .await;

        Ok(ObjectInfo {
            bucket: bucket.to_string(),
            name: object.to_string(),
            version_id: header.version_id.clone(),
            size: total_size,
            etag,
            mod_time: header.mod_time,
            content_type: meta
                .metadata
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("Content-Type"))
                .map(|(_, v)| v.clone())
                .unwrap_or_else(|| "application/octet-stream".into()),
            user_metadata: meta
                .metadata
                .iter()
                .filter(|(k, _)| !k.eq_ignore_ascii_case("Content-Type"))
                .cloned()
                .collect(),
        })
    }

    async fn abort_multipart_upload(
        &self,
        _bucket: &str,
        _object: &str,
        upload_id: &str,
    ) -> MinioResult<()> {
        // Delete part files
        let meta = self.read_upload_meta(upload_id).await?;
        for part in &meta.parts {
            let _ = self
                .disk
                .delete(
                    crate::base::constants::MULTIPART_DIR,
                    &Self::part_path(upload_id, part.number),
                )
                .await;
        }
        // Delete upload.meta
        let _ = self
            .disk
            .delete(
                crate::base::constants::MULTIPART_DIR,
                &Self::upload_meta_path(upload_id),
            )
            .await;
        Ok(())
    }

    async fn get_object(&self, bucket: &str, object: &str) -> MinioResult<(Vec<u8>, ObjectInfo)> {
        let meta_bytes = self
            .disk
            .read_all(bucket, &Self::meta_path(object))
            .await
            .map_err(|e| map_not_found(e, bucket, object))?;
        let (header, _) = self.read_meta(&meta_bytes)?;

        let data = self
            .disk
            .read_all(bucket, &Self::data_path(object))
            .await
            .map_err(|e| map_not_found(e, bucket, object))?;

        let info = ObjectInfo {
            bucket: bucket.to_string(),
            name: object.to_string(),
            version_id: header.version_id.clone(),
            size: data.len() as i64,
            etag: header
                .parts
                .first()
                .map_or(String::new(), |p| p.etag.clone()),
            mod_time: header.mod_time,
            content_type: header
                .meta_sys
                .iter()
                .find(|(k, _)| k == "content-type")
                .map(|(_, v)| String::from_utf8_lossy(v).to_string())
                .unwrap_or_else(|| "application/octet-stream".into()),
            user_metadata: header
                .meta_user
                .iter()
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
        // Read metadata first (small), then stream only the requested byte range.
        let info = self
            .stat_object(bucket, object)
            .await
            .map_err(|e| map_not_found(e, bucket, object))?;
        let data = self
            .disk
            .read_range(bucket, &Self::data_path(object), offset, length)
            .await
            .map_err(|e| map_not_found(e, bucket, object))?;
        Ok((data, info))
    }

    async fn stat_object(&self, bucket: &str, object: &str) -> MinioResult<ObjectInfo> {
        let meta_bytes = self
            .disk
            .read_all(bucket, &Self::meta_path(object))
            .await
            .map_err(|e| map_not_found(e, bucket, object))?;
        let (header, _) = self.read_meta(&meta_bytes)?;

        Ok(ObjectInfo {
            bucket: bucket.to_string(),
            name: object.to_string(),
            version_id: header.version_id.clone(),
            size: header.parts.iter().map(|p| p.actual_size).sum(),
            etag: header
                .parts
                .first()
                .map_or(String::new(), |p| p.etag.clone()),
            mod_time: header.mod_time,
            content_type: header
                .meta_sys
                .iter()
                .find(|(k, _)| k == "content-type")
                .map(|(_, v)| String::from_utf8_lossy(v).to_string())
                .unwrap_or_else(|| "application/octet-stream".into()),
            user_metadata: header
                .meta_user
                .iter()
                .map(|(k, v)| (k.clone(), String::from_utf8_lossy(v).to_string()))
                .collect(),
        })
    }

    async fn delete_object(&self, bucket: &str, object: &str) -> MinioResult<()> {
        let mod_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as i64;

        // Delete the data file so disk space is reclaimed.
        let _ = self.disk.delete(bucket, &Self::data_path(object)).await;

        let version_id = uuid::Uuid::now_v7().to_string();
        let mut header = XlMetaVersionHeader::new(version_id.clone());
        header.mod_time = mod_time;
        header.r#type = VersionType::Delete as u8;
        header.signature = header.compute_signature()?;

        // Read existing meta, append delete marker
        let mut meta = match self.disk.read_all(bucket, &Self::meta_path(object)).await {
            Ok(bytes) => XlMeta::from_bytes(&bytes)?,
            Err(_) => XlMeta {
                versions: Vec::new(),
            },
        };

        meta.versions.push(XlMetaEntry::Delete {
            version_id,
            mod_time,
            signature: header.signature,
            flags: 0,
        });

        let meta_bytes = meta.to_bytes()?;
        self.disk
            .write_all(bucket, &Self::meta_path(object), &meta_bytes)
            .await?;
        Ok(())
    }

    async fn delete_objects(
        &self,
        bucket: &str,
        objects: &[String],
    ) -> MinioResult<DeleteObjectsResult> {
        let mut deleted = Vec::new();
        let mut errors = Vec::new();
        for key in objects {
            match self.delete_object(bucket, key).await {
                Ok(()) => deleted.push(key.clone()),
                Err(e) => {
                    let (_, code, message) = crate::s3::error::to_s3_error_code(&e);
                    errors.push((key.clone(), code.to_string(), message.to_string()));
                }
            }
        }
        Ok(DeleteObjectsResult { deleted, errors })
    }

    async fn list_objects(
        &self,
        bucket: &str,
        prefix: &str,
        delimiter: &str,
        max_keys: usize,
        start_after: Option<&str>,
        continuation_token: Option<&str>,
    ) -> MinioResult<ListObjectsResult> {
        // Resolve the cursor: continuation_token takes precedence over start_after
        let marker = continuation_token
            .and_then(|t| {
                use base64::Engine;
                base64::engine::general_purpose::STANDARD
                    .decode(t)
                    .ok()
                    .and_then(|b| String::from_utf8(b).ok())
            })
            .or_else(|| start_after.map(|s| s.to_string()))
            .unwrap_or_default();

        // Read a generous buffer to support pagination filtering
        let buffer_size = if max_keys > 0 {
            (max_keys + 1) * 2
        } else {
            2000
        };
        let effective_max = if max_keys == 0 { 1000 } else { max_keys };

        let mut entries = self.disk.list_dir(bucket, prefix, buffer_size).await?;
        // S3 requires lexicographic ordering; filesystem read_dir order is not guaranteed
        entries.sort();
        let mut objects = Vec::new();
        let mut prefixes = Vec::new();
        let mut last_key = String::new();
        let mut more = false;

        for entry in &entries {
            let obj_path = format!("{prefix}{entry}");

            // Skip entries at or before the marker
            if !marker.is_empty() && obj_path.as_str() <= marker.as_str() {
                continue;
            }

            // Stop collecting once we have enough items
            if objects.len() + prefixes.len() >= effective_max {
                more = true;
                break;
            }

            let meta_path = format!("{obj_path}/xl.meta");
            match self.disk.stat_file(bucket, &meta_path).await {
                Ok(stat) if !stat.is_dir => {
                    if let Ok(bytes) = self.disk.read_all(bucket, &meta_path).await {
                        if let Ok(meta) = XlMeta::from_bytes(&bytes) {
                            let is_deleted = meta
                                .versions
                                .iter()
                                .rev()
                                .next()
                                .map(|v| matches!(v, XlMetaEntry::Delete { .. }))
                                .unwrap_or(false);
                            if is_deleted {
                                continue;
                            }
                            if let Some(XlMetaEntry::Object { header, .. }) = meta
                                .versions
                                .iter()
                                .rev()
                                .find(|v| matches!(v, XlMetaEntry::Object { .. }))
                            {
                                last_key = obj_path.clone();
                                objects.push(ObjectInfo {
                                    bucket: bucket.to_string(),
                                    name: obj_path,
                                    version_id: header.version_id.clone(),
                                    size: header.parts.iter().map(|p| p.actual_size).sum(),
                                    etag: header
                                        .parts
                                        .first()
                                        .map_or(String::new(), |p| p.etag.clone()),
                                    mod_time: header.mod_time,
                                    content_type: "application/octet-stream".into(),
                                    user_metadata: Vec::new(),
                                });
                                continue;
                            }
                        }
                    }
                    // Fallback
                    last_key = obj_path.clone();
                    objects.push(ObjectInfo {
                        bucket: bucket.to_string(),
                        name: obj_path,
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

        // Also set more=true if the output count equals effective_max (can't know without reading more)
        // The conservative approach: when we exhausted all entries, check if we hit the limit
        if !more {
            more = objects.len() + prefixes.len() >= effective_max;
        }

        let next_token = if more && !last_key.is_empty() {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.encode(last_key.as_bytes())
        } else {
            String::new()
        };

        prefixes.sort();

        Ok(ListObjectsResult {
            objects,
            common_prefixes: prefixes,
            is_truncated: more,
            next_continuation_token: next_token,
        })
    }
}
