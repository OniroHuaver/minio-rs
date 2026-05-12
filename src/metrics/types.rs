//! Metric descriptor types for the V3 `/minio/metrics/v3/*` endpoint.
//!
//! Provides lightweight metadata about registered metrics, used by `?list`
//! queries and for grouping metrics under collector paths.

use serde::Serialize;

/// Type of a Prometheus metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricType {
    Counter,
    Gauge,
    Histogram,
}

/// Human-readable metadata for a single metric.
#[derive(Debug, Clone, Serialize)]
pub struct MetricInfo {
    /// Fully-qualified Prometheus metric name (e.g. `minio_system_drive_total_bytes`).
    pub name: String,
    /// Help string for the metric.
    pub help: String,
    /// Prometheus metric type.
    #[serde(rename = "type")]
    pub metric_type: MetricType,
    /// Label names attached to this metric.
    pub labels: Vec<String>,
}
