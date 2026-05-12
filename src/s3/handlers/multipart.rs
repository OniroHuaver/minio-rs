//! Multipart upload handlers: Create, UploadPart, Complete, Abort

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use axum::http::header;
use bytes::Bytes;
use uuid::Uuid;

use crate::object::object_api::CompletedPart;
use crate::s3::error::to_s3_error_code;
use crate::s3::request::{extract_metadata, CompleteMultipartUploadBody};
use crate::s3::response::{
    s3_error_response, s3_xml_response,
    CompleteMultipartUploadResultXml, InitiateMultipartUploadResultXml, S3_XMLNS,
};
use crate::s3::state::AppState;

/// POST handler that dispatches to CreateMultipartUpload or CompleteMultipartUpload
/// based on query parameters.
pub async fn multipart_post_handler(
    State(state): State<Arc<AppState>>,
    Path(params): Path<HashMap<String, String>>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if query.contains_key("uploads") {
        return create_multipart_upload_handler(State(state), Path(params), headers).await;
    }
    if query.contains_key("uploadId") {
        let upload_id = query.get("uploadId").cloned().unwrap_or_default();
        return complete_multipart_upload_handler(
            State(state),
            Path(params),
            upload_id,
            body,
        )
        .await;
    }
    let request_id = Uuid::new_v4().to_string();
    s3_error_response(
        StatusCode::BAD_REQUEST,
        "InvalidRequest",
        "Missing uploads or uploadId query parameter.",
        &request_id,
        "/",
    )
    .into_response()
}

/// `POST /{bucket}/{key}?uploads` — CreateMultipartUpload
async fn create_multipart_upload_handler(
    State(state): State<Arc<AppState>>,
    Path(params): Path<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    let request_id = Uuid::new_v4().to_string();
    let bucket = params.get("bucket").cloned().unwrap_or_default();
    let key = params.get("key").cloned().unwrap_or_default();
    let resource = format!("/{}/{}", bucket, key);

    let metadata = extract_metadata(&headers);

    match state
        .object_api
        .new_multipart_upload(&bucket, &key, &metadata)
        .await
    {
        Ok(info) => {
            tracing::debug!(
                "create_multipart_upload: {}/{} upload_id={}",
                bucket,
                key,
                info.upload_id
            );
            let result = InitiateMultipartUploadResultXml {
                xmlns: S3_XMLNS.to_string(),
                bucket: bucket.clone(),
                key: key.clone(),
                upload_id: info.upload_id.clone(),
            };
            s3_xml_response(&result).into_response()
        }
        Err(e) => {
            let (status, code, message) = to_s3_error_code(&e);
            tracing::error!("create_multipart_upload failed: {}: {}", resource, e);
            s3_error_response(status, code, message, &request_id, &resource).into_response()
        }
    }
}

/// `PUT /{bucket}/{key}?partNumber=N&uploadId=ID` — UploadPart
pub async fn upload_part_handler(
    State(state): State<Arc<AppState>>,
    Path(params): Path<HashMap<String, String>>,
    Query(query): Query<HashMap<String, String>>,
    body: Bytes,
) -> Response {
    let request_id = Uuid::new_v4().to_string();
    let bucket = params.get("bucket").cloned().unwrap_or_default();
    let key = params.get("key").cloned().unwrap_or_default();
    let resource = format!("/{}/{}", bucket, key);

    let part_number: u32 = match query
        .get("partNumber")
        .and_then(|v| v.parse().ok())
    {
        Some(n) if (1..=10000).contains(&n) => n,
        _ => {
            return s3_error_response(
                StatusCode::BAD_REQUEST,
                "InvalidArgument",
                "partNumber must be between 1 and 10000.",
                &request_id,
                &resource,
            )
            .into_response();
        }
    };

    let upload_id = match query.get("uploadId") {
        Some(id) if !id.is_empty() => id.clone(),
        _ => {
            return s3_error_response(
                StatusCode::BAD_REQUEST,
                "InvalidArgument",
                "uploadId is required.",
                &request_id,
                &resource,
            )
            .into_response();
        }
    };

    match state
        .object_api
        .put_object_part(&bucket, &key, &upload_id, part_number, &body)
        .await
    {
        Ok(etag) => {
            tracing::debug!(
                "upload_part: {}/{} upload_id={} part={} etag={}",
                bucket,
                key,
                upload_id,
                part_number,
                etag
            );
            let mut headers = HeaderMap::new();
            headers.insert(
                header::ETAG,
                HeaderValue::from_str(&format!("\"{}\"", etag)).unwrap(),
            );
            (StatusCode::OK, headers, String::new()).into_response()
        }
        Err(e) => {
            let (status, code, message) = to_s3_error_code(&e);
            tracing::error!("upload_part failed: {}: {}", resource, e);
            s3_error_response(status, code, message, &request_id, &resource).into_response()
        }
    }
}

