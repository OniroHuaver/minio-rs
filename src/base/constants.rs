//! Global constant definitions
//!
//! Scattered constants used across the storage system.

/// Small file threshold (128 KiB), files below this size are inlined into xl.meta
pub const SMALL_FILE_THRESHOLD: i64 = 128 * 1024;

/// Large file threshold (128 MiB), files above this size enable PUT read-ahead optimizations
pub const BIG_FILE_THRESHOLD: i64 = 128 * 1024 * 1024;

/// xl.meta format filename
pub const XL_META_FILE: &str = "xl.meta";

/// xl.meta backup filename (recovery from failed atomic rename)
pub const XL_META_BACKUP_FILE: &str = "xl.meta.bkp";

/// xl.meta binary header magic number
pub const XL_HEADER_MAGIC: &[u8; 4] = b"XL2 ";

/// xl.meta format major version
pub const XL_VERSION_MAJOR: u16 = 1;

/// xl.meta format minor version
pub const XL_VERSION_MINOR: u16 = 3;

/// System configuration directory (at disk root)
pub const MINIO_SYS_DIR: &str = ".minio.sys";

/// Temporary file directory
pub const TMP_DIR: &str = "tmp";

/// Multipart upload intermediate state directory
pub const MULTIPART_DIR: &str = "multipart";

/// Legacy V1 format data directory (migrated from xl.json)
pub const LEGACY_DIR: &str = "legacy";

/// Default EC block size (4 MiB)
pub const DEFAULT_BLOCK_SIZE: i64 = 4 * 1024 * 1024;
