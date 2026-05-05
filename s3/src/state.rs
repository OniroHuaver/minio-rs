//! AppState — shared state injected into all handlers

use std::sync::Arc;

use object::ObjectAPI;

/// Shared application state for the S3 HTTP layer.
pub struct AppState {
    pub object_api: Arc<dyn ObjectAPI>,
    pub instance_id: String,
    pub region: String,
}