/// `POST /{bucket}/{key}?uploadId=ID` — CompleteMultipartUpload
async fn complete_multipart_upload_handler(
    State(state): State<Arc<AppState>>,
    Path(params): Path<HashMap<String, String>>,
    upload_id: String,
    body: Bytes,
) -> Response {
    let request_id = Uuid::new_v4().to_string();
    let bucket = params.get("bucket").cloned().unwrap_or_default();
    let key = params.get("key").cloned().unwrap_or_default();
    let resource = format!("/{}/{}", bucket, key);

    // Parse XML body
    let body_str = String::from_utf8_lossy(&body);
    let complete_req: CompleteMultipartUploadBody = match quick_xml::de::from_str(&body_str) {
        Ok(req) => req,
        Err(e) => {
            tracing::error!("Malformed CompleteMultipartUpload XML: {}", e);
            return s3_error_response(
                StatusCode::BAD_REQUEST,
                "MalformedXML",
                "The XML you provided was not well-formed.",
                &request_id,
                &resource,
            )
            .into_response();
        }
    };

    let parts: Vec<CompletedPart> = complete_req
        .parts
        .iter()
        .map(|p| CompletedPart {
            part_number: p.part_number,
            etag: p.etag.trim_matches('"').to_string(),
        })
        .collect();

    match state
        .object_api
        .complete_multipart_upload(&bucket, &key, &upload_id, &parts)
        .await
    {
        Ok(info) => {
            tracing::debug!(
                "complete_multipart_upload: {}/{} upload_id={} etag={}",
                bucket,
                key,
                upload_id,
                info.etag
            );
            let result = CompleteMultipartUploadResultXml {
                xmlns: S3_XMLNS.to_string(),
                location: resource.clone(),
                bucket: bucket.clone(),
                key: key.clone(),
                etag: format!("\"{}\"", info.etag),
            };
            s3_xml_response(&result).into_response()
        }
        Err(e) => {
            let (status, code, message) = to_s3_error_code(&e);
            tracing::error!("complete_multipart_upload failed: {}: {}", resource, e);
            s3_error_response(status, code, message, &request_id, &resource).into_response()
        }
    }
}

/// `DELETE /{bucket}/{key}?uploadId=ID` — AbortMultipartUpload
pub async fn abort_multipart_upload_handler(
    State(state): State<Arc<AppState>>,
    Path(params): Path<HashMap<String, String>>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let request_id = Uuid::new_v4().to_string();
    let bucket = params.get("bucket").cloned().unwrap_or_default();
    let key = params.get("key").cloned().unwrap_or_default();
    let resource = format!("/{}/{}", bucket, key);

    let upload_id = match query.get("uploadId") {
        Some(id) if !id.is_empty() => id.clone(),
        _ => {
            return s3_error_response(
                StatusCode::BAD_REQUEST,
                "InvalidArgument",
                "uploadId is required.",
                &request_id,
                &resource,
            )
            .into_response();
        }
    };

    match state
        .object_api
        .abort_multipart_upload(&bucket, &key, &upload_id)
        .await
    {
        Ok(()) => {
            tracing::debug!(
                "abort_multipart_upload: {}/{} upload_id={}",
                bucket,
                key,
                upload_id
            );
            (StatusCode::NO_CONTENT, HeaderMap::new(), String::new()).into_response()
        }
        Err(e) => {
            let (status, code, message) = to_s3_error_code(&e);
            tracing::error!("abort_multipart_upload failed: {}: {}", resource, e);
            s3_error_response(status, code, message, &request_id, &resource).into_response()
        }
    }
}
