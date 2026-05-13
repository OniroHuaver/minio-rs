//! HTTP request statistics — `CounterVec` and `HistogramVec` for the
//! `/api/requests` metrics group.
//!
//! `HttpStats` is `Clone`-able and cheap to share.  Call `record()` from
//! the request-recording middleware to populate the metrics.

use prometheus::{CounterVec, HistogramOpts, HistogramVec, Opts};

/// Cheaply-cloneable handle to the S3 API request counters.
#[derive(Clone)]
pub struct HttpStats {
    pub requests: CounterVec,
    pub duration: HistogramVec,
}

impl HttpStats {
    pub fn new() -> Self {
        let requests = CounterVec::new(
            Opts::new("s3_requests_total", "Total number of S3 API requests"),
            &["method", "status"],
        )
        .unwrap();

        let duration = HistogramVec::new(
            HistogramOpts::new(
                "s3_requests_duration_seconds",
                "S3 API request duration in seconds",
            )
            .buckets(vec![
                0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
            ]),
            &["method"],
        )
        .unwrap();

        HttpStats { requests, duration }
    }

    /// Record a completed S3 API request.
    pub fn record(&self, method: &str, status: u16, duration_secs: f64) {
        self.requests
            .with_label_values(&[method, &status.to_string()])
            .inc();
        self.duration
            .with_label_values(&[method])
            .observe(duration_secs);
    }
}
