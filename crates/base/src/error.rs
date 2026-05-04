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

// std::io::Error 不实现 PartialEq，手动实现以支持测试断言
impl PartialEq for MinioError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::DiskIO(a), Self::DiskIO(b)) => a.kind() == b.kind(),
            (Self::DiskNotFound(a), Self::DiskNotFound(b)) => a == b,
            (Self::CorruptedDisk(a), Self::CorruptedDisk(b)) => a == b,
            (Self::XlMetaFormat(a), Self::XlMetaFormat(b)) => a == b,
            (Self::MessagePack(a), Self::MessagePack(b)) => a == b,
            (Self::EncodeError(a), Self::EncodeError(b)) => a == b,
            (Self::DecodeError(a), Self::DecodeError(b)) => a == b,
            (
                Self::InsufficientReadQuorum {
                    required: r1,
                    actual: a1,
                },
                Self::InsufficientReadQuorum {
                    required: r2,
                    actual: a2,
                },
            ) => r1 == r2 && a1 == a2,
            (
                Self::InsufficientWriteQuorum {
                    required: r1,
                    actual: a1,
                },
                Self::InsufficientWriteQuorum {
                    required: r2,
                    actual: a2,
                },
            ) => r1 == r2 && a1 == a2,
            (Self::ObjectNotFound(a), Self::ObjectNotFound(b)) => a == b,
            (Self::BucketNotFound(a), Self::BucketNotFound(b)) => a == b,
            (Self::ObjectAlreadyExists(a), Self::ObjectAlreadyExists(b)) => a == b,
            (
                Self::ChecksumMismatch {
                    expected: e1,
                    actual: a1,
                },
                Self::ChecksumMismatch {
                    expected: e2,
                    actual: a2,
                },
            ) => e1 == e2 && a1 == a2,
            (Self::InvalidSignature(a), Self::InvalidSignature(b)) => a == b,
            (Self::ExpiredCredentials(a), Self::ExpiredCredentials(b)) => a == b,
            (Self::AccessDenied(a), Self::AccessDenied(b)) => a == b,
            (Self::Internal(a), Self::Internal(b)) => a == b,
            _ => false,
        }
    }
}

/// MinIO Result 类型别名
pub type MinioResult<T> = Result<T, MinioError>;
