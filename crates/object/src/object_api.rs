//! ObjectAPI trait and related type definitions

use base::error::MinioResult;

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
    pub next_marker: String,
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
    ) -> MinioResult<ListObjectsResult>;

    // ---- Multipart Upload (Phase 1 optional) ----

    // async fn new_multipart_upload(...);
    // async fn put_object_part(...);
    // async fn complete_multipart_upload(...);
    // async fn abort_multipart_upload(...);
}
