//! Bucket versioning handlers: GetBucketVersioning, PutBucketVersioning

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use uuid::Uuid;

use crate::s3::error::to_s3_error_code;
use crate::s3::response::{
    S3_XMLNS, VersioningConfigurationXml, s3_error_response, s3_xml_response,
};
use crate::s3::state::AppState;

/// `GET /:bucket?versioning` — GetBucketVersioning
pub async fn get_bucket_versioning_handler(
    State(state): State<Arc<AppState>>,
    Path(bucket): Path<String>,
) -> Response {
    let request_id = Uuid::new_v4().to_string();
    let resource = format!("/{}", bucket);

    match state.object_api.get_bucket_versioning(&bucket).await {
        Ok(Some(config)) => {
            let xml = VersioningConfigurationXml {
                xmlns: S3_XMLNS.to_string(),
                status: match config.status {
                    crate::object::VersioningStatus::Enabled => "Enabled".to_string(),
                    crate::object::VersioningStatus::Suspended => "Suspended".to_string(),
                },
                mfa_delete: "Disabled".to_string(),
            };
            s3_xml_response(&xml).into_response()
        }
        Ok(None) => {
            // No versioning config → return empty default
            let xml = VersioningConfigurationXml {
                xmlns: S3_XMLNS.to_string(),
                status: String::new(),
                mfa_delete: "Disabled".to_string(),
            };
            s3_xml_response(&xml).into_response()
        }
        Err(e) => {
            let (status, code, message) = to_s3_error_code(&e);
            tracing::error!("get_bucket_versioning failed: {}: {}", resource, e);
            s3_error_response(status, code, message, &request_id, &resource).into_response()
        }
    }
}

/// `PUT /:bucket?versioning` — PutBucketVersioning
pub async fn put_bucket_versioning_handler(
    State(state): State<Arc<AppState>>,
    Path(bucket): Path<String>,
    body: Bytes,
) -> Response {
    let request_id = Uuid::new_v4().to_string();
    let resource = format!("/{}", bucket);

    // Parse XML body
    let body_str = String::from_utf8_lossy(&body);
    #[derive(serde::Deserialize)]
    #[serde(rename = "VersioningConfiguration")]
    struct VersioningInput {
        #[serde(rename = "Status")]
        status: String,
    }

    let input: VersioningInput = match quick_xml::de::from_str(&body_str) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("Malformed versioning XML: {}", e);
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

    match state
        .object_api
        .set_bucket_versioning(&bucket, &input.status)
        .await
    {
        Ok(()) => {
            tracing::debug!("put_bucket_versioning: {} status={}", bucket, input.status);
            (StatusCode::OK, axum::http::HeaderMap::new(), String::new()).into_response()
        }
        Err(e) => {
            let (status, code, message) = to_s3_error_code(&e);
            tracing::error!("put_bucket_versioning failed: {}: {}", resource, e);
            s3_error_response(status, code, message, &request_id, &resource).into_response()
        }
    }
}
