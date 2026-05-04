//! 统一错误类型
//!
//! 涵盖存储层、EC 层、对象层、HTTP 层的所有错误变体。

use thiserror::Error;

/// MinIO 核心错误枚举
#[derive(Error, Debug)]
pub enum MinioError {
    // ---- 存储层 ----
    #[error("磁盘 IO 错误: {0}")]
    DiskIO(#[from] std::io::Error),

    #[error("磁盘未找到: {0}")]
    DiskNotFound(String),

    #[error("磁盘格式损坏: {0}")]
    CorruptedDisk(String),

    // ---- xl.meta 格式 ----
    #[error("xl.meta 格式错误: {0}")]
    XlMetaFormat(String),

    #[error("MessagePack 序列化错误: {0}")]
    MessagePack(String),

    // ---- Erasure Coding ----
    #[error("EC 编码失败: {0}")]
    EncodeError(String),

    #[error("EC 解码失败: {0}")]
    DecodeError(String),

    #[error("读取 Quorum 不足: 需要 {required}，实际 {actual}")]
    InsufficientReadQuorum { required: usize, actual: usize },

    #[error("写入 Quorum 不足: 需要 {required}，实际 {actual}")]
    InsufficientWriteQuorum { required: usize, actual: usize },

    // ---- 对象操作 ----
    #[error("对象不存在: {0}")]
    ObjectNotFound(String),

    #[error("Bucket 不存在: {0}")]
    BucketNotFound(String),

    #[error("对象已存在: {0}")]
    ObjectAlreadyExists(String),

    #[error("校验和不匹配: 期望 {expected}, 实际 {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    // ---- 认证授权 ----
    #[error("签名无效: {0}")]
    InvalidSignature(String),

    #[error("凭证过期: {0}")]
    ExpiredCredentials(String),

    #[error("访问拒绝: {0}")]
    AccessDenied(String),

    // ---- 内部错误 ----
    #[error("内部错误: {0}")]
    Internal(String),
}

/// MinIO Result 类型别名
pub type MinioResult<T> = Result<T, MinioError>;
