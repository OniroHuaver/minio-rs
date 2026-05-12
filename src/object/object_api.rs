//! ObjectAPI trait and related type definitions

use crate::base::error::MinioResult;

/// Object information
#[derive(Debug, Clone)]
pub struct ObjectInfo {
    pub bucket: String,
    pub name: String,
    pub version_id: String,
    pub size: i64,
    pub etag: String,
    pub mod_time: i64,
    pub content_type: String,
    pub user_metadata: Vec<(String, String)>,
}

/// Metadata handling directive for CopyObject.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MetadataDirective {
    /// Preserve source object metadata.
    Copy,
    /// Replace with new metadata.
    Replace,
}

/// Multipart upload information
#[derive(Debug, Clone)]
pub struct MultipartInfo {
    pub upload_id: String,
    pub bucket: String,
    pub object: String,
    pub initiated: i64,
}

/// ListObjects result
#[derive(Debug, Clone)]
pub struct ListObjectsResult {
    pub objects: Vec<ObjectInfo>,
    pub common_prefixes: Vec<String>,
    pub is_truncated: bool,
    /// Base64-encoded next key for continuation-token pagination.
    pub next_continuation_token: String,
}

/// Result of a multi-object delete operation.
#[derive(Debug, Clone)]
pub struct DeleteObjectsResult {
    /// Keys that were successfully deleted.
    pub deleted: Vec<String>,
    /// Keys that failed to delete, with (key, code, message).
    pub errors: Vec<(String, String, String)>,
}

/// ObjectAPI trait — unified abstraction for object-level operations
#[async_trait::async_trait]
pub trait ObjectAPI: Send + Sync {
    // ---- Bucket operations ----

    async fn make_bucket(&self, bucket: &str) -> MinioResult<()>;
    async fn delete_bucket(&self, bucket: &str) -> MinioResult<()>;
    async fn list_buckets(&self) -> MinioResult<Vec<String>>;
    async fn bucket_exists(&self, bucket: &str) -> MinioResult<bool>;

    // ---- Object operations ----

    /// PUT object
    async fn put_object(
        &self,
        bucket: &str,
        object: &str,
        data: &[u8],
        metadata: &[(String, String)],
    ) -> MinioResult<ObjectInfo>;

    /// GET object (returns full data)
    async fn get_object(
        &self,
        bucket: &str,
        object: &str,
    ) -> MinioResult<(Vec<u8>, ObjectInfo)>;

    /// GET object (range read)
    async fn get_object_range(
        &self,
        bucket: &str,
        object: &str,
        offset: i64,
        length: i64,
    ) -> MinioResult<(Vec<u8>, ObjectInfo)>;

    /// HEAD object (metadata only)
    async fn stat_object(&self, bucket: &str, object: &str) -> MinioResult<ObjectInfo>;

    /// DELETE object
    async fn delete_object(&self, bucket: &str, object: &str) -> MinioResult<()>;

    /// LIST objects (V2)
    async fn list_objects(
        &self,
        bucket: &str,
        prefix: &str,
        delimiter: &str,
        max_keys: usize,
        start_after: Option<&str>,
        continuation_token: Option<&str>,
    ) -> MinioResult<ListObjectsResult>;

    /// Delete multiple objects in a single request.
    /// Returns (deleted_keys, errors: Vec<(key, code, message)>).
    async fn delete_objects(
        &self,
        bucket: &str,
        objects: &[String],
    ) -> MinioResult<DeleteObjectsResult>;

    /// Get bucket versioning configuration.
    async fn get_bucket_versioning(&self, bucket: &str) -> MinioResult<Option<VersioningConfig>>;

    /// Set bucket versioning configuration.
    async fn set_bucket_versioning(&self, bucket: &str, status: &str) -> MinioResult<()>;

    /// Copy an object server-side.
    ///
    /// S3: `PUT /{dst-bucket}/{dst-key}` + `x-amz-copy-source` header.
    async fn copy_object(
        &self,
        src_bucket: &str,
        src_object: &str,
        dst_bucket: &str,
        dst_object: &str,
        metadata: &[(String, String)],
        directive: MetadataDirective,
    ) -> MinioResult<ObjectInfo>;

    // ---- Multipart Upload ----

    /// Create a new multipart upload. Returns upload metadata.
    async fn new_multipart_upload(
        &self,
        bucket: &str,
        object: &str,
        metadata: &[(String, String)],
    ) -> MinioResult<MultipartInfo>;

    /// Upload a part. Returns the part ETag (hex string).
    async fn put_object_part(
        &self,
        bucket: &str,
        object: &str,
        upload_id: &str,
        part_number: u32,
        data: &[u8],
    ) -> MinioResult<String>;

    /// Complete a multipart upload. Returns the final ObjectInfo.
    async fn complete_multipart_upload(
        &self,
        bucket: &str,
        object: &str,
        upload_id: &str,
        parts: &[CompletedPart],
    ) -> MinioResult<ObjectInfo>;

    /// Abort a multipart upload.
    async fn abort_multipart_upload(
        &self,
        bucket: &str,
        object: &str,
        upload_id: &str,
    ) -> MinioResult<()>;
}

/// Part descriptor for CompleteMultipartUpload request.
#[derive(Debug, Clone)]
pub struct CompletedPart {
    pub part_number: u32,
    pub etag: String,
}

/// Versioning configuration for a bucket.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct VersioningConfig {
    pub status: VersioningStatus,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub enum VersioningStatus {
    Enabled,
    Suspended,
}

// TODO: ACL / Bucket Policy trait (Phase 3 — IAM/STS)
//
// These operations are currently NOT implemented:
//
// Bucket-level:
//   - GetBucketAcl / PutBucketAcl
//   - GetBucketPolicy / PutBucketPolicy / DeleteBucketPolicy
//   - GetBucketVersioning / PutBucketVersioning
//   - GetBucketLocation
//   - GetBucketTagging / PutBucketTagging / DeleteBucketTagging
//
// Object-level:
//   - GetObjectAcl / PutObjectAcl
//   - GetObjectTagging / PutObjectTagging / DeleteObjectTagging
//   - GetObjectLegalHold / PutObjectLegalHold
//   - GetObjectRetention / PutObjectRetention
//
// Router counterparts are marked with TODO in src/s3/router.rs.
