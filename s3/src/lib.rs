//! S3-compatible HTTP API layer
//!
//! Phase 1 provides core bucket and object operations over the S3 REST API
//! using path-style addressing.  Authentication (SigV4), multipart uploads,
//! and virtual-hosted style are deferred to later phases.

pub mod error;
pub mod handlers;
pub mod request;
pub mod response;
pub mod router;
pub mod state;

pub use router::router;
pub use state::AppState;
