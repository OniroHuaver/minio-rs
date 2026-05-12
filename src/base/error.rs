//! Unified error type
//!
//! Covers all error variants across storage, EC, object, and HTTP layers.

use thiserror::Error;

/// MinIO core error enum
#[derive(Error, Debug)]
pub enum MinioError {
    // ---- Storage Layer ----
    #[error("Disk IO error: {0}")]
    DiskIO(#[from] std::io::Error),

    #[error("Disk not found: {0}")]
    DiskNotFound(String),

    #[error("Corrupted disk: {0}")]
    CorruptedDisk(String),

    // ---- xl.meta Format ----
    #[error("xl.meta format error: {0}")]
    XlMetaFormat(String),

    #[error("MessagePack serialization error: {0}")]
    MessagePack(String),

    // ---- Erasure Coding ----
    #[error("EC encode failed: {0}")]
    EncodeError(String),

    #[error("EC decode failed: {0}")]
    DecodeError(String),

    #[error("Insufficient read quorum: need {required}, got {actual}")]
    InsufficientReadQuorum { required: usize, actual: usize },

    #[error("Insufficient write quorum: need {required}, got {actual}")]
    InsufficientWriteQuorum { required: usize, actual: usize },

    // ---- Object Operations ----
    #[error("Object not found: {0}")]
    ObjectNotFound(String),

    #[error("Bucket not found: {0}")]
    BucketNotFound(String),

    #[error("Object already exists: {0}")]
    ObjectAlreadyExists(String),

    #[error("Bucket already exists: {0}")]
    BucketAlreadyExists(String),

    // ---- Multipart Upload ----
    #[error("NoSuchUpload: {0}")]
    NoSuchUpload(String),

    #[error("InvalidPart: {0}")]
    InvalidPart(String),

    #[error("EntityTooSmall: minimum part size is 5 MiB")]
    EntityTooSmall,

    #[error("Checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    // ---- Authentication/Authorization ----
    #[error("Invalid signature: {0}")]
    InvalidSignature(String),

    #[error("Expired credentials: {0}")]
    ExpiredCredentials(String),

    #[error("Access denied: {0}")]
    AccessDenied(String),

    // ---- Server Startup ----
    #[error("Port already in use: {0}")]
    PortInUse(String),

    // ---- Internal Errors ----
    #[error("Internal error: {0}")]
    Internal(String),
}

// std::io::Error does not implement PartialEq; manually implemented to support test assertions
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
            (Self::BucketAlreadyExists(a), Self::BucketAlreadyExists(b)) => a == b,
            (Self::NoSuchUpload(a), Self::NoSuchUpload(b)) => a == b,
            (Self::InvalidPart(a), Self::InvalidPart(b)) => a == b,
            (Self::EntityTooSmall, Self::EntityTooSmall) => true,
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
            (Self::PortInUse(a), Self::PortInUse(b)) => a == b,
            (Self::Internal(a), Self::Internal(b)) => a == b,
            _ => false,
        }
    }
}

/// MinIO Result type alias
pub type MinioResult<T> = Result<T, MinioError>;
