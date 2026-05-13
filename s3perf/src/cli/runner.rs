//! Benchmark runner entrypoints — one function per `s3perf` subcommand plus
//! the shared `run_benchmark` orchestrator.

use crate::aggregate::Aggregated;
use crate::bench::append::AppendBenchmark;
use crate::bench::delete::DeleteBenchmark;
use crate::bench::fanout::FanoutBenchmark;
use crate::bench::get::GetBenchmark;
use crate::bench::list::ListBenchmark;
use crate::bench::mixed::{MixedBenchmark, MixedDistrib};
use crate::bench::multipart::MultipartBenchmark;
use crate::bench::multipart_put::MultipartPutBenchmark;
use crate::bench::put::PutBenchmark;
use crate::bench::snowball::SnowballBenchmark;
use crate::bench::stat::StatBenchmark;
use crate::bench::zip::ZipBenchmark;
use crate::bench::Benchmark;
use crate::generator::ObjSize;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

use super::{collector_with_optional_influx, take_influx_join, BenchConfig};

/// Run the GET workload.
pub async fn run_get(
    bc: &BenchConfig,
    versions: usize,
    range: Option<(i64, i64)>,
    list_existing: bool,
) -> anyhow::Result<Aggregated> {
    let collector = collector_with_optional_influx()?;
    let mut common = bc.build_common(collector, "s3perf-get");
    common.versioned = versions > 1;
    common.versions = versions;
    let bm = GetBenchmark::new(common, range, list_existing);
    run_benchmark(Arc::new(bm)).await
}

/// Run the PUT workload.
pub async fn run_put(
    bc: &BenchConfig,
    md5: bool,
    checksum: Option<String>,
    use_post: bool,
) -> anyhow::Result<Aggregated> {
    let collector = collector_with_optional_influx()?;
    let checksum_type =
        crate::bench::checksum::ChecksumType::from_cli_flag(checksum.as_deref().unwrap_or(""))
            .or_else(|| {
                if md5 {
                    Some(crate::bench::checksum::ChecksumType::MD5)
                } else {
                    None
                }
            });

    let mut common = bc.build_common(collector, "s3perf-put");
    common.objects = 0;
    common.checksum = checksum_type;
    let bm = PutBenchmark::new(common, md5, checksum, use_post);
    run_benchmark(Arc::new(bm)).await
}

/// Run DELETE.
pub async fn run_delete(bc: &BenchConfig, batch_size: usize) -> anyhow::Result<Aggregated> {
    let collector = collector_with_optional_influx()?;
    let common = bc.build_common(collector, "s3perf-del");
    let bm = DeleteBenchmark::new(common, batch_size);
    run_benchmark(Arc::new(bm)).await
}

/// Run LIST.
pub async fn run_list(bc: &BenchConfig, versions: bool) -> anyhow::Result<Aggregated> {
    let collector = collector_with_optional_influx()?;
    let mut common = bc.build_common(collector, "s3perf-list");
    common.versioned = versions;
    common.versions = if versions { 2 } else { 1 };
    common.auto_term_dur = None;
    common.auto_term_scale = 0.075;
    let bm = ListBenchmark::new(common, versions);
    run_benchmark(Arc::new(bm)).await
}

/// Run HEAD/STAT.
pub async fn run_stat(bc: &BenchConfig) -> anyhow::Result<Aggregated> {
    let collector = collector_with_optional_influx()?;
    let mut common = bc.build_common(collector, "s3perf-stat");
    common.auto_term_dur = None;
    common.auto_term_scale = 0.075;
    let bm = StatBenchmark::new(common);
    run_benchmark(Arc::new(bm)).await
}

/// Run Mixed (GET/STAT/PUT/DELETE).
pub async fn run_mixed(
    bc: &BenchConfig,
    get_distrib: f64,
    stat_distrib: f64,
    put_distrib: f64,
    delete_distrib: f64,
) -> anyhow::Result<Aggregated> {
    let collector = collector_with_optional_influx()?;
    let common = bc.build_common(collector, "s3perf-mixed");
    let distrib = MixedDistrib {
        get: get_distrib,
        stat: stat_distrib,
        put: put_distrib,
        delete: delete_distrib,
    };
    let bm = MixedBenchmark::new(common, distrib);
    run_benchmark(Arc::new(bm)).await
}

/// Run versioned-objects mix.
pub async fn run_versioned(
    bc: &BenchConfig,
    get_distrib: f64,
    stat_distrib: f64,
    put_distrib: f64,
    delete_distrib: f64,
) -> anyhow::Result<Aggregated> {
    use crate::bench::versioned::{VersionedBenchmark, VersionedDistrib};
    let collector = collector_with_optional_influx()?;
    let mut common = bc.build_common(collector, "s3perf-ver");
    common.versioned = true;
    let dist = VersionedDistrib {
        get: get_distrib,
        stat: stat_distrib,
        put: put_distrib,
        delete: delete_distrib,
    };
    let bm = VersionedBenchmark::new(common, dist);
    run_benchmark(Arc::new(bm)).await
}

