//! Core benchmark types: `Common`, `Operation`, `Benchmark`, and helpers.

use crate::bench::checksum::ChecksumType;
use crate::bench::collector::Collector;
use crate::bench::rate_limiter::RateLimiter;
use crate::bench::sse::SseConfig;
use crate::generator::{ObjSize, Source};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

// ---------------------------------------------------------------------------
// Error-proof helpers for common unwrap patterns
// ---------------------------------------------------------------------------

/// Extract inner value from `Arc<Mutex<T>>` when the Arc is known to be unique.
/// Replaces the `Arc::try_unwrap(x).unwrap().into_inner().unwrap()` triple-unwrap.
pub(crate) fn take_from_arc_mutex<T: Default>(arc: Arc<Mutex<T>>) -> anyhow::Result<T> {
    Arc::try_unwrap(arc)
        .map_err(|_| anyhow::anyhow!("Arc still has live references"))?
        .into_inner()
        .map_err(|_| anyhow::anyhow!("Mutex poisoned"))
}

pub mod body;
pub mod collector;
pub mod http_transport;
pub mod s3_client;
pub mod sse;
pub mod checksum;
pub mod get;
pub mod put;
pub mod rate_limiter;
pub mod delete;
pub mod list;
pub mod stat;
pub mod mixed;
pub mod versioned;
pub mod retention;
pub mod multipart;
pub mod multipart_put;
pub mod snowball;
pub mod fanout;
pub mod append;
pub mod zip;
pub mod iceberg_read;
pub mod iceberg_commits;
pub mod iceberg_mixed;
pub mod iceberg_sustained;

// ---------------------------------------------------------------------------
// Common — shared benchmark configuration
// ---------------------------------------------------------------------------
#[derive(Clone)]
pub struct Common {
    pub concurrency: usize,
    pub duration: Duration,
    pub bucket: String,
    pub location: String,
    pub source: Arc<dyn Fn() -> Box<dyn Source> + Send + Sync>,
    pub client_factory: crate::bench::s3_client::ClientFactory,
    pub collector: Arc<dyn Collector>,
    pub client_idx: usize,
    pub total_clients: usize,
    pub client_mode: bool,
    pub clear: bool,
    pub discard_output: bool,
    pub versioned: bool,
    pub locking: bool,
    pub auto_term_dur: Option<Duration>,
    pub auto_term_scale: f64,
    pub rps_limit: Option<f64>,
    /// Shared rate limiter for `rps_limit` (None disables throttling).
    pub rps_limiter: Option<std::sync::Arc<RateLimiter>>,
    pub host_select: HostSelect,
    pub no_prefix: bool,
    pub custom_prefix: Option<String>,
    pub obj_size: ObjSize,
    pub objects: usize,
    pub versions: usize,
    pub bench_data: Option<String>,
    pub analyze_only: bool,
    pub sse: SseConfig,
    pub checksum: Option<ChecksumType>,
    pub hosts: Vec<String>,
    /// Per-host inflight counts for weighed host selection.
    pub host_inflight: Arc<Mutex<Vec<usize>>>,
}

impl Common {
    /// Await before each S3 request when a global RPS cap is configured.
    pub async fn throttle_rps(&self) {
        if let Some(ref lim) = self.rps_limiter {
            lim.wait().await;
        }
    }

    pub fn prefix(&self) -> String {
        self.custom_prefix.clone().unwrap_or_else(|| {
            format!(
                "s3perf-{}",
                uuid::Uuid::new_v4()
                    .to_string()
                    .split('-')
                    .next()
                    .unwrap()
            )
        })
    }

    /// Host count (at least 1; indices match `client_factory`).
    pub fn host_count(&self) -> usize {
        self.hosts.len().max(1)
    }

    /// Simple round-robin host index (no inflight tracking).
    pub fn host_index(&self, thread: usize) -> usize {
        thread % self.host_count()
    }

    pub fn endpoint_for(&self, host_idx: usize) -> String {
        if self.hosts.is_empty() {
            String::new()
        } else {
            self.hosts[host_idx % self.hosts.len()].clone()
        }
    }

