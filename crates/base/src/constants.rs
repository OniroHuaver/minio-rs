//! 全局常量定义
//!
//! 对应 Go: cmd/ 中的散布常量

/// 小文件阈值 (128 KiB)，小于此值的文件内容内联存储到 xl.meta
pub const SMALL_FILE_THRESHOLD: i64 = 128 * 1024;

/// 大文件阈值 (128 MiB)，大于此值的文件启用 PUT 预读优化
pub const BIG_FILE_THRESHOLD: i64 = 128 * 1024 * 1024;

/// xl.meta 格式文件名
pub const XL_META_FILE: &str = "xl.meta";

/// xl.meta 备份文件名 (原子 rename 失败恢复)
pub const XL_META_BACKUP_FILE: &str = "xl.meta.bkp";

/// xl.meta 二进制头魔数
pub const XL_HEADER_MAGIC: &[u8; 4] = b"XL2 ";

/// xl.meta 格式主版本
pub const XL_VERSION_MAJOR: u16 = 1;

/// xl.meta 格式次版本
pub const XL_VERSION_MINOR: u16 = 3;

/// 系统配置目录 (磁盘根目录下)
pub const MINIO_SYS_DIR: &str = ".minio.sys";

/// 临时文件目录
pub const TMP_DIR: &str = "tmp";

/// Multipart 上传中间态目录
pub const MULTIPART_DIR: &str = "multipart";

/// 传统 V1 格式数据目录 (从 xl.json 迁移)
pub const LEGACY_DIR: &str = "legacy";

/// 默认 EC 块大小 (4 MiB)
pub const DEFAULT_BLOCK_SIZE: i64 = 4 * 1024 * 1024;
