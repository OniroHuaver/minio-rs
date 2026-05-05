//! server: MinIO Rust binary entry point (library form)
//!
//! This crate provides both `[[bin]]` (main.rs) and `[lib]` (lib.rs) targets,
//! so that integration tests can reference the library's public API.

// ============================================================================
// Module declarations
// ============================================================================

/// CLI argument parsing (clap derive)
pub mod cmd;

/// Server startup flow: disk check, EC pool init, HTTP serve
pub mod server;

/// Disk path checking and preparation
pub mod disk;

/// Startup banner display
pub mod banner;

// ============================================================================
// Re-exports
// ============================================================================

pub use base::format;
pub use s3::AppState;
pub use crate::server::ServerConfig;

// ============================================================================
// Constants
// ============================================================================

/// Server version
pub const VERSION: &str = "DEVELOPMENT.GOGET";
