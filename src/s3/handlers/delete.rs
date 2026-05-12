//! Multi-object delete handler (POST /:bucket?delete)

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use uuid::Uuid;

use crate::s3::error::to_s3_error_code;
use crate::s3::request::DeleteObjectsBody;
use crate::s3::response::{
    s3_error_response, s3_xml_response, DeleteErrorXml, DeleteResultXml, DeletedEntry,
};

use crate::s3::state::AppState;

pub async fn delete_objects_handler(
    State(state): State<Arc<AppState>>,
    Path(bucket): Path<String>,
    body: Bytes,
) -> Response {
    let request_id = Uuid::new_v4().to_string();
    let resource = format!("/{}", bucket);

    // Parse XML request body
    let body_str = String::from_utf8_lossy(&body);
    let delete_req: DeleteObjectsBody = match quick_xml::de::from_str(&body_str) {
        Ok(req) => req,
        Err(e) => {
            tracing::error!("Malformed delete objects XML: {}", e);
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

    let keys: Vec<String> = delete_req
        .objects
        .iter()
        .map(|o| o.key.clone())
        .collect();
    let quiet = delete_req.quiet.unwrap_or(false);

    match state.object_api.delete_objects(&bucket, &keys).await {
        Ok(result) => {
            let deleted: Vec<DeletedEntry> = if quiet {
                Vec::new()
            } else {
                result
                    .deleted
                    .iter()
                    .map(|k| DeletedEntry { key: k.clone() })
                    .collect()
            };
            let errors: Vec<DeleteErrorXml> = result
                .errors
                .iter()
                .map(|(key, code, message)| DeleteErrorXml {
                    key: key.clone(),
                    code: code.clone(),
                    message: message.clone(),
                })
                .collect();

            let delete_result = DeleteResultXml { deleted, errors };
            s3_xml_response(&delete_result).into_response()
        }
        Err(e) => {
            let (status, code, message) = to_s3_error_code(&e);
            tracing::error!("delete_objects failed: {}: {}", resource, e);
            s3_error_response(status, code, message, &request_id, &resource).into_response()
        }
    }
}
