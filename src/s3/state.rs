//! AppState — shared state injected into all handlers

use std::sync::Arc;

use crate::metrics::HttpStats;
use crate::metrics::MetricsRegistry;
use crate::object::ObjectAPI;

/// Shared application state for the S3 HTTP layer.
pub struct AppState {
    pub object_api: Arc<dyn ObjectAPI>,
    pub instance_id: String,
    pub region: String,
    /// (access_key, secret_key) for SigV4 auth.
    /// When None, auth is disabled (anonymous access).
    pub credentials: Option<(String, String)>,
    /// Metrics V3 registry for `/minio/metrics/v3/*`.
    pub metrics: Arc<MetricsRegistry>,
    /// HTTP request counters populated by the metrics middleware.
    pub http_stats: Arc<HttpStats>,
    /// When true, `/minio/metrics/v3` endpoints skip SigV4 auth
    /// (controlled by `MINIO_PROMETHEUS_AUTH_TYPE=public`).
    pub prometheus_auth_public: bool,
}