/// Run retention / object-lock workloads.
pub async fn run_retention(bc: &BenchConfig, versions: usize) -> anyhow::Result<Aggregated> {
    use crate::bench::retention::RetentionBenchmark;
    let collector = collector_with_optional_influx()?;
    let mut common = bc.build_common(collector, "s3perf-ret");
    common.versioned = true;
    common.locking = true;
    common.versions = versions;
    let bm = RetentionBenchmark::new(common);
    run_benchmark(Arc::new(bm)).await
}

/// Run multipart download stress.
pub async fn run_multipart(
    bc: &BenchConfig,
    part_size: usize,
    parts: usize,
    obj_name: String,
) -> anyhow::Result<Aggregated> {
    let collector = collector_with_optional_influx()?;
    let mut common = bc.build_common(collector, "s3perf-mp");
    common.objects = 0;
    common.obj_size = ObjSize::Fixed(part_size as i64);
    let bm = MultipartBenchmark::new(common, part_size, parts, obj_name);
    run_benchmark(Arc::new(bm)).await
}

/// Run multipart upload stress.
pub async fn run_multipart_put(
    bc: &BenchConfig,
    parts: usize,
    part_size: usize,
    part_concurrency: usize,
) -> anyhow::Result<Aggregated> {
    let collector = collector_with_optional_influx()?;
    let mut common = bc.build_common(collector, "s3perf-mpp");
    common.objects = 0;
    common.obj_size = ObjSize::Fixed(part_size as i64);
    let bm = MultipartPutBenchmark::new(common, parts, part_size, part_concurrency);
    run_benchmark(Arc::new(bm)).await
}

/// Run snowball Tar uploads.
pub async fn run_snowball(bc: &BenchConfig, objs_per: usize) -> anyhow::Result<Aggregated> {
    let collector = collector_with_optional_influx()?;
    let mut common = bc.build_common(collector, "s3perf-sb");
    common.objects = 0;
    let obj_bytes = match &bc.obj_size {
        ObjSize::Fixed(s) => *s as usize,
        _ => 512 * 1024,
    };
    let bm = SnowballBenchmark::new(common, obj_bytes, objs_per);
    run_benchmark(Arc::new(bm)).await
}

/// Run fan-out copy workload.
pub async fn run_fanout(bc: &BenchConfig, copies: usize) -> anyhow::Result<Aggregated> {
    let collector = collector_with_optional_influx()?;
    let mut common = bc.build_common(collector, "s3perf-fo");
    common.objects = 0;
    let obj_bytes = match &bc.obj_size {
        ObjSize::Fixed(s) => *s as usize,
        _ => 1024 * 1024,
    };
    let bm = FanoutBenchmark::new(common, copies, obj_bytes);
    run_benchmark(Arc::new(bm)).await
}

/// Run AppendObject workload.
pub async fn run_append(bc: &BenchConfig) -> anyhow::Result<Aggregated> {
    let collector = collector_with_optional_influx()?;
    let mut common = bc.build_common(collector, "s3perf-ap");
    common.objects = 0;
    common.auto_term_dur = None;
    common.auto_term_scale = 0.075;
    let obj_bytes = match &bc.obj_size {
        ObjSize::Fixed(s) => *s as usize,
        _ => 10 * 1024 * 1024,
    };
    let bm = AppendBenchmark::new(common, Some(obj_bytes as i64));
    run_benchmark(Arc::new(bm)).await
}

/// Run in-memory ZIP upload workload.
pub async fn run_zip(bc: &BenchConfig, entries: usize) -> anyhow::Result<Aggregated> {
    let collector = collector_with_optional_influx()?;
    let mut common = bc.build_common(collector, "s3perf-zip");
    common.objects = 0;
    let bm = ZipBenchmark::new(common, entries);
    run_benchmark(Arc::new(bm)).await
}

