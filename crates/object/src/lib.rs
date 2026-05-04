//! object: 对象操作编排层
//!
//! 对应 Go: cmd/object-api-interface.go + cmd/erasure-server-pool.go
//!
//! 本 crate 是存储系统的业务逻辑核心：
//! - `ObjectAPI` trait: 对象级操作接口 (PUT/GET/DELETE/LIST)
//! - `erasureObjects`: 基于 EC 的对象存储实现
//! - `erasureServerPools`: 多池路由 (Phase 2)

use base::error::MinioResult;

/// 对象信息
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

/// Multipart 上传信息
#[derive(Debug, Clone)]
pub struct MultipartInfo {
    pub upload_id: String,
    pub bucket: String,
    pub object: String,
    pub initiated: i64,
}

/// ObjectAPI trait — 对象级操作的统一抽象
///
/// 对应 Go: `cmd/object-api-interface.go ObjectLayer`
#[async_trait::async_trait]
pub trait ObjectAPI: Send + Sync {
    // ---- Bucket 操作 ----

    async fn make_bucket(&self, bucket: &str) -> MinioResult<()>;
    async fn delete_bucket(&self, bucket: &str) -> MinioResult<()>;
    async fn list_buckets(&self) -> MinioResult<Vec<String>>;
    async fn bucket_exists(&self, bucket: &str) -> MinioResult<bool>;

    // ---- Object 操作 ----

    /// PUT 对象
    async fn put_object(
        &self,
        bucket: &str,
        object: &str,
        data: &[u8],
        metadata: &[(String, String)],
    ) -> MinioResult<ObjectInfo>;

    /// GET 对象 (返回完整数据)
    async fn get_object(
        &self,
        bucket: &str,
        object: &str,
    ) -> MinioResult<(Vec<u8>, ObjectInfo)>;

    /// GET 对象 (范围读取)
    async fn get_object_range(
        &self,
        bucket: &str,
        object: &str,
        offset: i64,
        length: i64,
    ) -> MinioResult<(Vec<u8>, ObjectInfo)>;

    /// HEAD 对象 (仅元数据)
    async fn stat_object(&self, bucket: &str, object: &str) -> MinioResult<ObjectInfo>;

    /// DELETE 对象
    async fn delete_object(&self, bucket: &str, object: &str) -> MinioResult<()>;

    /// LIST 对象 (V2)
    async fn list_objects(
        &self,
        bucket: &str,
        prefix: &str,
        delimiter: &str,
        max_keys: usize,
    ) -> MinioResult<ListObjectsResult>;

    // ---- Multipart Upload (Phase 1 可选) ----

    // async fn new_multipart_upload(...);
    // async fn put_object_part(...);
    // async fn complete_multipart_upload(...);
    // async fn abort_multipart_upload(...);
}

/// ListObjects 结果
#[derive(Debug, Clone)]
pub struct ListObjectsResult {
    pub objects: Vec<ObjectInfo>,
    pub common_prefixes: Vec<String>,
    pub is_truncated: bool,
    pub next_marker: String,
}

// ---- 测试模块 ----
// 对应 Go cmd/ 下的 *_test.go 测试文件迁移
// 所有测试函数均带有 #[ignore] 标记，待对应类型就绪后启用
#[cfg(test)]
mod tests {
    //! 对象操作测试套件
    //!
    //! 模块组织:
    //! - `object_api`: 核心 ObjectAPI 操作(PUT/GET/DELETE/LIST/Multipart)
    //! - `utils`:      工具函数(校验、元数据、压缩)
    //! - `handlers`:   HTTP handler 层测试
    //! - `bucket`:     Bucket 级操作(handler/策略/加密/复制)
    //! - `lifecycle`:  生命周期配置解析和评估
    //! - `replication`: 复制配置解析
    //! - `encryption`:  加密(SSE-C/SSE-S3/ETag解密/范围读取)
    //! - `lock`:        命名空间锁和本地锁
    //! - `object_lock`: 对象锁定(保留/法律保留)
    //! - `batch`:       批量作业(过期/复制/轮转)
    //! - `data_usage`:  数据使用扫描和缓存
    //! - `copy_part`:   Copy part 范围解析
    //! - `lambda`:      Object Lambda handler
    //! - `versioning`:  版本控制配置
    //! - `bandwidth`:   复制带宽监控
}
