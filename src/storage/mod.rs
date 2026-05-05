//! storage: Storage abstraction layer
//!
//! ## Architecture
//!
//! ```text
//! StorageAPI (trait)
//!   ├── xlStorage      (local disk driver)
//!   └── storageClient  (remote disk RPC, Phase 2)
//! ```

use crate::base::error::MinioResult;

// Sub-modules
pub mod format;
pub mod xl_storage;

#[cfg(test)]
mod test;

// Re-exports
pub use format::{
    calculate_part_size_from_idx, hash_deterministic_string, is_xl_meta_erasure_info_valid,
    is_xl_meta_format_valid, read_xl_meta, write_xl_meta, write_xl_meta_no_data, ChecksumInfo,
    ErasureInfo, ObjectPartInfo, StatInfo, XlMetaDataDirDecoder, XlMetaV1Object,
    XlMetaV2DeleteMarker, XlMetaV2Object, XlMetaV2Version, XlMetaV2VersionHeader,
};
pub use xl_storage::XlStorage;

/// Disk information
#[derive(Debug, Clone)]
pub struct DiskInfo {
    /// Total capacity (bytes)
    pub total: u64,
    /// Free capacity (bytes)
    pub free: u64,
    /// Used capacity (bytes)
    pub used: u64,
    /// Disk mount point
    pub mount_path: String,
    /// Online status
    pub online: bool,
    /// Whether formatted (.minio.sys/format.json exists)
    pub formatted: bool,
    /// Healing status
    pub healing: bool,
    /// Endpoint
    pub endpoint: String,
}

/// StorageAPI trait -- unified abstraction for disk-level I/O operations
///
/// Local disk is implemented by `xlStorage`, remote disk by `storageRESTClient`.
/// The upper layer (erasureObjects) does not know whether the backend is local or remote.
#[async_trait::async_trait]
pub trait StorageAPI: Send + Sync {
    // ---- Disk basics ----

    /// Return disk information
    async fn disk_info(&self) -> MinioResult<DiskInfo>;

    /// Check whether disk is online
    async fn is_online(&self) -> bool;

    /// Disk endpoint identifier
    fn endpoint(&self) -> &str;

    // ---- File I/O ----

    /// Read entire file contents
    async fn read_all(&self, volume: &str, path: &str) -> MinioResult<Vec<u8>>;

    /// Read a byte range from a file
    async fn read_range(
        &self,
        volume: &str,
        path: &str,
        offset: i64,
        length: i64,
    ) -> MinioResult<Vec<u8>>;

    /// Write to file (overwrite)
    async fn write_all(&self, volume: &str, path: &str, data: &[u8]) -> MinioResult<()>;

    /// Append to file
    async fn append_file(&self, volume: &str, path: &str, data: &[u8]) -> MinioResult<()>;

    /// Delete file
    async fn delete(&self, volume: &str, path: &str) -> MinioResult<()>;

    /// Atomic rename (cross-directory move)
    async fn rename(
        &self,
        src_volume: &str,
        src_path: &str,
        dst_volume: &str,
        dst_path: &str,
    ) -> MinioResult<()>;

    // ---- Directory operations ----

    /// List directory contents
    async fn list_dir(
        &self,
        volume: &str,
        dir_path: &str,
        count: usize,
    ) -> MinioResult<Vec<String>>;

    /// Create volume (recursive)
    async fn make_volume(&self, volume: &str) -> MinioResult<()>;

    /// Delete volume (recursive)
    async fn delete_volume(&self, volume: &str) -> MinioResult<()>;

    // ---- Statistics ----

    /// Get file stat
    async fn stat_file(&self, volume: &str, path: &str) -> MinioResult<FileStat>;

    /// Check if file exists
    async fn file_exists(&self, volume: &str, path: &str) -> MinioResult<bool>;
}

/// File stat information
#[derive(Debug, Clone)]
pub struct FileStat {
    pub size: i64,
    pub mod_time: i64,
    pub is_dir: bool,
}
