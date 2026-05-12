//! S3-compatible HTTP API layer

pub mod auth;
pub mod error;
pub mod handlers;
pub mod request;
pub mod response;
pub mod router;
pub mod state;

pub use router::router;
pub use state::AppState;
