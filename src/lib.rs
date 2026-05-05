//! minio-rs — S3-compatible object storage server
//!
//! Single-crate architecture with all modules under `src/`.

pub mod base;
pub mod erasure;
pub mod grid;
pub mod iam;
pub mod object;
pub mod s3;
pub mod server;
pub mod storage;

// Re-exports
pub use crate::server::ServerConfig;

/// Server version
pub const VERSION: &str = "DEVELOPMENT.GOGET";
