//! HTTP handler for `/minio/metrics/v3/*path`.
//!
//! Supports:
//! - `GET /minio/metrics/v3` — all metrics, Prometheus text format.
//! - `GET /minio/metrics/v3/{path}` — metrics under `path`, including
//!   descendants (parent aggregation).
//! - `?list` — returns metric metadata as JSON.
//! - `?bucket=a,b` — filters metrics to those with a matching `bucket` label.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::metrics::types::MetricInfo;
use crate::s3::state::AppState;

/// Route handler for `/minio/metrics/v3` and `/minio/metrics/v3/*path`.
pub async fn metrics_handler(
    State(state): State<Arc<AppState>>,
    path: Option<Path<String>>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let v3_path = match path {
        Some(Path(p)) if !p.is_empty() => format!("/{}", p.trim_start_matches('/')),
        _ => "/".to_string(),
    };

    // ── ?bucket=a,b — parse comma-separated bucket names ─
    let bucket_filter: Option<HashSet<String>> = params
        .get("bucket")
        .filter(|v| !v.is_empty())
        .map(|v| v.split(',').map(|s| s.trim().to_string()).collect());

    // ── ?list — return metric metadata ──
    if params.contains_key("list") {
        let list = state.metrics.list(Some(&v3_path));
        return list_response(&list);
    }

    // ── Default — gather and encode ──
    match state.metrics.encode_text(&v3_path, bucket_filter.as_ref()) {
        Ok(body) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
            body,
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [(header::CONTENT_TYPE, "text/plain")],
            e,
        )
            .into_response(),
    }
}

/// Render `?list` output — JSON.
fn list_response(list: &BTreeMap<String, Vec<MetricInfo>>) -> Response {
    match serde_json::to_string_pretty(list) {
        Ok(json) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json")],
            json,
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [(header::CONTENT_TYPE, "text/plain")],
            format!("json error: {e}"),
        )
            .into_response(),
    }
}
