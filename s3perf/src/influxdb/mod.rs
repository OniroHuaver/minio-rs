//! InfluxDB v2 telemetry fan-out (`s3perf` measurements + terminal `s3perf_run_summary`).

use crate::bench::Operation;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info};

// ---------------------------------------------------------------------------
// InfluxConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct InfluxConfig {
    pub url: String,
    pub token: String,
    pub bucket: String,
    pub org: String,
    pub global_tags: HashMap<String, String>,
}

/// Parse an Influx v2 ingest URL shaped like \
/// `http[s]://<token>@<host>:<port>/<bucket>/<org>?tag=value`.
pub fn parse_influx_url(raw: &str) -> Result<InfluxConfig, String> {
    // Split scheme
    let (scheme, rest) = if let Some(rest) = raw.strip_prefix("https://") {
        ("https", rest)
    } else if let Some(rest) = raw.strip_prefix("http://") {
        ("http", rest)
    } else {
        return Err("missing URL scheme (http:// or https://)".into());
    };

    // Pull token segment
    let (token, rest) = rest
        .split_once('@')
        .ok_or("missing token segment (expect token@host:port/bucket/org)")?;

    // host:port and bucket/org?query portions
    let (host_port, path_query) = rest
        .split_once('/')
        .ok_or("missing bucket path segment (expect host:port/bucket/org)")?;

    // bucket/org plus optional tags query
    let (path, query) = match path_query.split_once('?') {
        Some((p, q)) => (p, q),
        None => (path_query, ""),
    };

    // Bucket + org identifiers
    let mut parts = path.split('/');
    let bucket = parts.next().unwrap_or("s3perf").to_string();
    let org = parts.next().unwrap_or("default").to_string();

    // Attach query-string pairs as influx tags
    let mut global_tags = HashMap::new();
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            global_tags.insert(k.to_string(), v.to_string());
        }
    }

    // Unique run correlation id
    global_tags.insert(
        "run_id".to_string(),
        uuid::Uuid::new_v4()
            .to_string()
            .chars()
            .take(8)
            .collect(),
    );

    let url = format!("{scheme}://{host_port}");

    Ok(InfluxConfig {
        url,
        token: token.to_string(),
        bucket,
        org,
        global_tags,
    })
}

// ---------------------------------------------------------------------------
// AggregatedStats
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Clone)]
pub struct AggregatedStats {
    pub bytes: i64,
    pub objects: f64,
    pub ops: usize,
    pub errors: usize,
    pub req_dur: Duration,
    pub req_min: Duration,
    pub req_max: Duration,
    pub ttfb: Duration,
    pub ttfb_min: Duration,
    pub ttfb_max: Duration,
}

impl AggregatedStats {
    pub fn add_operation(&mut self, op: &Operation) {
        self.bytes += op.size;
        self.objects += op.obj_per_op as f64;
        self.ops += 1;
        if !op.successful() {
            self.errors += 1;
        }
        let dur = op.duration();
        self.req_dur += dur;
        if self.ops == 1 {
            self.req_min = dur;
            self.req_max = dur;
        } else {
            self.req_min = self.req_min.min(dur);
            self.req_max = self.req_max.max(dur);
        }
        if let Some(ttfb) = op.ttfb() {
            self.ttfb += ttfb;
            if self.ttfb_min.is_zero() || self.ttfb_min > ttfb {
                self.ttfb_min = ttfb;
            }
            self.ttfb_max = self.ttfb_max.max(ttfb);
        }
    }
}

// ---------------------------------------------------------------------------
// InfluxWriter (background ingestion task)

pub struct InfluxWriter {
    config: InfluxConfig,
    rx: mpsc::UnboundedReceiver<Operation>,
    /// host → op_type → AggregatedStats
    hosts: HashMap<String, HashMap<String, AggregatedStats>>,
    /// op_type → AggregatedStats (global rollup)
    total_ops: HashMap<String, AggregatedStats>,
    /// Pending operations since last flush
    pending: usize,
    client: reqwest::Client,
}

impl InfluxWriter {
    pub fn new(config: InfluxConfig, rx: mpsc::UnboundedReceiver<Operation>) -> Self {
        Self {
            config,
            rx,
            hosts: HashMap::new(),
            total_ops: HashMap::new(),
            pending: 0,
            client: reqwest::Client::new(),
        }
    }

