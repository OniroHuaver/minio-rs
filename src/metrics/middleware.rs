//! Axum middleware that records S3 API request metrics via `HttpStats`.

use std::sync::Arc;
use std::time::Instant;

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;

use crate::metrics::http_stats::HttpStats;

/// Middleware that records method, status code, and duration for every
/// S3 API request passing through it.
pub async fn metrics_middleware(
    State(stats): State<Arc<HttpStats>>,
    req: Request,
    next: Next,
) -> Response {
    let method = req.method().to_string();
    let start = Instant::now();

    let response = next.run(req).await;

    let status = response.status().as_u16();
    stats.record(&method, status, start.elapsed().as_secs_f64());

    response
}
