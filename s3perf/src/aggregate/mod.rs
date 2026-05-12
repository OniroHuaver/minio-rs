//! 数据分析模块 — 吞吐量计算、延迟百分位、对比、合并、序列化。

#[cfg(test)]
mod tests;

use crate::bench::Operation;
use crate::bench::OperationsExt;
use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};

// ---------------------------------------------------------------------------
// Throughput 统计
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Throughput {
    pub op_type: String,
    pub avg_mbps: f64,
    pub avg_ops: f64,
    pub total_bytes: i64,
    pub total_ops: usize,
    pub duration_secs: f64,
    pub errors: usize,
    pub segmented: Option<ThroughputSegmented>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThroughputSegmented {
    pub segment_duration_ms: u64,
    pub fastest_mbps: f64,
    pub median_mbps: f64,
    pub slowest_mbps: f64,
    pub fastest_ops: f64,
    pub median_ops: f64,
    pub slowest_ops: f64,
    pub segments: Vec<SegmentStats>,
    pub fastest_start: DateTime<Utc>,
    pub median_start: DateTime<Utc>,
    pub slowest_start: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentStats {
    pub start: DateTime<Utc>,
    pub mbps: f64,
    pub ops: f64,
    pub bytes: i64,
    pub errors: usize,
}

// ---------------------------------------------------------------------------
// 请求级别统计
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleSizedRequests {
    pub size: i64,
    pub requests: usize,
    pub avg_duration_ms: f64,
    pub median_duration_ms: f64,
    pub p90_duration_ms: f64,
    pub p99_duration_ms: f64,
    pub fastest_duration_ms: f64,
    pub slowest_duration_ms: f64,
    pub std_dev_ms: f64,
    // TTFB
    pub avg_ttfb_ms: Option<f64>,
    pub median_ttfb_ms: Option<f64>,
    pub p90_ttfb_ms: Option<f64>,
    pub p99_ttfb_ms: Option<f64>,
    pub fastest_ttfb_ms: Option<f64>,
    pub slowest_ttfb_ms: Option<f64>,
    // First/Last access
    pub first_access: Option<AccessStats>,
    pub last_access: Option<AccessStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiSizedRequests {
    pub avg_size: f64,
    pub avg_duration_ms: f64,
    pub median_duration_ms: f64,
    pub p90_duration_ms: f64,
    pub p99_duration_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessStats {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// OpAnalysis — 单操作类型的完整分析
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpAnalysis {
    pub op_type: String,
    pub throughput: Throughput,
    pub throughput_by_host: HashMap<String, Throughput>,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub single_sized: Option<SingleSizedRequests>,
    pub multi_sized: Option<MultiSizedRequests>,
    pub host_names: Vec<String>,
    pub hosts: usize,
    pub clients: usize,
    pub concurrency: usize,
    pub objects_per_operation: u32,
    pub errors: usize,
    pub first_errors: Vec<String>,
    pub n: usize,
    pub skipped: bool,
}

// ---------------------------------------------------------------------------
// Aggregated — 完整聚合结果
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Aggregated {
    pub mixed: bool,
    pub operations: Vec<OpAnalysis>,
    pub mixed_server_stats: Option<Throughput>,
    pub mixed_throughput_by_host: HashMap<String, Throughput>,
}

// ---------------------------------------------------------------------------
// 数据分析主入口
// ---------------------------------------------------------------------------
pub fn analyze(
    ops: &[Operation],
    segment_dur: std::time::Duration,
    concurrency: usize,
) -> Aggregated {
    let chrono_seg_dur = TimeDelta::milliseconds(segment_dur.as_millis() as i64);
    let by_op = ops.to_vec().sort_split_by_op_type();
    let mut analyses = Vec::new();

    for (op_type, op_list) in &by_op {
        let analysis = analyze_single_op(op_list, op_type, chrono_seg_dur, concurrency);
        analyses.push(analysis);
    }

    let all_ok: Vec<&Operation> = ops.iter().filter(|o| o.successful()).collect();
    let total_bytes: i64 = all_ok.iter().map(|o| o.size).sum();
    let total_ops_num = all_ok.len();
    let time_rng = ops.to_vec().time_range().unwrap_or((Utc::now(), Utc::now()));
    let delta = time_rng.1 - time_rng.0;
    let d = delta.num_milliseconds() as f64 / 1000.0;
    let avg_mbps = if d > 0.0 {
        (total_bytes as f64 / (1024.0 * 1024.0)) / d
    } else {
        0.0
    };
    let avg_ops = if d > 0.0 { total_ops_num as f64 / d } else { 0.0 };
    let errors = ops.iter().filter(|o| !o.successful()).count();

    let mixed_server_stats = Some(Throughput {
        op_type: "ALL".into(),
        avg_mbps,
        avg_ops,
        total_bytes,
        total_ops: total_ops_num,
        duration_secs: d,
        errors,
        segmented: compute_segmented(ops, chrono_seg_dur),
    });

    Aggregated {
        mixed: by_op.len() > 1,
        operations: analyses,
        mixed_server_stats,
        mixed_throughput_by_host: HashMap::new(),
    }
}

fn analyze_single_op(
    ops: &[Operation],
    op_type: &str,
    segment_dur: TimeDelta,
    concurrency: usize,
) -> OpAnalysis {
    let ok: Vec<&Operation> = ops.iter().filter(|o| o.successful()).collect();
    let errors = ops.iter().filter(|o| !o.successful()).count();
    let n = ops.len();

    let total_bytes: i64 = ok.iter().map(|o| o.size).sum();
    let time_rng = ops.to_vec().time_range().unwrap_or((Utc::now(), Utc::now()));
    let delta = time_rng.1 - time_rng.0;
    let d = delta.num_milliseconds() as f64 / 1000.0;

    let avg_mbps = if d > 0.0 {
        (total_bytes as f64 / (1024.0 * 1024.0)) / d
    } else {
        0.0
    };
    let avg_ops = if d > 0.0 { ok.len() as f64 / d } else { 0.0 };

    let throughput = Throughput {
        op_type: op_type.to_string(),
        avg_mbps,
        avg_ops,
        total_bytes,
        total_ops: ok.len(),
        duration_secs: d,
        errors,
        segmented: compute_segmented(ops, segment_dur),
    };

    let single_sized = if !ok.is_empty() {
        Some(compute_single_sized(&ok))
    } else {
        None
    };

    let host_names: Vec<String> = ok
        .iter()
        .map(|o| o.endpoint.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let hosts = host_names.len();
    let clients = ok
        .iter()
        .map(|o| o.client_id.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .len();

    let first_errors: Vec<String> = ops
        .iter()
        .filter(|o| !o.successful())
        .take(3)
        .map(|o| o.err.clone())
        .collect();

    OpAnalysis {
        op_type: op_type.to_string(),
        throughput,
        throughput_by_host: HashMap::new(),
        start_time: time_rng.0,
        end_time: time_rng.1,
        single_sized,
        multi_sized: None,
        host_names,
        hosts,
        clients,
        concurrency,
        objects_per_operation: ok.first().map(|o| o.obj_per_op).unwrap_or(1),
        errors,
        first_errors,
        n,
        skipped: false,
    }
}

fn compute_segmented(ops: &[Operation], segment_dur: TimeDelta) -> Option<ThroughputSegmented> {
    if ops.is_empty() {
        return None;
    }
    let segments = ops.to_vec().aggregate(segment_dur.to_std().unwrap_or_default());
    if segments.is_empty() {
        return None;
    }

    let dur_s = segment_dur.num_milliseconds() as f64 / 1000.0;
    let dur_ms = segment_dur.num_milliseconds() as u64;

    let mut mbps_list: Vec<(f64, &crate::bench::Segment)> = segments
        .iter()
        .map(|s| {
            let mbps = if dur_s > 0.0 {
                (s.total_bytes as f64 / (1024.0 * 1024.0)) / dur_s
            } else {
                0.0
            };
            (mbps, s)
        })
        .collect();
    mbps_list.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let len = mbps_list.len();
    let default_seg = &segments[0];
    let fastest = mbps_list.last().map(|(v, s)| (*v, *s)).unwrap_or((0.0, default_seg));
    let median = mbps_list.get(len / 2).map(|(v, s)| (*v, *s)).unwrap_or((0.0, default_seg));
    let slowest = mbps_list.first().map(|(v, s)| (*v, *s)).unwrap_or((0.0, default_seg));

    let stats: Vec<SegmentStats> = segments
        .iter()
        .map(|s| {
            let mbps = if dur_s > 0.0 {
                (s.total_bytes as f64 / (1024.0 * 1024.0)) / dur_s
            } else {
                0.0
            };
            let ops_rate = if dur_s > 0.0 {
                s.ops_started as f64 / dur_s
            } else {
                0.0
            };
            SegmentStats { start: s.start, mbps, ops: ops_rate, bytes: s.total_bytes, errors: s.errors }
        })
        .collect();

    Some(ThroughputSegmented {
        segment_duration_ms: dur_ms,
        fastest_mbps: fastest.0,
        median_mbps: median.0,
        slowest_mbps: slowest.0,
        fastest_ops: stats.last().map(|s| s.ops).unwrap_or(0.0),
        median_ops: stats.get(stats.len() / 2).map(|s| s.ops).unwrap_or(0.0),
        slowest_ops: stats.first().map(|s| s.ops).unwrap_or(0.0),
        segments: stats,
        fastest_start: fastest.1.start,
        median_start: median.1.start,
        slowest_start: slowest.1.start,
    })
}

fn compute_single_sized(ops: &[&Operation]) -> SingleSizedRequests {
    let size = ops.first().map(|o| o.size).unwrap_or(0);
    let requests = ops.len();

    let mut durs: Vec<f64> = ops.iter().map(|o| o.duration().as_secs_f64() * 1000.0).collect();
    durs.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let avg = durs.iter().sum::<f64>() / durs.len() as f64;
    let median = percentile(&durs, 0.50);
    let p90 = percentile(&durs, 0.90);
    let p99 = percentile(&durs, 0.99);
    let fastest = durs.first().copied().unwrap_or(0.0);
    let slowest = durs.last().copied().unwrap_or(0.0);
    let variance = durs.iter().map(|d| (d - avg).powi(2)).sum::<f64>() / durs.len() as f64;

    let ttfb_vals: Vec<f64> = ops
        .iter()
        .filter_map(|o| o.ttfb().map(|t| t.as_secs_f64() * 1000.0))
        .collect();

    let (avg_ttfb, med_ttfb, p90_ttfb, p99_ttfb, fast_ttfb, slow_ttfb) = if ttfb_vals.is_empty() {
        (None, None, None, None, None, None)
    } else {
        let mut ttfb_sorted = ttfb_vals.clone();
        ttfb_sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        (
            Some(ttfb_vals.iter().sum::<f64>() / ttfb_vals.len() as f64),
            Some(percentile(&ttfb_sorted, 0.50)),
            Some(percentile(&ttfb_sorted, 0.90)),
            Some(percentile(&ttfb_sorted, 0.99)),
            Some(ttfb_sorted.first().copied().unwrap_or(0.0)),
            Some(ttfb_sorted.last().copied().unwrap_or(0.0)),
        )
    };

    SingleSizedRequests {
        size,
        requests,
        avg_duration_ms: avg,
        median_duration_ms: median,
        p90_duration_ms: p90,
        p99_duration_ms: p99,
        fastest_duration_ms: fastest,
        slowest_duration_ms: slowest,
        std_dev_ms: variance.sqrt(),
        avg_ttfb_ms: avg_ttfb,
        median_ttfb_ms: med_ttfb,
        p90_ttfb_ms: p90_ttfb,
        p99_ttfb_ms: p99_ttfb,
        fastest_ttfb_ms: fast_ttfb,
        slowest_ttfb_ms: slow_ttfb,
        first_access: None,
        last_access: None,
    }
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = (sorted.len() as f64 * p).ceil() as usize;
    sorted[(idx.saturating_sub(1)).min(sorted.len() - 1)]
}

// ---------------------------------------------------------------------------
// 对比 & 合并
// ---------------------------------------------------------------------------
pub fn compare(before: &Aggregated, after: &Aggregated) -> CompareResult {
    let mut diffs = Vec::new();
    for op in &after.operations {
        if let Some(before_op) = before.operations.iter().find(|o| o.op_type == op.op_type) {
            let mbps_diff = if before_op.throughput.avg_mbps > 0.0 {
                (op.throughput.avg_mbps - before_op.throughput.avg_mbps)
                    / before_op.throughput.avg_mbps
                    * 100.0
            } else {
                0.0
            };
            let ops_diff = if before_op.throughput.avg_ops > 0.0 {
                (op.throughput.avg_ops - before_op.throughput.avg_ops)
                    / before_op.throughput.avg_ops
                    * 100.0
            } else {
                0.0
            };
            diffs.push(OpDiff {
                op_type: op.op_type.clone(),
                before_mbps: before_op.throughput.avg_mbps,
                after_mbps: op.throughput.avg_mbps,
                mbps_diff_pct: mbps_diff,
                before_ops: before_op.throughput.avg_ops,
                after_ops: op.throughput.avg_ops,
                ops_diff_pct: ops_diff,
            });
        }
    }
    CompareResult { diffs }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompareResult {
    pub diffs: Vec<OpDiff>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpDiff {
    pub op_type: String,
    pub before_mbps: f64,
    pub after_mbps: f64,
    pub mbps_diff_pct: f64,
    pub before_ops: f64,
    pub after_ops: f64,
    pub ops_diff_pct: f64,
}

pub fn merge(op_sets: &[Vec<Operation>]) -> Vec<Operation> {
    if op_sets.is_empty() {
        return Vec::new();
    }
    let max_start = op_sets
        .iter()
        .filter_map(|ops| ops.iter().map(|o| o.start).min())
        .max()
        .unwrap_or_else(Utc::now);
    let min_end = op_sets
        .iter()
        .filter_map(|ops| ops.iter().map(|o| o.end).max())
        .min()
        .unwrap_or_else(Utc::now);

    op_sets
        .iter()
        .flat_map(|ops| {
            ops.iter()
                .filter(|o| o.start >= max_start && o.end <= min_end)
                .cloned()
                .collect::<Vec<_>>()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// CSV / JSON 序列化
// ---------------------------------------------------------------------------
pub fn write_csv_zst(ops: &[Operation], writer: &mut impl Write) -> crate::generator::Result<()> {
    let mut wtr = csv::WriterBuilder::new()
        .delimiter(b'\t')
        .from_writer(Vec::new());

    for (idx, op) in ops.iter().enumerate() {
        wtr.write_record(&[
            idx.to_string(),
            op.thread.to_string(),
            op.op_type.clone(),
            op.client_id.clone(),
            op.obj_per_op.to_string(),
            op.size.to_string(),
            op.endpoint.clone(),
            op.file.clone(),
            op.err.clone(),
            op.start.to_rfc3339(),
            op.first_byte.map(|t| t.to_rfc3339()).unwrap_or_default(),
            op.last_byte.map(|t| t.to_rfc3339()).unwrap_or_default(),
            op.end.to_rfc3339(),
            op.duration().as_nanos().to_string(),
            op.categories.to_string(),
        ])
        .map_err(|e| crate::generator::Error::Csv(e.to_string()))?;
    }

    let csv_data = wtr.into_inner().map_err(|e| crate::generator::Error::Csv(e.to_string()))?;
    let compressed = zstd::encode_all(csv_data.as_slice(), 3)
        .map_err(|e| crate::generator::Error::Zstd(e.to_string()))?;
    writer.write_all(&compressed)?;
    Ok(())
}

pub fn read_csv_zst(reader: &mut impl Read) -> crate::generator::Result<Vec<Operation>> {
    let mut compressed = Vec::new();
    reader.read_to_end(&mut compressed)?;
    let data =
        zstd::decode_all(compressed.as_slice()).map_err(|e| crate::generator::Error::Zstd(e.to_string()))?;

    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .has_headers(false)
        .from_reader(data.as_slice());

    let mut ops = Vec::new();
    for result in rdr.records() {
        let record = result.map_err(|e| crate::generator::Error::Csv(e.to_string()))?;
        if record.len() < 15 {
            continue;
        }
        ops.push(Operation {
            start: DateTime::parse_from_rfc3339(&record[9])
                .map(|t| t.with_timezone(&Utc))
                .unwrap_or_default(),
            end: DateTime::parse_from_rfc3339(&record[13])
                .map(|t| t.with_timezone(&Utc))
                .unwrap_or_default(),
            first_byte: {
                let s = &record[11];
                if s.is_empty() {
                    None
                } else {
                    DateTime::parse_from_rfc3339(s)
                        .ok()
                        .map(|t| t.with_timezone(&Utc))
                }
            },
            last_byte: {
                let s = &record[12];
                if s.is_empty() {
                    None
                } else {
                    DateTime::parse_from_rfc3339(s)
                        .ok()
                        .map(|t| t.with_timezone(&Utc))
                }
            },
            op_type: record[2].to_string(),
            err: record[8].to_string(),
            file: record[7].to_string(),
            client_id: record[3].to_string(),
            endpoint: record[6].to_string(),
            obj_per_op: record[4].parse().unwrap_or(1),
            size: record[5].parse().unwrap_or(0),
            thread: record[1].parse().unwrap_or(0),
            categories: record[14].parse().unwrap_or(0),
        });
    }
    Ok(ops)
}

pub fn write_json_zst(agg: &Aggregated, writer: &mut impl Write) -> crate::generator::Result<()> {
    let json = serde_json::to_vec_pretty(agg)?;
    let compressed = zstd::encode_all(json.as_slice(), 3)
        .map_err(|e| crate::generator::Error::Zstd(e.to_string()))?;
    writer.write_all(&compressed)?;
    Ok(())
}

pub fn read_json_zst(reader: &mut impl Read) -> crate::generator::Result<Aggregated> {
    let mut compressed = Vec::new();
    reader.read_to_end(&mut compressed)?;
    let data =
        zstd::decode_all(compressed.as_slice()).map_err(|e| crate::generator::Error::Zstd(e.to_string()))?;
    let agg: Aggregated = serde_json::from_slice(&data)?;
    Ok(agg)
}