/// Run a benchmark from YAML (`s3perf run <file>`).
pub async fn execute_run_yaml(cfg: crate::config::RunFileConfig) -> anyhow::Result<Aggregated> {
    use crate::bench::s3_client::S3Config;
    use crate::bench::sse::SseConfig;
    use crate::cli::app::{parse_duration, parse_obj_size, parse_size};

    cfg.validate().map_err(|e| anyhow::anyhow!("{e}"))?;
    let r = &cfg.s3perf.remote;
    let p = &cfg.s3perf.params;
    let s3_config = S3Config {
        host: r.host.clone(),
        access_key: r.access_key.clone(),
        secret_key: r.secret_key.clone(),
        region: r.region.clone(),
        tls: r.tls,
        insecure: r.insecure,
        no_verify_ssl: r.insecure,
        ca_pem: None,
    };
    let duration = parse_duration(&p.duration).map_err(|e| anyhow::anyhow!(e))?;
    let mut obj_size = parse_obj_size(&p.obj.size).map_err(|e| anyhow::anyhow!(e))?;
    if p.obj.rand_size {
        let max = match &obj_size {
            ObjSize::Fixed(s) => *s,
            ObjSize::Random { max: m } => *m,
            ObjSize::Bucketed { buckets, .. } => {
                buckets.first().map(|(s, _)| *s).unwrap_or(1 << 20)
            }
        };
        obj_size = ObjSize::Random { max };
    }
    let autoterm_dur = parse_duration(&p.autoterm.dur).map_err(|e| anyhow::anyhow!(e))?;
    let dist = p.distribution.as_ref();
    let (g, st, pu, del) = match dist {
        Some(d) => (
            d.get / 100.0,
            d.stat / 100.0,
            d.put / 100.0,
            d.delete / 100.0,
        ),
        None => (0.45, 0.05, 0.25, 0.25),
    };
    let ps = parse_size("5MiB").map_err(|e| anyhow::anyhow!(e))? as usize;

    let bc = BenchConfig {
        s3_config,
        bucket: r.bucket.clone(),
        region: r.region.clone(),
        concurrency: p.concurrent,
        duration,
        obj_size,
        objects: p.objects,
        clear: !p.no_clear,
        autoterm: p.autoterm.enabled,
        autoterm_dur,
        autoterm_pct: p.autoterm.pct / 100.0,
        output: cfg.s3perf.bench_data.clone(),
        host_select: "weighed".parse().map_err(|e: String| anyhow::anyhow!(e))?,
        hosts: vec![r.host.clone()],
        no_prefix: false,
        prefix: None,
        sse: SseConfig::None,
        rps_limit: None,
    };

    match cfg.s3perf.benchmark.as_str() {
        "mixed" => run_mixed(&bc, g, st, pu, del).await,
        "get" => run_get(&bc, p.versions.max(1), None, false).await,
        "put" => run_put(&bc, false, None, false).await,
        "delete" => run_delete(&bc, 100).await,
        "list" => run_list(&bc, p.versions > 1).await,
        "stat" => run_stat(&bc).await,
        "versioned" => run_versioned(&bc, g, st, pu, del).await,
        "retention" => run_retention(&bc, if p.versions > 0 { p.versions } else { 5 }).await,
        "multipart" => run_multipart(&bc, ps, 200, "s3perf-multipart.bin".into()).await,
        "multipart-put" => run_multipart_put(&bc, 8, ps, 4).await,
        "snowball" => run_snowball(&bc, 4).await,
        "fanout" => run_fanout(&bc, 4).await,
        "append" => run_append(&bc).await,
        "zip" => run_zip(&bc, 8).await,
        other => anyhow::bail!("unsupported benchmark type: {other}"),
    }
}

