//! S3 error code mapping from MinioError

use axum::http::StatusCode;
use crate::base::error::MinioError;

/// Map a MinioError to (HTTP status, S3 error code, human-readable message).
pub fn to_s3_error_code(err: &MinioError) -> (StatusCode, &'static str, &'static str) {
    match err {
        MinioError::DiskIO(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "InternalError", "Internal server error")
        }
        MinioError::DiskNotFound(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "InternalError", "Internal server error")
        }
        MinioError::CorruptedDisk(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "InternalError", "Internal server error")
        }
        MinioError::XlMetaFormat(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "InternalError", "Internal server error")
        }
        MinioError::MessagePack(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "InternalError", "Internal server error")
        }
        MinioError::EncodeError(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "InternalError", "Internal server error")
        }
        MinioError::DecodeError(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "InternalError", "Internal server error")
        }
        MinioError::InsufficientReadQuorum { .. } => {
            (StatusCode::SERVICE_UNAVAILABLE, "ServiceUnavailable", "Service Unavailable")
        }
        MinioError::InsufficientWriteQuorum { .. } => {
            (StatusCode::SERVICE_UNAVAILABLE, "ServiceUnavailable", "Service Unavailable")
        }
        MinioError::ObjectNotFound(_) => {
            (StatusCode::NOT_FOUND, "NoSuchKey", "The specified key does not exist.")
        }
        MinioError::BucketNotFound(_) => {
            (StatusCode::NOT_FOUND, "NoSuchBucket", "The specified bucket does not exist.")
        }
        MinioError::ObjectAlreadyExists(_) => {
            (StatusCode::CONFLICT, "ObjectAlreadyExists", "The specified object already exists.")
        }
        MinioError::BucketAlreadyExists(_) => {
            (StatusCode::CONFLICT, "BucketAlreadyExists", "The requested bucket name is not available.")
        }
        MinioError::NoSuchUpload(_) => {
            (StatusCode::NOT_FOUND, "NoSuchUpload", "The specified multipart upload does not exist.")
        }
        MinioError::InvalidPart(_) => {
            (StatusCode::BAD_REQUEST, "InvalidPart", "One or more of the specified parts could not be found.")
        }
        MinioError::EntityTooSmall => {
            (StatusCode::BAD_REQUEST, "EntityTooSmall", "Your proposed upload is smaller than the minimum allowed object size.")
        }
        MinioError::ChecksumMismatch { .. } => {
            (StatusCode::BAD_REQUEST, "BadDigest", "The Content-MD5 you specified did not match what we received.")
        }
        MinioError::InvalidSignature(_) => {
            (StatusCode::FORBIDDEN, "SignatureDoesNotMatch", "The request signature we calculated does not match the signature you provided.")
        }
        MinioError::ExpiredCredentials(_) => {
            (StatusCode::FORBIDDEN, "ExpiredToken", "The provided token has expired.")
        }
        MinioError::AccessDenied(_) => {
            (StatusCode::FORBIDDEN, "AccessDenied", "Access Denied.")
        }
        MinioError::PortInUse(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "InternalError", "Internal server error")
        }
        MinioError::Internal(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "InternalError", "Internal server error")
        }
    }
}
