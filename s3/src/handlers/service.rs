//! ListBuckets handler

use std::sync::Arc;

use axum::{
    extract::State,
    response::{IntoResponse, Response},
};
use uuid::Uuid;

use crate::error::to_s3_error_code;
use crate::response::{
    s3_error_response, s3_xml_response, BucketEntry, BucketsList, ListAllMyBucketsResult, Owner,
    S3_XMLNS,
};
use crate::state::AppState;

pub async fn list_buckets_handler(
    State(state): State<Arc<AppState>>,
) -> Response {
    let request_id = Uuid::new_v4().to_string();

    match state.object_api.list_buckets().await {
        Ok(buckets) => {
            let now = chrono::Utc::now().timestamp();
            let now_str = crate::response::format_s3_timestamp(now);

            let result = ListAllMyBucketsResult {
                xmlns: S3_XMLNS.to_string(),
                owner: Owner {
                    id: "minio".to_string(),
                    display_name: "minio".to_string(),
                },
                buckets: BucketsList {
                    bucket: buckets
                        .iter()
                        .map(|name| BucketEntry {
                            name: name.clone(),
                            creation_date: now_str.clone(),
                        })
                        .collect(),
                },
            };

            tracing::debug!("list_buckets: {} buckets", buckets.len());
            s3_xml_response(&result).into_response()
        }
        Err(e) => {
            let (status, code, message) = to_s3_error_code(&e);
            tracing::error!("list_buckets failed: {}", e);
            s3_error_response(status, code, message, &request_id, "/").into_response()
        }
    }
}
