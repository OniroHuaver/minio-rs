//! Object-level handlers: PutObject, GetObject, HeadObject, DeleteObject

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::Engine;
use bytes::Bytes;
use uuid::Uuid;

use crate::object::object_api::MetadataDirective;
use crate::s3::error::to_s3_error_code;
use crate::s3::request::{RangeSpec, extract_metadata, parse_range, percent_decode};
use crate::s3::response::{
    CopyObjectResultXml, S3_XMLNS, format_http_timestamp, s3_error_response, s3_xml_response,
};
use crate::s3::state::AppState;

/// Maximum object size allowed (5 GiB).
const MAX_OBJECT_SIZE: usize = 5 * 1024 * 1024 * 1024;

pub async fn put_object_handler(
    State(state): State<Arc<AppState>>,
    Path(params): Path<HashMap<String, String>>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let request_id = Uuid::new_v4().to_string();
    let bucket = params.get("bucket").cloned().unwrap_or_default();
    let key = params.get("key").cloned().unwrap_or_default();
    let resource = format!("/{}/{}", bucket, key);

    // UploadPart: PUT with ?partNumber=N&uploadId=ID
    if query.contains_key("partNumber") && query.contains_key("uploadId") {
        return crate::s3::handlers::multipart::upload_part_handler(
            State(state),
            Path(params),
            Query(query),
            body,
        )
        .await;
    }

    // CopyObject: PUT with x-amz-copy-source header
    if let Some(copy_source) = headers
        .get("x-amz-copy-source")
        .and_then(|v| v.to_str().ok())
    {
        return copy_object_dispatch(state, &bucket, &key, copy_source, &headers, &request_id)
            .await;
    }

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
    let result = match range_header {
        Some(RangeSpec::Bytes { start, end }) => {
            state
                .object_api
                .get_object_range(&bucket, &key, start, end - start + 1)
                .await
        }
        Some(RangeSpec::From { start }) => {
            // Stat first to determine object size, then read from start to end
            match state.object_api.stat_object(&bucket, &key).await {
                Ok(info) => {
                    state
                        .object_api
                        .get_object_range(&bucket, &key, start, info.size - start)
                        .await
                }
                Err(e) => Err(e),
            }
        }
        Some(RangeSpec::Suffix { length }) => {
            // Stat first, then read last N bytes
            match state.object_api.stat_object(&bucket, &key).await {
                Ok(info) => {
                    let start = std::cmp::max(0, info.size - length);
                    state
                        .object_api
                        .get_object_range(&bucket, &key, start, length)
                        .await
                }
                Err(e) => Err(e),
            }
        }
        None => state.object_api.get_object(&bucket, &key).await,
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
            resp_headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
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
            resp_headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
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
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let bucket = params.get("bucket").cloned().unwrap_or_default();
    let key = params.get("key").cloned().unwrap_or_default();

    // AbortMultipartUpload: DELETE /{bucket}/{key}?uploadId=ID
    if let Some(upload_id) = query.get("uploadId") {
        if !upload_id.is_empty() {
            return crate::s3::handlers::multipart::abort_multipart_upload_handler(
                State(state),
                Path(params),
                Query(query),
            )
            .await;
        }
    }

    let request_id = Uuid::new_v4().to_string();
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

/// Handle CopyObject when `x-amz-copy-source` header is present.
async fn copy_object_dispatch(
    state: Arc<AppState>,
    dst_bucket: &str,
    dst_key: &str,
    copy_source: &str,
    headers: &HeaderMap,
    request_id: &str,
) -> Response {
    let resource = format!("/{}/{}", dst_bucket, dst_key);

    // Parse x-amz-copy-source: "/src-bucket/src-key" (both may be percent-encoded)
    let source = copy_source.strip_prefix('/').unwrap_or(copy_source);
    let (src_bucket, src_key) = match source.split_once('/') {
        Some((b, k)) if !b.is_empty() => {
            let bucket_decoded = percent_decode(b);
            let key_decoded = percent_decode(k);
            (bucket_decoded, key_decoded)
        }
        _ => {
            return s3_error_response(
                StatusCode::BAD_REQUEST,
                "InvalidArgument",
                "x-amz-copy-source must be in the format /bucket/key",
                request_id,
                &resource,
            )
            .into_response();
        }
    };

    // Parse x-amz-metadata-directive
    let directive = headers
        .get("x-amz-metadata-directive")
        .and_then(|v| v.to_str().ok())
        .map(|d| d.to_uppercase())
        .and_then(|d| match d.as_str() {
            "REPLACE" => Some(MetadataDirective::Replace),
            "COPY" => Some(MetadataDirective::Copy),
            _ => None,
        })
        .unwrap_or(MetadataDirective::Copy);

    let metadata = extract_metadata(headers);

    match state
        .object_api
        .copy_object(
            &src_bucket,
            &src_key,
            dst_bucket,
            dst_key,
            &metadata,
            directive,
        )
        .await
    {
        Ok(info) => {
            tracing::debug!(
                "copy_object: {}/{} -> {}/{} etag={}",
                src_bucket,
                src_key,
                dst_bucket,
                dst_key,
                info.etag
            );
            let result = CopyObjectResultXml {
                xmlns: S3_XMLNS.to_string(),
                last_modified: format_http_timestamp(info.mod_time),
                etag: format!("\"{}\"", info.etag),
            };
            s3_xml_response(&result).into_response()
        }
        Err(e) => {
            let (status, code, message) = to_s3_error_code(&e);
            tracing::error!(
                "copy_object failed: {}/{} -> {}/{}: {}",
                src_bucket,
                src_key,
                dst_bucket,
                dst_key,
                e
            );
            s3_error_response(status, code, message, request_id, &resource).into_response()
        }
    }
}
