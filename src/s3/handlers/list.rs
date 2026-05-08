//! ListObjectsV2 handler

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
};
use uuid::Uuid;

use crate::s3::error::to_s3_error_code;
use crate::s3::response::{
    s3_error_response, s3_xml_response, CommonPrefixesEntry, ContentEntry, ListBucketResult,
    S3_XMLNS,
};
use crate::s3::state::AppState;

pub async fn list_objects_v2_handler(
    State(state): State<Arc<AppState>>,
    Path(bucket): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let request_id = Uuid::new_v4().to_string();
    let resource = format!("/{}", bucket);

    let prefix = params.get("prefix").cloned().unwrap_or_default();
    let delimiter = params.get("delimiter").cloned().unwrap_or_default();
    let max_keys: usize = params
        .get("max-keys")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000)
        .clamp(1, 1000);
    let start_after = params.get("start-after").map(|s| s.as_str());
    let continuation_token = params.get("continuation-token").map(|s| s.as_str());

    match state
        .object_api
        .list_objects(&bucket, &prefix, &delimiter, max_keys, start_after, continuation_token)
        .await
    {
        Ok(result) => {
            let contents: Vec<ContentEntry> = result
                .objects
                .iter()
                .map(|obj| ContentEntry {
                    key: obj.name.clone(),
                    last_modified: crate::s3::response::format_s3_timestamp(obj.mod_time),
                    etag: format!("\"{}\"", obj.etag),
                    size: obj.size,
                    storage_class: "STANDARD".to_string(),
                })
                .collect();

            let common_prefixes: Vec<CommonPrefixesEntry> = result
                .common_prefixes
                .iter()
                .map(|p| CommonPrefixesEntry {
                    prefix: p.clone(),
                })
                .collect();

            let list_result = ListBucketResult {
                xmlns: S3_XMLNS.to_string(),
                name: bucket.clone(),
                prefix: prefix.clone(),
                key_count: contents.len() + common_prefixes.len(),
                max_keys,
                is_truncated: result.is_truncated,
                next_continuation_token: if result.is_truncated {
                    Some(result.next_continuation_token.clone())
                } else {
                    None
                },
                continuation_token: continuation_token.map(|s| s.to_string()),
                contents,
                common_prefixes,
            };

            tracing::debug!(
                "list_objects: bucket={} prefix={} delimiter={} count={} truncated={}",
                bucket,
                prefix,
                delimiter,
                result.objects.len(),
                result.is_truncated
            );
            s3_xml_response(&list_result).into_response()
        }
        Err(e) => {
            let (status, code, message) = to_s3_error_code(&e);
            tracing::error!("list_objects failed: {}: {}", resource, e);
            s3_error_response(status, code, message, &request_id, &resource).into_response()
        }
    }
}
