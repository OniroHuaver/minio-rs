//! axum Router construction — maps S3 operations to handlers

use std::sync::Arc;

use axum::{
    routing::{delete, get, head, put},
    Router,
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::handlers::bucket::{bucket_exists_handler, create_bucket_handler, delete_bucket_handler};
use crate::handlers::list::list_objects_v2_handler;
use crate::handlers::object::{
    delete_object_handler, get_object_handler, head_object_handler, put_object_handler,
};
use crate::handlers::service::list_buckets_handler;
use crate::state::AppState;

/// Build the S3 HTTP router.
///
/// The returned `Router` has all Phase 1 routes registered and is already
/// configured with the provided `AppState`.  Middleware (CORS, tracing) is
/// applied to the entire router.
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        // Service: ListBuckets
        .route("/", get(list_buckets_handler))
        // Bucket operations
        .route("/:bucket", get(list_objects_v2_handler))
        .route("/:bucket", put(create_bucket_handler))
        .route("/:bucket", head(bucket_exists_handler))
        .route("/:bucket", delete(delete_bucket_handler))
        // Object operations
        .route("/:bucket/*key", put(put_object_handler))
        .route("/:bucket/*key", get(get_object_handler))
        .route("/:bucket/*key", head(head_object_handler))
        .route("/:bucket/*key", delete(delete_object_handler))
        // Middleware
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}
