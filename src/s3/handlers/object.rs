//! Object-level handlers: PutObject, GetObject, HeadObject, DeleteObject

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use base64::Engine;
use bytes::Bytes;
use uuid::Uuid;

use crate::s3::error::to_s3_error_code;
use crate::s3::request::{extract_metadata, parse_range};
use crate::s3::response::{format_http_timestamp, s3_error_response};
use crate::s3::state::AppState;

/// Maximum object size allowed (5 GiB).
const MAX_OBJECT_SIZE: usize = 5 * 1024 * 1024 * 1024;

pub async fn put_object_handler(
    State(state): State<Arc<AppState>>,
    Path(params): Path<HashMap<String, String>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let request_id = Uuid::new_v4().to_string();
    let bucket = params.get("bucket").cloned().unwrap_or_default();
    let key = params.get("key").cloned().unwrap_or_default();
    let resource = format!("/{}/{}", bucket, key);

    // Validate object size (max 5 GiB)
    if body.len() > MAX_OBJECT_SIZE {
        return s3_error_response(
            StatusCode::BAD_REQUEST,
            "EntityTooLarge",
            "Your proposed upload exceeds the maximum allowed object size.",
            &request_id,
            &resource,
        )
        .into_response();
    }

    // Content-MD5 verification
    if let Some(md5_header) = headers.get("content-md5") {
        if let Ok(md5_str) = md5_header.to_str() {
            if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(md5_str) {
                use md5::{Digest, Md5};
                let mut hasher = Md5::new();
                hasher.update(&body);
                let computed = hasher.finalize().to_vec();
                if computed != decoded {
                    return s3_error_response(
                        StatusCode::BAD_REQUEST,
                        "BadDigest",
                        "The Content-MD5 you specified did not match what we received.",
                        &request_id,
                        &resource,
                    )
                    .into_response();
                }
            }
        }
    }

    let metadata = extract_metadata(&headers);

    match state
        .object_api
        .put_object(&bucket, &key, &body, &metadata)
        .await
    {
        Ok(info) => {
            tracing::debug!("put_object: {}/{} etag={}", bucket, key, info.etag);
            let mut resp_headers = HeaderMap::new();
            resp_headers.insert(
                header::ETAG,
                HeaderValue::from_str(&format!("\"{}\"", info.etag)).unwrap(),
            );
            (StatusCode::OK, resp_headers, String::new()).into_response()
        }
        Err(e) => {
            let (status, code, message) = to_s3_error_code(&e);
            tracing::error!("put_object failed: {}: {}", resource, e);
            s3_error_response(status, code, message, &request_id, &resource).into_response()
        }
    }
}

pub async fn get_object_handler(
    State(state): State<Arc<AppState>>,
    Path(params): Path<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    let request_id = Uuid::new_v4().to_string();
    let bucket = params.get("bucket").cloned().unwrap_or_default();
    let key = params.get("key").cloned().unwrap_or_default();
    let resource = format!("/{}/{}", bucket, key);

    let range_header = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| parse_range(s));

    // If a Range header was parsed, get only that portion.
    let result = if let Some((start, end)) = range_header {
        state
            .object_api
            .get_object_range(&bucket, &key, start, end - start + 1)
            .await
    } else {
        state
            .object_api
            .get_object(&bucket, &key)
            .await
    };

    match result {
        Ok((data, info)) => {
            let mut resp_headers = HeaderMap::new();
            // Content-Type
            resp_headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_str(&info.content_type)
                    .unwrap_or(HeaderValue::from_static("application/octet-stream")),
            );
            // ETag (S3 format: quoted)
            resp_headers.insert(
                header::ETAG,
                HeaderValue::from_str(&format!("\"{}\"", info.etag)).unwrap(),
            );
            // Last-Modified
            resp_headers.insert(
                header::LAST_MODIFIED,
                HeaderValue::from_str(&format_http_timestamp(info.mod_time)).unwrap(),
            );
            // Cache-Control
            resp_headers.insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("no-store"),
            );
            // User metadata (x-amz-meta-*)
            for (k, v) in &info.user_metadata {
                let header_name = format!("x-amz-meta-{}", k);
                if let Ok(name) = axum::http::HeaderName::from_bytes(header_name.as_bytes()) {
                    if let Ok(val) = HeaderValue::from_str(v) {
                        resp_headers.insert(name, val);
                    }
                }
            }

            let status = if range_header.is_some() {
                StatusCode::PARTIAL_CONTENT
            } else {
                StatusCode::OK
            };

            tracing::debug!(
                "get_object: {}/{} status={} size={}",
                bucket,
                key,
                status.as_u16(),
                data.len()
            );
            (status, resp_headers, Bytes::from(data)).into_response()
        }
        Err(e) => {
            let (status, code, message) = to_s3_error_code(&e);
            tracing::error!("get_object failed: {}: {}", resource, e);
            s3_error_response(status, code, message, &request_id, &resource).into_response()
        }
    }
}

pub async fn head_object_handler(
    State(state): State<Arc<AppState>>,
    Path(params): Path<HashMap<String, String>>,
) -> Response {
    let request_id = Uuid::new_v4().to_string();
    let bucket = params.get("bucket").cloned().unwrap_or_default();
    let key = params.get("key").cloned().unwrap_or_default();
    let resource = format!("/{}/{}", bucket, key);

    match state.object_api.stat_object(&bucket, &key).await {
        Ok(info) => {
            let mut resp_headers = HeaderMap::new();
            resp_headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_str(&info.content_type)
                    .unwrap_or(HeaderValue::from_static("application/octet-stream")),
            );
            resp_headers.insert(
                header::ETAG,
                HeaderValue::from_str(&format!("\"{}\"", info.etag)).unwrap(),
            );
            resp_headers.insert(
                header::LAST_MODIFIED,
                HeaderValue::from_str(&format_http_timestamp(info.mod_time)).unwrap(),
            );
            resp_headers.insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("no-store"),
            );
            // User metadata
            for (k, v) in &info.user_metadata {
                let header_name = format!("x-amz-meta-{}", k);
                if let Ok(name) = axum::http::HeaderName::from_bytes(header_name.as_bytes()) {
                    if let Ok(val) = HeaderValue::from_str(v) {
                        resp_headers.insert(name, val);
                    }
                }
            }

            tracing::debug!("head_object: {}/{}", bucket, key);
            (StatusCode::OK, resp_headers, String::new()).into_response()
        }
        Err(e) => {
            let (status, code, message) = to_s3_error_code(&e);
            tracing::error!("head_object failed: {}: {}", resource, e);
            s3_error_response(status, code, message, &request_id, &resource).into_response()
        }
    }
}

pub async fn delete_object_handler(
    State(state): State<Arc<AppState>>,
    Path(params): Path<HashMap<String, String>>,
) -> Response {
    let request_id = Uuid::new_v4().to_string();
    let bucket = params.get("bucket").cloned().unwrap_or_default();
    let key = params.get("key").cloned().unwrap_or_default();
    let resource = format!("/{}/{}", bucket, key);

    match state.object_api.delete_object(&bucket, &key).await {
        Ok(()) => {
            tracing::debug!("delete_object: {}/{}", bucket, key);
            (StatusCode::NO_CONTENT, HeaderMap::new(), String::new()).into_response()
        }
        Err(e) => {
            let (status, code, message) = to_s3_error_code(&e);
            tracing::error!("delete_object failed: {}: {}", resource, e);
            s3_error_response(status, code, message, &request_id, &resource).into_response()
        }
    }
}
