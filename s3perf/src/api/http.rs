//! HTTP monitoring endpoints (axum): e.g. `GET /v1/status`.

use crate::api::{BenchmarkMonitor, BenchmarkStatus};
use axum::{extract::State, routing::get, Json, Router};
use std::net::SocketAddr;
use std::sync::Arc;

async fn status_handler(State(m): State<Arc<BenchmarkMonitor>>) -> Json<BenchmarkStatus> {
    Json(m.status())
}

/// Bind `addr` and serve `GET /v1/status`.
pub async fn serve_monitor(
    monitor: Arc<BenchmarkMonitor>,
    addr: SocketAddr,
) -> Result<(), std::io::Error> {
    let app = Router::new()
        .route("/v1/status", get(status_handler))
        .with_state(monitor);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
