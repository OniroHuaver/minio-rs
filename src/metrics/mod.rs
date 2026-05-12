//! Metrics V3 module — Prometheus metrics over `/minio/metrics/v3/*`.
//!
//! Architecture:
//! - `types` — lightweight metric descriptors for `?list` queries.
//! - `registry` — maps V3 paths to `prometheus::Registry` instances.
//! - `handler` — axum HTTP handler for the V3 endpoint.
//! - `collectors` — per-path metric registration.
//! - `http_stats` — `CounterVec`/`HistogramVec` for `/api/requests`.
//! - `middleware` — axum middleware that records API request metrics.

mod collectors;
pub mod handler;
pub mod http_stats;
pub mod middleware;
pub mod registry;
pub mod types;

pub use collectors::build_registry;
pub use handler::metrics_handler;
pub use http_stats::HttpStats;
pub use middleware::metrics_middleware;
pub use registry::MetricsRegistry;