    /// Pick host index for one request; pair with [`Self::release_host_index`] when using weighed selection.
    pub fn pick_host_index(&self, thread_id: usize) -> usize {
        let n = self.hosts.len();
        if n == 0 {
            return 0;
        }
        match self.host_select {
            HostSelect::RoundRobin => thread_id % n,
            HostSelect::Weighed => {
                let mut g = self.host_inflight.lock().unwrap();
                let idx = (0..n).min_by_key(|&i| g[i]).unwrap_or(0);
                g[idx] += 1;
                idx
            }
        }
    }

    pub fn release_host_index(&self, idx: usize) {
        let n = self.hosts.len();
        if n == 0 || !matches!(self.host_select, HostSelect::Weighed) {
            return;
        }
        let mut g = self.host_inflight.lock().unwrap();
        let i = idx % n;
        if g[i] > 0 {
            g[i] -= 1;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostSelect {
    Weighed,
    RoundRobin,
}

impl Default for HostSelect {
    fn default() -> Self {
        Self::Weighed
    }
}

impl FromStr for HostSelect {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "weighed" => Ok(Self::Weighed),
            "roundrobin" => Ok(Self::RoundRobin),
            _ => Err(format!("unknown host selection policy: {s}")),
        }
    }
}

// ---------------------------------------------------------------------------
// Operation — one recorded request/response
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub first_byte: Option<DateTime<Utc>>,
    pub last_byte: Option<DateTime<Utc>>,
    pub op_type: String,
    pub err: String,
    pub file: String,
    pub client_id: String,
    pub endpoint: String,
    pub obj_per_op: u32,
    pub size: i64,
    pub thread: u32,
    pub categories: u64,
}

impl Operation {
    pub fn duration(&self) -> Duration {
        let d = self.end - self.start;
        d.to_std().unwrap_or_default()
    }

    pub fn ttfb(&self) -> Option<Duration> {
        self.first_byte.map(|fb| {
            let d = fb - self.start;
            d.to_std().unwrap_or_default()
        })
    }

