//! storage: 存储抽象层
//!
//! 对应 Go: cmd/storage-interface.go + cmd/xl-storage*.go
//!
//! ## 架构
//!
//! ```text
//! StorageAPI (trait)
//!   ├── xlStorage      (本地磁盘驱动)
//!   └── storageClient  (远程磁盘 RPC, Phase 2)
//! ```

use base::error::MinioResult;

// 子模块
pub mod format;
pub mod xl_storage;

#[cfg(test)]
mod tests;

// 类型重导出
pub use format::{
    calculate_part_size_from_idx, hash_deterministic_string, is_xl_meta_erasure_info_valid,
    is_xl_meta_format_valid, read_xl_meta, write_xl_meta, write_xl_meta_no_data, ChecksumInfo,
    ErasureInfo, ObjectPartInfo, StatInfo, XlMetaDataDirDecoder, XlMetaV1Object,
    XlMetaV2DeleteMarker, XlMetaV2Object, XlMetaV2Version, XlMetaV2VersionHeader,
};
pub use xl_storage::XlStorage;

/// 磁盘信息
#[derive(Debug, Clone)]
pub struct DiskInfo {
    /// 总容量 (字节)
    pub total: u64,
    /// 可用容量 (字节)
    pub free: u64,
    /// 已用容量 (字节)
    pub used: u64,
    /// 磁盘挂载点
    pub mount_path: String,
    /// 是否在线
    pub online: bool,
    /// 是否经过格式化 (存在 .minio.sys/format.json)
    pub formatted: bool,
    /// 修复中状态
    pub healing: bool,
    /// 磁盘端点
    pub endpoint: String,
}

/// StorageAPI trait — 磁盘级 IO 操作的统一抽象
///
/// 对应 Go: `cmd/storage-interface.go StorageAPI`
///
/// 本地磁盘由 `xlStorage` 实现，远程磁盘由 `storageRESTClient` 实现。
/// 上层 (erasureObjects) 不感知底层是本地还是远程。
#[async_trait::async_trait]
pub trait StorageAPI: Send + Sync {
    // ---- 磁盘基础 ----

    /// 返回磁盘信息
    async fn disk_info(&self) -> MinioResult<DiskInfo>;

    /// 检查磁盘是否在线
    async fn is_online(&self) -> bool;

    /// 磁盘端点标识
    fn endpoint(&self) -> &str;

    // ---- 文件 IO ----

    /// 读取文件的全部内容
    async fn read_all(&self, volume: &str, path: &str) -> MinioResult<Vec<u8>>;

    /// 读取文件的指定范围
    async fn read_range(
        &self,
        volume: &str,
        path: &str,
        offset: i64,
        length: i64,
    ) -> MinioResult<Vec<u8>>;

    /// 写入文件 (覆盖)
    async fn write_all(&self, volume: &str, path: &str, data: &[u8]) -> MinioResult<()>;

    /// 追加写入文件
    async fn append_file(&self, volume: &str, path: &str, data: &[u8]) -> MinioResult<()>;

    /// 删除文件
    async fn delete(&self, volume: &str, path: &str) -> MinioResult<()>;

    /// 原子 Rename (跨目录移动)
    async fn rename(
        &self,
        src_volume: &str,
        src_path: &str,
        dst_volume: &str,
        dst_path: &str,
    ) -> MinioResult<()>;

    // ---- 目录操作 ----

    /// 列出目录内容
    async fn list_dir(
        &self,
        volume: &str,
        dir_path: &str,
        count: usize,
    ) -> MinioResult<Vec<String>>;

    /// 创建目录 (递归)
    async fn make_volume(&self, volume: &str) -> MinioResult<()>;

    /// 删除目录 (递归)
    async fn delete_volume(&self, volume: &str) -> MinioResult<()>;

    // ---- 统计 ----

    /// 获取文件状态
    async fn stat_file(&self, volume: &str, path: &str) -> MinioResult<FileStat>;

    /// 检查文件是否存在
    async fn file_exists(&self, volume: &str, path: &str) -> MinioResult<bool>;
}

/// 文件状态信息
#[derive(Debug, Clone)]
pub struct FileStat {
    pub size: i64,
    pub mod_time: i64,
    pub is_dir: bool,
}
