//! server: MinIO Rust binary entry point (library form)
//!
//! This crate provides both [[bin]] (main.rs) and [lib] (lib.rs) targets,
//! so that integration tests (tests/) can reference the library's public API.
//!
//! Phase 1: module skeleton only; actual functionality is implemented incrementally.

// ============================================================================
// Module declarations
// ============================================================================

/// Endpoint types and endpoint resolution
pub mod endpoint {
    // TODO: Phase 2 - implement Endpoint, Endpoints, ellipsis expansion
}

/// Storage pool layout
pub mod layout {
    // TODO: Phase 2 - storage pool, set partitioning, EC layout
}

/// Network utilities
pub mod net {
    // TODO: Phase 2 - IP sorting, Host:Port parsing, local address detection
}

/// General utility functions
pub mod utils {
    // TODO: Phase 2 - object size validation, path parsing, LCP, ETag, etc.
}

/// Server startup flow and configuration
pub mod server {
    // TODO: Phase 2 - Server startup, global config, startup messages
}

/// Admin handlers
pub mod admin {
    // TODO: Phase 2 - admin API routing and handlers
}

/// Update checking
pub mod update {
    // TODO: Phase 2 - version update check and notification
}

/// OS utilities
pub mod osutil {
    // TODO: Phase 2 - readDir, mkdirAll, renameAll
}

/// ARN type
pub use base::format;

// Version constant
pub const VERSION: &str = "DEVELOPMENT.GOGET";