    pub fn successful(&self) -> bool {
        self.err.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Operations — extension helpers on `Vec<Operation>`
// ---------------------------------------------------------------------------
pub trait OperationsExt {
    fn sort_by_start_time(&mut self);
    fn sort_by_end_time(&mut self);
    fn sort_by_duration(&mut self);
    fn sort_by_ttfb(&mut self);

    fn filter_by_op(&self, op: &str) -> Vec<Operation>;
    fn filter_by_endpoint(&self, ep: &str) -> Vec<Operation>;
    fn filter_successful(&self) -> Vec<Operation>;
    fn filter_errors(&self) -> Vec<Operation>;
    fn filter_inside_range(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Vec<Operation>;
    fn filter_first(&self, n: usize) -> Vec<Operation>;
    fn filter_last(&self, n: usize) -> Vec<Operation>;

    fn by_endpoint(&self) -> Vec<(String, Vec<Operation>)>;
    fn sort_split_by_endpoint(&self) -> Vec<(String, Vec<Operation>)>;
    fn sort_split_by_op_type(&self) -> Vec<(String, Vec<Operation>)>;
    fn sort_split_by_client(&self) -> Vec<(String, Vec<Operation>)>;

    fn time_range(&self) -> Option<(DateTime<Utc>, DateTime<Utc>)>;
    fn active_time_range(&self) -> Option<(DateTime<Utc>, DateTime<Utc>)>;
    fn total_duration(&self) -> Duration;
    fn threads(&self) -> usize;
    fn hosts(&self) -> usize;
    fn clients(&self) -> usize;
    fn endpoints(&self) -> Vec<String>;
    fn op_types(&self) -> Vec<String>;
    fn min_max_size(&self) -> Option<(i64, i64)>;
    fn avg_size(&self) -> f64;
    fn avg_duration(&self) -> Duration;
    fn median_duration(&self) -> Duration;
    fn std_dev_duration(&self) -> Duration;

    fn aggregate(&self, segment_dur: Duration) -> Vec<Segment>;
}

impl OperationsExt for Vec<Operation> {
    fn sort_by_start_time(&mut self) {
        self.sort_by_key(|o| o.start);
    }
    fn sort_by_end_time(&mut self) {
        self.sort_by_key(|o| o.end);
    }
    fn sort_by_duration(&mut self) {
        self.sort_by(|a, b| a.duration().cmp(&b.duration()));
    }
    fn sort_by_ttfb(&mut self) {
        self.sort_by(|a, b| a.ttfb().cmp(&b.ttfb()));
    }

    fn filter_by_op(&self, op: &str) -> Vec<Operation> {
        self.iter().filter(|o| o.op_type == op).cloned().collect()
    }
    fn filter_by_endpoint(&self, ep: &str) -> Vec<Operation> {
        self.iter().filter(|o| o.endpoint == ep).cloned().collect()
    }
    fn filter_successful(&self) -> Vec<Operation> {
        self.iter().filter(|o| o.successful()).cloned().collect()
    }
    fn filter_errors(&self) -> Vec<Operation> {
        self.iter().filter(|o| !o.successful()).cloned().collect()
    }
    fn filter_inside_range(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Vec<Operation> {
        self.iter()
            .filter(|o| o.start >= start && o.end <= end)
            .cloned()
            .collect()
    }
    fn filter_first(&self, n: usize) -> Vec<Operation> {
        let mut v: Vec<_> = self.clone();
        v.sort_by_start_time();
        v.truncate(n);
        v
    }
    fn filter_last(&self, n: usize) -> Vec<Operation> {
        let mut v: Vec<_> = self.clone();
        v.sort_by_start_time();
        if n < v.len() {
            v.drain(..v.len() - n);
        }
        v
    }

    fn by_endpoint(&self) -> Vec<(String, Vec<Operation>)> {
        let mut map: std::collections::BTreeMap<String, Vec<Operation>> =
            std::collections::BTreeMap::new();
        for o in self {
            map.entry(o.endpoint.clone()).or_default().push(o.clone());
        }
        map.into_iter().collect()
    }
    fn sort_split_by_endpoint(&self) -> Vec<(String, Vec<Operation>)> {
        self.by_endpoint()
    }
    fn sort_split_by_op_type(&self) -> Vec<(String, Vec<Operation>)> {
        let mut map: std::collections::BTreeMap<String, Vec<Operation>> =
            std::collections::BTreeMap::new();
        for o in self {
            map.entry(o.op_type.clone()).or_default().push(o.clone());
        }
        map.into_iter().collect()
    }
    fn sort_split_by_client(&self) -> Vec<(String, Vec<Operation>)> {
        let mut map: std::collections::BTreeMap<String, Vec<Operation>> =
            std::collections::BTreeMap::new();
        for o in self {
            map.entry(o.client_id.clone())
                .or_default()
                .push(o.clone());
        }
        map.into_iter().collect()
    }

    fn time_range(&self) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
        let min = self.iter().map(|o| o.start).min()?;
        let max = self.iter().map(|o| o.end).max()?;
        Some((min, max))
    }
    fn active_time_range(&self) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
        self.time_range()
    }
    fn total_duration(&self) -> Duration {
        self.iter().map(|o| o.duration()).sum()
    }
    fn threads(&self) -> usize {
        self.iter().map(|o| o.thread).collect::<std::collections::BTreeSet<_>>().len()
    }
    fn hosts(&self) -> usize {
        self.iter().map(|o| o.endpoint.as_str()).collect::<std::collections::BTreeSet<_>>().len()
    }
    fn clients(&self) -> usize {
        self.iter().map(|o| o.client_id.as_str()).collect::<std::collections::BTreeSet<_>>().len()
    }
    fn endpoints(&self) -> Vec<String> {
        let mut eps: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for o in self {
            eps.insert(o.endpoint.clone());
        }
        eps.into_iter().collect()
    }
    fn op_types(&self) -> Vec<String> {
        let mut types: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for o in self {
            types.insert(o.op_type.clone());
        }
        types.into_iter().collect()
    }
    fn min_max_size(&self) -> Option<(i64, i64)> {
        let min = self.iter().map(|o| o.size).min()?;
        let max = self.iter().map(|o| o.size).max()?;
        Some((min, max))
    }
    fn avg_size(&self) -> f64 {
        if self.is_empty() {
            return 0.0;
        }
        self.iter().map(|o| o.size as f64).sum::<f64>() / self.len() as f64
    }
    fn avg_duration(&self) -> Duration {
        if self.is_empty() {
            return Duration::default();
        }
        let total: Duration = self.iter().map(|o| o.duration()).sum();
        total.div_f64(self.len() as f64)
    }
    fn median_duration(&self) -> Duration {
        let mut durs: Vec<Duration> = self.iter().map(|o| o.duration()).collect();
        durs.sort();
        if durs.is_empty() {
            return Duration::default();
        }
        let mid = durs.len() / 2;
        if durs.len() % 2 == 0 {
            (durs[mid - 1] + durs[mid]).div_f64(2.0)
        } else {
            durs[mid]
        }
    }
    fn std_dev_duration(&self) -> Duration {
        if self.len() < 2 {
            return Duration::default();
        }
        let avg = self.avg_duration().as_secs_f64();
        let variance: f64 = self
            .iter()
            .map(|o| {
                let diff = o.duration().as_secs_f64() - avg;
                diff * diff
            })
            .sum::<f64>()
            / (self.len() - 1) as f64;
        Duration::from_secs_f64(variance.sqrt())
    }

    fn aggregate(&self, segment_dur: Duration) -> Vec<Segment> {
        if self.is_empty() {
            return Vec::new();
        }
        let (start, _end) = self.time_range().unwrap();
        let dur_ms = segment_dur.as_millis() as i64;
        if dur_ms <= 0 {
            return Vec::new();
        }
        let start_ms = start.timestamp_millis();

        let mut buckets: std::collections::BTreeMap<i64, Vec<&Operation>> =
            std::collections::BTreeMap::new();
        for op in self {
            let b = (op.start.timestamp_millis() - start_ms) / dur_ms;
            buckets.entry(b).or_default().push(op);
        }

        buckets
            .into_iter()
            .map(|(bucket, ops)| {
                let seg_start_ms = start_ms + bucket * dur_ms;
                let seg_start =
                    DateTime::from_timestamp_millis(seg_start_ms).unwrap_or(start);
                let seg_end =
                    DateTime::from_timestamp_millis(seg_start_ms + dur_ms).unwrap_or(start);
                let total_bytes: i64 = ops.iter().map(|o| o.size).sum();
                let objects: f64 = ops.iter().map(|o| o.obj_per_op as f64).sum();
                let full_ops = ops
                    .iter()
                    .filter(|o| o.start >= seg_start && o.end <= seg_end)
                    .count();
                let partial_ops = ops.len() - full_ops;
                let errors = ops.iter().filter(|o| !o.successful()).count();
                let avg_dur = if ops.is_empty() {
                    0.0
                } else {
                    ops.iter()
                        .map(|o| o.duration().as_secs_f64() * 1000.0)
                        .sum::<f64>()
                        / ops.len() as f64
                };

                Segment {
                    start: seg_start,
                    ends_before: seg_end,
                    op_type: ops.first().map(|o| o.op_type.clone()).unwrap_or_default(),
                    total_bytes,
                    objects,
                    full_ops,
                    partial_ops,
                    ops_started: ops.len(),
                    ops_ended: ops.len(),
                    errors,
                    req_avg: avg_dur,
                    objs_per_op: ops.first().map(|o| o.obj_per_op).unwrap_or(1),
                }
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Benchmark trait
// ---------------------------------------------------------------------------
#[async_trait::async_trait]
pub trait Benchmark: Send + Sync {
    /// Prepare: create bucket and seed objects.
    async fn prepare(&self, ctx: &CancellationToken) -> crate::generator::Result<()>;
    /// Run: execute workload and record operations.
    async fn start(
        &self,
        ctx: &CancellationToken,
        wait: tokio::sync::broadcast::Receiver<()>,
    ) -> crate::generator::Result<()>;
    /// Cleanup: delete benchmark objects.
    async fn cleanup(&self, ctx: &CancellationToken);
    /// Shared configuration for this benchmark.
    fn common(&self) -> &Common;
    /// Collected operations after the run.
    fn ops(&self) -> Vec<Operation>;
}

// ---------------------------------------------------------------------------
// Segment — time-bucket aggregate
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub start: DateTime<Utc>,
    pub ends_before: DateTime<Utc>,
    pub op_type: String,
    pub total_bytes: i64,
    pub objects: f64,
    pub full_ops: usize,
    pub partial_ops: usize,
    pub ops_started: usize,
    pub ops_ended: usize,
    pub errors: usize,
    pub req_avg: f64,
    pub objs_per_op: u32,
}
