//! axum Router construction — maps S3 operations to handlers
//!
//! The router is split in two parts:
//! - **Metrics router**: `/minio/metrics/v3*` — no API counting middleware.
//! - **S3 API router**: all other routes — with `metrics_middleware` to
//!   populate the `/api/requests` metric group.
//!
//! The two are merged, then wrapped with common layers (SigV4, body-limit,
//! tracing, CORS).

use std::sync::Arc;

use axum::{
    extract::DefaultBodyLimit,
    middleware,
    routing::{delete, get, head, post, put},
    Router,
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::metrics::{metrics_handler, metrics_middleware};
use crate::s3::auth::sigv4_middleware;
use crate::s3::handlers::bucket::{
    bucket_exists_handler, bucket_get_handler, bucket_put_handler, delete_bucket_handler,
};
use crate::s3::handlers::delete::delete_objects_handler;
use crate::s3::handlers::multipart::multipart_post_handler;
use crate::s3::handlers::object::{
    delete_object_handler, get_object_handler, head_object_handler, put_object_handler,
};
use crate::s3::handlers::service::list_buckets_handler;
use crate::s3::state::AppState;

/// Build the S3 HTTP router.
pub fn router(state: Arc<AppState>) -> Router {
    // ── Metrics routes (no API counting, separate auth control) ──────────
    let metrics_router = Router::new()
        .route("/minio/metrics/v3", get(metrics_handler))
        .route("/minio/metrics/v3/*path", get(metrics_handler));

    // ── S3 API routes (with request counting middleware) ─────────────────
    let s3_router = Router::new()
        // Service: ListBuckets
        .route("/", get(list_buckets_handler))
        // Bucket operations
        .route("/:bucket", get(bucket_get_handler))
        .route("/:bucket", put(bucket_put_handler))
        .route("/:bucket", head(bucket_exists_handler))
        .route("/:bucket", post(delete_objects_handler))
        .route("/:bucket", delete(delete_bucket_handler))
        // Object operations
        .route("/:bucket/*key", put(put_object_handler))
        .route("/:bucket/*key", get(get_object_handler))
        .route("/:bucket/*key", head(head_object_handler))
        .route("/:bucket/*key", delete(delete_object_handler))
        // Multipart upload
        .route("/:bucket/*key", post(multipart_post_handler))
        // API request counting middleware (S3 routes only)
        .layer(middleware::from_fn_with_state(
            state.http_stats.clone(),
            metrics_middleware,
        ));

    // ── Merge and apply common layers ────────────────────────────────────
    metrics_router
        .merge(s3_router)
        .layer(middleware::from_fn_with_state(state.clone(), sigv4_middleware))
        // 5 GiB body limit
        .layer(DefaultBodyLimit::max(5 * 1024 * 1024 * 1024))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}