    pub async fn run(mut self) {
        info!("InfluxDB writer starting");

        let mut interval = tokio::time::interval(Duration::from_millis(200));

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if self.pending > 0 {
                        self.flush_line_protocol().await;
                    }
                }
                msg = self.rx.recv() => {
                    match msg {
                        Some(op) => {
                            self.record(&op);
                            self.pending += 1;
                            // sample ~100 ops or interval tick triggers flush
                            if self.pending >= 100 {
                                self.flush_line_protocol().await;
                            }
                        }
                        None => {
                            // flush pending rows then summary before shutdown
                            self.flush_line_protocol().await;
                            self.flush_summary().await;
                            info!("InfluxDB writer shutting down");
                            return;
                        }
                    }
                }
            }
        }
    }

    fn record(&mut self, op: &Operation) {
        // Per-host and per-operation buckets
        let host_entry = self
            .hosts
            .entry(op.endpoint.clone())
            .or_default()
            .entry(op.op_type.clone())
            .or_default();
        host_entry.add_operation(op);

        // Global rollup per op-type
        let total_entry = self.total_ops.entry(op.op_type.clone()).or_default();
        total_entry.add_operation(op);
    }

    async fn flush_line_protocol(&mut self) {
        if self.pending == 0 {
            return;
        }
        self.pending = 0;

        let global_tags = &self.config.global_tags;
        let mut lines = Vec::new();

        // Global rollup series
        for (op_type, stats) in &self.total_ops {
            let tags = format_tags(&[("op", op_type.as_str())], global_tags);
            lines.push(build_realtime_measurement_line(&tags, stats));
        }

        // Endpoint-scoped rollup
        for (host, by_op) in &self.hosts {
            for (op_type, stats) in by_op {
                let tags =
                    format_tags(&[("op", op_type.as_str()), ("endpoint", host)], global_tags);
                lines.push(build_realtime_measurement_line(&tags, stats));
            }
        }

        if lines.is_empty() {
            return;
        }

        let body = lines.join("\n");
        let url = format!(
            "{}/api/v2/write?org={}&bucket={}&precision=ns",
            self.config.url, self.config.org, self.config.bucket
        );

        match self
            .client
            .post(&url)
            .header("Authorization", format!("Token {}", self.config.token))
            .header("Content-Type", "text/plain; charset=utf-8")
            .body(body)
            .send()
            .await
        {
            Ok(resp) => {
                if !resp.status().is_success() {
                    error!(
                        "InfluxDB write failed: {} - {}",
                        resp.status(),
                        resp.text().await.unwrap_or_default()
                    );
                }
            }
            Err(e) => error!("InfluxDB HTTP error: {e}"),
        }
    }

    async fn flush_summary(&mut self) {
        let global_tags = &self.config.global_tags;
        let mut lines = Vec::new();

        for (op_type, stats) in &self.total_ops {
            let tags = format_tags(&[("op", op_type.as_str())], global_tags);
            lines.push(build_summary_line(&tags, stats));
        }

        for (host, by_op) in &self.hosts {
            for (op_type, stats) in by_op {
                let tags =
                    format_tags(&[("op", op_type.as_str()), ("endpoint", host)], global_tags);
                lines.push(build_summary_line(&tags, stats));
            }
        }

        if lines.is_empty() {
            return;
        }

        let body = lines.join("\n");
        let url = format!(
            "{}/api/v2/write?org={}&bucket={}&precision=ns",
            self.config.url, self.config.org, self.config.bucket
        );

        let _ = self
            .client
            .post(&url)
            .header("Authorization", format!("Token {}", self.config.token))
            .header("Content-Type", "text/plain; charset=utf-8")
            .body(body)
            .send()
            .await;
    }
}

fn format_tags(tags: &[(&str, &str)], global: &HashMap<String, String>) -> String {
    let mut parts: Vec<String> = tags
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    for (k, v) in global {
        parts.push(format!("{k}={v}"));
    }
    parts.join(",")
}

fn build_realtime_measurement_line(tags: &str, stats: &AggregatedStats) -> String {
    let req_secs = stats.req_dur.as_secs_f64();
    let ttfb_secs = stats.ttfb.as_secs_f64();
    format!(
        "s3perf,{tags} requests={}i,objects={}i,bytes_total={}i,errors={}i,request_total_secs={req_secs:.6},request_ttfb_total_secs={ttfb_secs:.6}",
        stats.ops,
        stats.objects as i64,
        stats.bytes,
        stats.errors,
    )
}

fn build_summary_line(tags: &str, stats: &AggregatedStats) -> String {
    let req_secs = stats.req_dur.as_secs_f64();
    let ttfb_secs = stats.ttfb.as_secs_f64();
    let avg_req = if stats.ops > 0 {
        req_secs / stats.ops as f64
    } else {
        0.0
    };
    let avg_ttfb = if stats.ops > 0 {
        ttfb_secs / stats.ops as f64
    } else {
        0.0
    };
    format!(
        "s3perf_run_summary,{tags} requests={}i,objects={}i,bytes_total={}i,errors={}i,\
         request_avg_secs={avg_req:.6},request_min_secs={min:.6},request_max_secs={max:.6},\
         request_ttfb_avg_secs={avg_ttfb:.6},request_ttfb_min_secs={tmin:.6},request_ttfb_max_secs={tmax:.6}",
        stats.ops,
        stats.objects as i64,
        stats.bytes,
        stats.errors,
        min = stats.req_min.as_secs_f64(),
        max = stats.req_max.as_secs_f64(),
        tmin = stats.ttfb_min.as_secs_f64(),
        tmax = stats.ttfb_max.as_secs_f64(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_influx_url() {
        let cfg = parse_influx_url(
            "http://mytoken@localhost:8086/mybucket/myorg?host=dev&dc=us1",
        )
        .unwrap();
        assert_eq!(cfg.url, "http://localhost:8086");
        assert_eq!(cfg.token, "mytoken");
        assert_eq!(cfg.bucket, "mybucket");
        assert_eq!(cfg.org, "myorg");
        assert!(cfg.global_tags.contains_key("host"));
        assert!(cfg.global_tags.contains_key("dc"));
        assert!(cfg.global_tags.contains_key("run_id"));
    }

    #[test]
    fn test_aggregated_stats() {
        let mut stats = AggregatedStats::default();
        let op = Operation {
            start: chrono::Utc::now(),
            end: chrono::Utc::now() + Duration::from_millis(12),
            first_byte: Some(chrono::Utc::now() + Duration::from_millis(8)),
            last_byte: None,
            op_type: "GET".into(),
            err: String::new(),
            file: "obj-1".into(),
            client_id: "c1".into(),
            endpoint: "localhost:9000".into(),
            obj_per_op: 1,
            size: 1024,
            thread: 0,
            categories: 0,
        };
        stats.add_operation(&op);
        assert_eq!(stats.ops, 1);
        assert_eq!(stats.bytes, 1024);
        assert_eq!(stats.errors, 0);
    }
}
