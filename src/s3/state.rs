//! AppState — shared state injected into all handlers

use std::sync::Arc;

use crate::object::ObjectAPI;

/// Shared application state for the S3 HTTP layer.
pub struct AppState {
    pub object_api: Arc<dyn ObjectAPI>,
    pub instance_id: String,
    pub region: String,
    /// (access_key, secret_key) for SigV4 auth.
    /// When None, auth is disabled (anonymous access).
    pub credentials: Option<(String, String)>,
}
