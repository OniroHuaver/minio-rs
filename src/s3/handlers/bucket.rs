//! Bucket-level handlers: CreateBucket, DeleteBucket, HeadBucket, GetBucketLocation

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use uuid::Uuid;

use crate::s3::error::to_s3_error_code;
use crate::s3::handlers::list::list_objects_v2_handler;
use crate::s3::response::{s3_error_response, s3_xml_response, LocationConstraintResult, S3_XMLNS};
use crate::s3::state::AppState;

/// Dispatcher for `PUT /:bucket` — dispatches ?versioning or CreateBucket.
pub async fn bucket_put_handler(
    State(state): State<Arc<AppState>>,
    Path(bucket): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    body: Bytes,
) -> Response {
    if params.contains_key("versioning") {
        return crate::s3::handlers::versioning::put_bucket_versioning_handler(
            State(state),
            Path(bucket),
            body,
        )
        .await;
    }
    create_bucket_handler(State(state), Path(bucket)).await
}

pub async fn create_bucket_handler(
    State(state): State<Arc<AppState>>,
    Path(bucket): Path<String>,
) -> Response {
    let request_id = Uuid::new_v4().to_string();
    let resource = format!("/{}", bucket);

    match state.object_api.make_bucket(&bucket).await {
        Ok(()) => {
            tracing::debug!("create_bucket: {}", bucket);
            let mut headers = HeaderMap::new();
            headers.insert(header::LOCATION, HeaderValue::from_str(&resource).unwrap());
            (StatusCode::OK, headers, String::new()).into_response()
        }
        Err(e) => {
            let (status, code, message) = to_s3_error_code(&e);
            tracing::error!("create_bucket failed: {}: {}", bucket, e);
            s3_error_response(status, code, message, &request_id, &resource).into_response()
        }
    }
}

pub async fn delete_bucket_handler(
    State(state): State<Arc<AppState>>,
    Path(bucket): Path<String>,
) -> Response {
    let request_id = Uuid::new_v4().to_string();
    let resource = format!("/{}", bucket);

    match state.object_api.delete_bucket(&bucket).await {
        Ok(()) => {
            tracing::debug!("delete_bucket: {}", bucket);
            (StatusCode::NO_CONTENT, HeaderMap::new(), String::new()).into_response()
        }
        Err(e) => {
            let (status, code, message) = to_s3_error_code(&e);
            tracing::error!("delete_bucket failed: {}: {}", bucket, e);
            s3_error_response(status, code, message, &request_id, &resource).into_response()
        }
    }
}

pub async fn bucket_exists_handler(
    State(state): State<Arc<AppState>>,
    Path(bucket): Path<String>,
) -> Response {
    let request_id = Uuid::new_v4().to_string();
    let resource = format!("/{}", bucket);

    match state.object_api.bucket_exists(&bucket).await {
        Ok(true) => {
            tracing::debug!("head_bucket: {} exists", bucket);
            (StatusCode::OK, HeaderMap::new(), String::new()).into_response()
        }
        Ok(false) => {
            let (status, code, message) =
                to_s3_error_code(&crate::base::error::MinioError::BucketNotFound(bucket));
            s3_error_response(status, code, message, &request_id, &resource).into_response()
        }
        Err(e) => {
            let (status, code, message) = to_s3_error_code(&e);
            tracing::error!("head_bucket failed: {}: {}", bucket, e);
            s3_error_response(status, code, message, &request_id, &resource).into_response()
        }
    }
}

/// Dispatcher for `GET /:bucket` — routes to the correct handler based on query params.
pub async fn bucket_get_handler(
    State(state): State<Arc<AppState>>,
    Path(bucket): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    if params.contains_key("location") {
        return get_bucket_location_handler(State(state), Path(bucket)).await;
    }
    if params.contains_key("versioning") {
        return crate::s3::handlers::versioning::get_bucket_versioning_handler(State(state), Path(bucket)).await;
    }
    // Fallback: ListObjectsV2
    list_objects_v2_handler(State(state), Path(bucket), Query(params)).await
}

/// `GET /:bucket?location` — GetBucketLocation
async fn get_bucket_location_handler(
    State(state): State<Arc<AppState>>,
    Path(_bucket): Path<String>,
) -> Response {
    let location = LocationConstraintResult {
        xmlns: S3_XMLNS.to_string(),
        location: state.region.clone(),
    };
    s3_xml_response(&location).into_response()
}
