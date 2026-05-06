//! axum Router construction — maps S3 operations to handlers

use std::sync::Arc;

use axum::{
    extract::DefaultBodyLimit,
    routing::{delete, get, head, put},
    Router,
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::s3::handlers::bucket::{bucket_exists_handler, create_bucket_handler, delete_bucket_handler};
use crate::s3::handlers::list::list_objects_v2_handler;
use crate::s3::handlers::object::{
    delete_object_handler, get_object_handler, head_object_handler, put_object_handler,
};
use crate::s3::handlers::service::list_buckets_handler;
use crate::s3::state::AppState;

/// Build the S3 HTTP router.
///
/// The returned `Router` has all Phase 1 routes registered and is already
/// configured with the provided `AppState`.  Middleware (CORS, tracing) is
/// applied to the entire router.
///
/// ## Missing endpoints (TODOs for future phases)
///
/// - `POST /:bucket?delete` → DeleteObjects (multi-object delete)
/// - `PUT /:bucket/*key` with `x-amz-copy-source` → CopyObject
/// - `POST /:bucket/*key?uploads` → CreateMultipartUpload
/// - `PUT /:bucket/*key?partNumber=&uploadId=` → UploadPart
/// - `POST /:bucket/*key?uploadId=` → CompleteMultipartUpload
/// - `DELETE /:bucket/*key?uploadId=` → AbortMultipartUpload
/// - `GET /:bucket?acl` → GetBucketAcl
/// - `PUT /:bucket?acl` → PutBucketAcl
/// - `GET /:bucket?policy` → GetBucketPolicy
/// - `PUT /:bucket?policy` → PutBucketPolicy
/// - `DELETE /:bucket?policy` → DeleteBucketPolicy
/// - `GET /:bucket?location` → GetBucketLocation
/// - `GET /:bucket?versioning` → GetBucketVersioning
/// - `PUT /:bucket?versioning` → PutBucketVersioning
/// - `GET /:bucket?tagging` → GetBucketTagging
/// - `PUT /:bucket?tagging` → PutBucketTagging
/// - `DELETE /:bucket?tagging` → DeleteBucketTagging
/// - `GET /:bucket/*key?acl` → GetObjectAcl
/// - `PUT /:bucket/*key?acl` → PutObjectAcl
/// - `GET /:bucket/*key?tagging` → GetObjectTagging
/// - `PUT /:bucket/*key?tagging` → PutObjectTagging
/// - `DELETE /:bucket/*key?tagging` → DeleteObjectTagging
/// - `GET /:bucket/*key?legal-hold` → GetObjectLegalHold
/// - `PUT /:bucket/*key?legal-hold` → PutObjectLegalHold
/// - `GET /:bucket/*key?retention` → GetObjectRetention
/// - `PUT /:bucket/*key?retention` → PutObjectRetention
/// - SigV4 auth middleware (currently all requests accepted without auth)
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
        // 5 GiB body limit — matches MAX_OBJECT_SIZE in put_object_handler
        .layer(DefaultBodyLimit::max(5 * 1024 * 1024 * 1024))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}
