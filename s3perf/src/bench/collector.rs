//! Collector — 操作收集器，负责收集、存储、自动终止判定。

use crate::bench::Operation;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

// ---------------------------------------------------------------------------
// Collector trait
// ---------------------------------------------------------------------------
pub trait Collector: Send + Sync {
    fn sender(&self) -> mpsc::UnboundedSender<Operation>;
    fn close(&self);
    fn ops(&self) -> Vec<Operation>;
    fn auto_term(
        &self,
        ctx: CancellationToken,
        op_name: &str,
        threshold: f64,
        want_samples: usize,
        split_into: usize,
        min_dur: Duration,
    ) -> CancellationToken;
}

// ---------------------------------------------------------------------------
// OpsCollector — 默认实现
// ---------------------------------------------------------------------------
pub struct OpsCollector {
    tx: mpsc::UnboundedSender<Operation>,
    ops: Arc<Mutex<Vec<Operation>>>,
}

impl OpsCollector {
    pub fn new() -> Self {
        Self::with_influx_fanout(None)
    }

    /// `influx_tx` 若存在，每个 operation 会额外 clone 一份推送（用于 InfluxDB 后台写入）。
    pub fn with_influx_fanout(influx_tx: Option<mpsc::UnboundedSender<Operation>>) -> Self {
        let ops = Arc::new(Mutex::new(Vec::new()));
        let ops_clone = ops.clone();
        let (tx, mut rx) = mpsc::unbounded_channel::<Operation>();

        tokio::spawn(async move {
            while let Some(op) = rx.recv().await {
                if let Some(ref t) = influx_tx {
                    let _ = t.send(op.clone());
                }
                if let Ok(mut guard) = ops_clone.lock() {
                    guard.push(op);
                }
            }
        });

        Self { tx, ops }
    }
}

impl Default for OpsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for OpsCollector {
    fn sender(&self) -> mpsc::UnboundedSender<Operation> {
        self.tx.clone()
    }

    fn close(&self) {}

    fn ops(&self) -> Vec<Operation> {
        self.ops.lock().map(|g| g.clone()).unwrap_or_default()
    }

    fn auto_term(
        &self,
        ctx: CancellationToken,
        op_name: &str,
        threshold: f64,
        want_samples: usize,
        split_into: usize,
        min_dur: Duration,
    ) -> CancellationToken {
        let auto_term_ctx = CancellationToken::new();
        let child_token = auto_term_ctx.clone();
        let parent_token = ctx.clone();
        let ops_arc = self.ops.clone();
        let op_name = op_name.to_string();

        // 先 clone parent_token 供闭包和外部使用
        let parent_for_spawn = parent_token.clone();

        tokio::spawn(async move {
            let tick = Duration::from_millis(500);
            let mut stable_since: Option<Instant> = None;

            loop {
                tokio::select! {
                    _ = parent_for_spawn.cancelled() => return,
                    _ = tokio::time::sleep(tick) => {}
                }

                let ops = ops_arc.lock().unwrap();
                if ops.len() < want_samples {
                    drop(ops);
                    continue;
                }

                let filtered: Vec<&Operation> = ops.iter().filter(|o| o.op_type == op_name).collect();
                if filtered.len() < want_samples {
                    drop(ops);
                    continue;
                }

                let mut sorted: Vec<&Operation> = filtered.clone();
                sorted.sort_by_key(|o| o.start);
                let chunk_size = (sorted.len() / split_into).max(2);
                if chunk_size < 2 {
                    drop(ops);
                    stable_since = None;
                    continue;
                }

                let mut chunk_rates: Vec<f64> = Vec::new();
                for i in 0..7.min(sorted.len() / chunk_size) {
                    let start_idx = sorted.len().saturating_sub(chunk_size * (i + 1));
                    let end_idx = sorted.len().saturating_sub(chunk_size * i);
                    if end_idx <= start_idx || start_idx >= sorted.len() {
                        continue;
                    }
                    let end_idx = end_idx.min(sorted.len());
                    let chunk_ops = &sorted[start_idx..end_idx];
                    if chunk_ops.len() < 2 {
                        continue;
                    }
                    let chunk_start = chunk_ops.first().unwrap().start;
                    let chunk_end = chunk_ops.last().unwrap().end;
                    let delta = chunk_end - chunk_start;
                    let dur_secs = delta.num_milliseconds() as f64 / 1000.0;
                    if dur_secs > 0.0 {
                        let total_bytes: i64 = chunk_ops.iter().map(|o| o.size).sum();
                        let mbps = (total_bytes as f64 / (1024.0 * 1024.0)) / dur_secs;
                        chunk_rates.push(mbps);
                    }
                }
                drop(ops);

                if chunk_rates.len() < 2 {
                    stable_since = None;
                    continue;
                }

                let avg: f64 = chunk_rates.iter().sum::<f64>() / chunk_rates.len() as f64;
                let max_deviation = chunk_rates
                    .iter()
                    .map(|r| (r - avg).abs())
                    .max_by(|a, b| a.partial_cmp(b).unwrap())
                    .unwrap_or(0.0);

                let is_stable = avg > 0.0 && (max_deviation / avg) <= threshold;

                if is_stable {
                    if let Some(since) = stable_since {
                        if since.elapsed() >= min_dur {
                            auto_term_ctx.cancel();
                            return;
                        }
                    } else {
                        stable_since = Some(Instant::now());
                    }
                } else {
                    stable_since = None;
                }
            }
        });

        // 合并两个 CancellationToken
        let merged = CancellationToken::new();
        let merged_clone = merged.clone();
        let child_clone = child_token.clone();
        let parent_clone = parent_token.clone();

        tokio::spawn(async move {
            tokio::select! {
                _ = child_clone.cancelled() => merged_clone.cancel(),
                _ = parent_clone.cancelled() => merged_clone.cancel(),
            }
        });

        merged
    }
}