/// Shared benchmark orchestration (prepare/start/analyze/cleanup).
pub async fn run_benchmark(bm: Arc<dyn Benchmark>) -> anyhow::Result<Aggregated> {
    let common = bm.common();
    let dur = common.duration;
    let monitor = Arc::new(crate::api::BenchmarkMonitor::new());

    let api_task = if let Ok(addr_s) = std::env::var("S3PERF_API_LISTEN") {
        let addr: SocketAddr = addr_s
            .parse()
            .map_err(|e| anyhow::anyhow!("S3PERF_API_LISTEN invalid address {addr_s}: {e}"))?;
        let m = monitor.clone();
        Some(tokio::spawn(async move {
            if let Err(e) = crate::api::http::serve_monitor(m, addr).await {
                tracing::error!("HTTP monitoring server exited: {e}");
            }
        }))
    } else {
        None
    };

    let tui_state = if std::env::var("S3PERF_TUI").ok().as_deref() == Some("1") {
        let cancel = CancellationToken::new();
        let st = Arc::new(crate::tui::TuiState::new(cancel.clone()));
        st.set_phase("Prepare", "Provisioning bucket and seed objects");
        st.set_progress(0.05);
        st.clone().spawn_render_loop();
        Some((st, cancel))
    } else {
        None
    };

    // Phase 1: Prepare
    eprintln!("Prepare: creating bucket and seeding objects...");
    monitor.set_status("prepare");
    let ctx = CancellationToken::new();
    bm.prepare(&ctx).await?;
    if let Some((ref st, _)) = tui_state {
        st.set_phase("Benchmark", "Benchmarking");
        st.set_progress(0.33);
    }

    // Phase 2: Start
    eprintln!("Benchmark: running for {}s...", dur.as_secs());
    monitor.start();
    let ctx = CancellationToken::new();
    let (broadcast_tx, _) = tokio::sync::broadcast::channel(4);
    let wait_rx = broadcast_tx.subscribe();
    let ping_tx = broadcast_tx.clone();
    let _ping_task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let _ = ping_tx.send(());
    });

    // Optional auto-stop based on throughput stability
    let ctx = if let Some(autoterm_dur) = common.auto_term_dur {
        common
            .collector
            .auto_term(ctx, "", common.auto_term_scale, 100, 25, autoterm_dur)
    } else {
        ctx
    };

    let bm_clone = Arc::clone(&bm);
    let bench_ctx = ctx.clone();
    let mut bench_handle = tokio::spawn(async move { bm_clone.start(&bench_ctx, wait_rx).await });

    if let Some((ref st, _)) = tui_state {
        st.set_progress(0.4);
    }

    tokio::select! {
        _ = tokio::time::sleep(dur + Duration::from_secs(5)) => {
            ctx.cancel();
        }
        _ = &mut bench_handle => {}
    }
    let bench_res = bench_handle.await;
    if let Err(e) = bench_res {
        tracing::warn!("benchmark task: {e}");
    }
    drop(broadcast_tx);

    // Phase 3: Analyze
    eprintln!("Analyzing results...");
    monitor.set_status("analyze");
    if let Some((ref st, _)) = tui_state {
        st.set_phase("Analyze", "Summarizing metrics");
        st.set_progress(0.66);
    }
    let ops = bm.ops();
    monitor.add_ops(&ops);
    let ok_ops: Vec<_> = ops.iter().filter(|o| o.successful()).cloned().collect();
    let err_ops: Vec<_> = ops.iter().filter(|o| !o.successful()).cloned().collect();

    let agg = crate::aggregate::analyze(&ops, Duration::from_secs(1), common.concurrency);
    monitor.set_aggregated(agg.clone());

    // Render human-readable summary tables
    eprintln!();
    eprintln!("==========================================");
    eprintln!("  s3perf benchmark results");
    eprintln!("==========================================");
    eprintln!("  operations: {}", ops.len());
    eprintln!("  successful: {}", ok_ops.len());
    eprintln!("  failed: {}", err_ops.len());
    if let Some(th) = &agg.mixed_server_stats {
        eprintln!(
            "  throughput: {:.2} MiB/s, {:.2} obj/s",
            th.avg_mbps, th.avg_ops
        );
        eprintln!("  wall time: {:.1}s", th.duration_secs);
    }
    eprintln!();
    for op_analysis in &agg.operations {
        eprintln!("  [{}]", op_analysis.op_type);
        eprintln!(
            "    throughput: {:.2} MiB/s, {:.2} obj/s",
            op_analysis.throughput.avg_mbps, op_analysis.throughput.avg_ops,
        );
        if let Some(ss) = &op_analysis.single_sized {
            eprintln!(
                "    latency (ms): avg={:.1} median={:.1} P90={:.1} P99={:.1}",
                ss.avg_duration_ms, ss.median_duration_ms, ss.p90_duration_ms, ss.p99_duration_ms,
            );
        }
        eprintln!("    errors: {}", op_analysis.errors);
        for e in &op_analysis.first_errors {
            eprintln!("    error detail: {e}");
        }
        eprintln!();
    }
    eprintln!("==========================================");

    // Phase 4: Cleanup
    if common.clear {
        eprintln!("Cleanup: removing benchmark objects...");
        monitor.set_status("cleanup");
        if let Some((ref st, _)) = tui_state {
            st.set_phase("Cleanup", "Deleting benchmark objects");
            st.set_progress(0.88);
        }
        bm.cleanup(&CancellationToken::new()).await;
    }

    let out_name = common.bench_data.clone();
    // Optional disk export (.csv.zst + .json.zst)
    if let Some(ref path) = common.bench_data {
        if !common.discard_output {
            eprintln!("Saving data to: {path}");
            let file = std::fs::File::create(path)?;
            let mut writer = std::io::BufWriter::new(file);
            crate::aggregate::write_csv_zst(&ops, &mut writer)?;
            // Mirror aggregate stats to JSON.zst beside the CSV export
            let json_path = path.replace(".csv.zst", ".json.zst");
            let json_file = std::fs::File::create(&json_path)?;
            let mut json_writer = std::io::BufWriter::new(json_file);
            crate::aggregate::write_json_zst(&agg, &mut json_writer)?;
        }
    }

    monitor.set_done(out_name.clone());

    if let Some((st, cancel)) = tui_state {
        st.set_progress(1.0);
        st.set_done();
        cancel.cancel();
        tokio::time::sleep(Duration::from_millis(600)).await;
    }

    if let Some(t) = api_task {
        t.abort();
    }

    drop(bm);
    if let Some(h) = take_influx_join() {
        let _ = h.await;
    }

    Ok(agg)
}
