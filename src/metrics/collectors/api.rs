//! API request metrics: HTTP request count, latency histogram.

use crate::metrics::http_stats::HttpStats;
use crate::metrics::registry::MetricsGroup;
use crate::metrics::types::{MetricInfo, MetricType};

/// Build the `/api/requests` `MetricsGroup` and return it alongside the
/// `HttpStats` handle for middleware recording.
pub fn requests_group(http_stats: &HttpStats) -> (MetricsGroup, HttpStats) {
    let infos = vec![
        MetricInfo {
            name: "s3_requests_total".into(),
            help: "Total number of S3 API requests".into(),
            metric_type: MetricType::Counter,
            labels: vec!["method".into(), "status".into()],
        },
        MetricInfo {
            name: "s3_requests_duration_seconds".into(),
            help: "S3 API request duration in seconds (histogram)".into(),
            metric_type: MetricType::Histogram,
            labels: vec!["method".into()],
        },
    ];

    let group = MetricsGroup::new("/api/requests", infos);
    group.register(Box::new(http_stats.requests.clone()));
    group.register(Box::new(http_stats.duration.clone()));

    (group, http_stats.clone())
}
