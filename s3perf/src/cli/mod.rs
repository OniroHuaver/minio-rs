//! CLI wiring: parsing, benchmarks, orchestration.

pub mod app;

use crate::bench::delete::DeleteBenchmark;
use crate::bench::get::GetBenchmark;
use crate::bench::list::ListBenchmark;
use crate::bench::mixed::MixedBenchmark;
use crate::bench::put::PutBenchmark;
use crate::bench::stat::StatBenchmark;
use crate::bench::collector::OpsCollector;
use crate::bench::s3_client::S3Config;
use crate::bench::sse::SseConfig;
use crate::bench::{Benchmark, Common, HostSelect};
use crate::generator::{DefaultSource, ObjSize, Source};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Influx URL from the outer `Cli` (`main` installs it once per process).
static INFLUX_URL: OnceLock<Option<String>> = OnceLock::new();
/// Background Influx fan-out task handle for this run.
static INFLUX_JOIN: Mutex<Option<tokio::task::JoinHandle<()>>> = Mutex::new(None);

pub fn set_influx_url(url: Option<String>) {
    let _ = INFLUX_URL.set(url);
}

fn influx_url_static() -> Option<&'static str> {
    INFLUX_URL
        .get()
        .and_then(|o| o.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// Build a collector and optionally spawn [`crate::influxdb::InfluxWriter`].
fn collector_with_optional_influx() -> anyhow::Result<Arc<OpsCollector>> {
    use tokio::sync::mpsc;
    *INFLUX_JOIN.lock().unwrap() = None;
    let Some(raw) = influx_url_static() else {
        return Ok(Arc::new(OpsCollector::new()));
    };
    let cfg = crate::influxdb::parse_influx_url(raw)
        .map_err(|e| anyhow::anyhow!("InfluxDB URL: {e}"))?;
    let (tx, rx) = mpsc::unbounded_channel();
    let coll = Arc::new(OpsCollector::with_influx_fanout(Some(tx)));
    let writer = crate::influxdb::InfluxWriter::new(cfg, rx);
    let h = tokio::spawn(async move {
        writer.run().await;
    });
    *INFLUX_JOIN.lock().unwrap() = Some(h);
    Ok(coll)
}

fn take_influx_join() -> Option<tokio::task::JoinHandle<()>> {
    INFLUX_JOIN.lock().unwrap().take()
}

fn new_host_inflight(hosts: &[String]) -> Arc<Mutex<Vec<usize>>> {
    let n = hosts.len().max(1);
    Arc::new(Mutex::new(vec![0; n]))
}

/// Strip `--remote-hosts` from argv so coordinators can forward the rest to agents unchanged.

fn args_without_remote_hosts_flag() -> Vec<String> {
    let argv: Vec<String> = std::env::args().collect();
    let mut out = Vec::new();
    if !argv.is_empty() {
        out.push(argv[0].clone());
    }
    let mut i = 1;
    while i < argv.len() {
        let a = argv[i].as_str();
        if a == "--remote-hosts" {
            i += 1;
            if i < argv.len() && !argv[i].starts_with('-') {
                i += 1;
            }
            continue;
        }
        if a.starts_with("--remote-hosts=") {
            i += 1;
            continue;
        }
        out.push(argv[i].clone());
        i += 1;
    }
    out
}

/// Subcommand name used by the coordinator when `--remote-hosts` lists remote agents (`None` otherwise).
pub fn distributed_bench_subcommand(cmd: &crate::cli::app::Commands) -> Option<&'static str> {
    use crate::cli::app::Commands;
    match cmd {
        Commands::Mixed { .. } => Some("mixed"),
        Commands::Get { .. } => Some("get"),
        Commands::Put { .. } => Some("put"),
        Commands::Delete { .. } => Some("delete"),
        Commands::List { .. } => Some("list"),
        Commands::Stat {} => Some("stat"),
        Commands::Versioned { .. } => Some("versioned"),
        Commands::Retention {} => Some("retention"),
        Commands::Multipart { .. } => Some("multipart"),
        Commands::MultipartPut { .. } => Some("multipart-put"),
        Commands::Snowball { .. } => Some("snowball"),
        Commands::Fanout { .. } => Some("fanout"),
        Commands::Append {} => Some("append"),
        Commands::Zip { .. } => Some("zip"),
        _ => None,
    }
}

pub async fn run_as_coordinator_if_requested(
    cli: &crate::cli::app::Cli,
    command_name: &str,
) -> anyhow::Result<bool> {
    let Some(raw) = cli.remote_hosts.as_deref() else {
        return Ok(false);
    };
    let hosts: Vec<String> = raw
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if hosts.is_empty() {
        return Ok(false);
    }
    let args = args_without_remote_hosts_flag();
    crate::server::run_server_benchmark(&hosts, command_name, HashMap::new(), args)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    Ok(true)
}

/// Run the GET workload.
pub async fn run_get(
    config: S3Config,
    bucket: String,
    region: String,
    concurrency: usize,
    duration: Duration,
    obj_size: ObjSize,
    objects: usize,
    versions: usize,
    range: Option<(i64, i64)>,
    list_existing: bool,
    clear: bool,
    autoterm: bool,
    autoterm_dur: Duration,
    autoterm_pct: f64,
    output: Option<String>,
    host_select: HostSelect,
    hosts: Vec<String>,
    no_prefix: bool,
    prefix: Option<String>,
    sse: crate::bench::sse::SseConfig,
    rps_limit: Option<f64>,
) -> anyhow::Result<()> {
    let collector = collector_with_optional_influx()?;
    let cf = crate::bench::s3_client::client_factory(config.clone(), hosts.clone());
    let host_inflight = new_host_inflight(&hosts);

    let common = Common {
        concurrency,
        duration,
        bucket: bucket.clone(),
        location: region,
        source: Arc::new({
            let p = prefix.clone().unwrap_or_else(|| "s3perf-get".into());
            let os = obj_size.clone();
            move || Box::new(DefaultSource::new(p.clone(), os.clone(), rand::random())) as Box<dyn Source>
        }),
        client_factory: cf,
        collector: collector.clone(),
        client_idx: 0,
        total_clients: 1,
        client_mode: false,
        clear,
        discard_output: output.is_none(),
        versioned: versions > 1,
        locking: false,
        auto_term_dur: if autoterm { Some(autoterm_dur) } else { None },
        auto_term_scale: autoterm_pct,
        rps_limit,
        host_select,
        no_prefix,
        custom_prefix: prefix,
        obj_size,
        objects,
        versions,
        bench_data: output,
        analyze_only: false,
        hosts,
        host_inflight,
        rps_limiter: crate::bench::rate_limiter::opt_rps_limiter(rps_limit),
        sse,
        checksum: None,
    };

    let bm = GetBenchmark::new(common, range, list_existing);
    run_benchmark(Arc::new(bm)).await
}

/// Run the PUT workload.
pub async fn run_put(
    config: S3Config,
    bucket: String,
    region: String,
    concurrency: usize,
    duration: Duration,
    obj_size: ObjSize,
    md5: bool,
    checksum: Option<String>,
    use_post: bool,
    clear: bool,
    autoterm: bool,
    autoterm_dur: Duration,
    autoterm_pct: f64,
    output: Option<String>,
    host_select: HostSelect,
    hosts: Vec<String>,
    no_prefix: bool,
    prefix: Option<String>,
    sse: crate::bench::sse::SseConfig,
    rps_limit: Option<f64>,
) -> anyhow::Result<()> {
    let collector = collector_with_optional_influx()?;
    let cf = crate::bench::s3_client::client_factory(config.clone(), hosts.clone());
    let host_inflight = new_host_inflight(&hosts);

    let checksum_type = crate::bench::checksum::ChecksumType::from_cli_flag(
        checksum.as_deref().unwrap_or(""),
    ).or_else(|| if md5 { Some(crate::bench::checksum::ChecksumType::MD5) } else { None });

    let common = Common {
        concurrency,
        duration,
        bucket: bucket.clone(),
        location: region,
        source: Arc::new({
            let p = prefix.clone().unwrap_or_else(|| "s3perf-put".into());
            let os = obj_size.clone();
            move || Box::new(DefaultSource::new(p.clone(), os.clone(), rand::random())) as Box<dyn Source>
        }),
        client_factory: cf,
        collector: collector.clone(),
        client_idx: 0,
        total_clients: 1,
        client_mode: false,
        clear,
        discard_output: output.is_none(),
        versioned: false,
        locking: false,
        auto_term_dur: if autoterm { Some(autoterm_dur) } else { None },
        auto_term_scale: autoterm_pct,
        rps_limit,
        host_select,
        no_prefix,
        custom_prefix: prefix,
        obj_size,
        objects: 0,
        versions: 1,
        bench_data: output,
        analyze_only: false,
        hosts,
        host_inflight,
        rps_limiter: crate::bench::rate_limiter::opt_rps_limiter(rps_limit),
        sse,
        checksum: checksum_type,
    };

    let bm = PutBenchmark::new(common, md5, checksum, use_post);
    run_benchmark(Arc::new(bm)).await
}

/// Run DELETE.
pub async fn run_delete(
    config: S3Config,
    bucket: String,
    region: String,
    concurrency: usize,
    duration: Duration,
    obj_size: ObjSize,
    objects: usize,
    batch_size: usize,
    clear: bool,
    autoterm: bool,
    autoterm_dur: Duration,
    autoterm_pct: f64,
    output: Option<String>,
    host_select: HostSelect,
    hosts: Vec<String>,
    no_prefix: bool,
    prefix: Option<String>,
    sse: crate::bench::sse::SseConfig,
    rps_limit: Option<f64>,
) -> anyhow::Result<()> {
    let collector = collector_with_optional_influx()?;
    let cf = crate::bench::s3_client::client_factory(config.clone(), hosts.clone());
    let host_inflight = new_host_inflight(&hosts);

    let common = Common {
        concurrency,
        duration,
        bucket: bucket.clone(),
        location: region,
        source: Arc::new({
            let p = prefix.clone().unwrap_or_else(|| "s3perf-del".into());
            let os = obj_size.clone();
            move || Box::new(DefaultSource::new(p.clone(), os.clone(), rand::random())) as Box<dyn Source>
        }),
        client_factory: cf,
        collector: collector.clone(),
        client_idx: 0,
        total_clients: 1,
        client_mode: false,
        clear,
        discard_output: output.is_none(),
        versioned: false,
        locking: false,
        auto_term_dur: if autoterm { Some(autoterm_dur) } else { None },
        auto_term_scale: autoterm_pct,
        rps_limit,
        host_select,
        no_prefix,
        custom_prefix: prefix,
        obj_size,
        objects,
        versions: 1,
        bench_data: output,
        analyze_only: false,
        hosts,
        host_inflight,
        rps_limiter: crate::bench::rate_limiter::opt_rps_limiter(rps_limit),
        sse,
        checksum: None,
    };

    let bm = DeleteBenchmark::new(common, batch_size);
    run_benchmark(Arc::new(bm)).await
}

/// Run LIST.
pub async fn run_list(
    config: S3Config,
    bucket: String,
    region: String,
    concurrency: usize,
    duration: Duration,
    obj_size: ObjSize,
    objects: usize,
    versions: bool,
    clear: bool,
    output: Option<String>,
    host_select: HostSelect,
    hosts: Vec<String>,
    no_prefix: bool,
    prefix: Option<String>,
    sse: crate::bench::sse::SseConfig,
    rps_limit: Option<f64>,
) -> anyhow::Result<()> {
    let collector = collector_with_optional_influx()?;
    let cf = crate::bench::s3_client::client_factory(config.clone(), hosts.clone());
    let host_inflight = new_host_inflight(&hosts);

    let common = Common {
        concurrency,
        duration,
        bucket: bucket.clone(),
        location: region,
        source: Arc::new({
            let p = prefix.clone().unwrap_or_else(|| "s3perf-list".into());
            let os = obj_size.clone();
            move || Box::new(DefaultSource::new(p.clone(), os.clone(), rand::random())) as Box<dyn Source>
        }),
        client_factory: cf,
        collector: collector.clone(),
        client_idx: 0,
        total_clients: 1,
        client_mode: false,
        clear,
        discard_output: output.is_none(),
        versioned: versions,
        locking: false,
        auto_term_dur: None,
        auto_term_scale: 0.075,
        rps_limit,
        host_select,
        no_prefix,
        custom_prefix: prefix,
        obj_size,
        objects,
        versions: if versions { 2 } else { 1 },
        bench_data: output,
        analyze_only: false,
        hosts,
        host_inflight,
        rps_limiter: crate::bench::rate_limiter::opt_rps_limiter(rps_limit),
        sse,
        checksum: None,
    };

    let bm = ListBenchmark::new(common, versions);
    run_benchmark(Arc::new(bm)).await
}

/// Run HEAD/STAT.
pub async fn run_stat(
    config: S3Config,
    bucket: String,
    region: String,
    concurrency: usize,
    duration: Duration,
    obj_size: ObjSize,
    objects: usize,
    clear: bool,
    output: Option<String>,
    host_select: HostSelect,
    hosts: Vec<String>,
    no_prefix: bool,
    prefix: Option<String>,
    sse: crate::bench::sse::SseConfig,
    rps_limit: Option<f64>,
) -> anyhow::Result<()> {
    let collector = collector_with_optional_influx()?;
    let cf = crate::bench::s3_client::client_factory(config.clone(), hosts.clone());
    let host_inflight = new_host_inflight(&hosts);

    let common = Common {
        concurrency,
        duration,
        bucket: bucket.clone(),
        location: region,
        source: Arc::new({
            let p = prefix.clone().unwrap_or_else(|| "s3perf-stat".into());
            let os = obj_size.clone();
            move || Box::new(DefaultSource::new(p.clone(), os.clone(), rand::random())) as Box<dyn Source>
        }),
        client_factory: cf,
        collector: collector.clone(),
        client_idx: 0,
        total_clients: 1,
        client_mode: false,
        clear,
        discard_output: output.is_none(),
        versioned: false,
        locking: false,
        auto_term_dur: None,
        auto_term_scale: 0.075,
        rps_limit,
        host_select,
        no_prefix,
        custom_prefix: prefix,
        obj_size,
        objects,
        versions: 1,
        bench_data: output,
        analyze_only: false,
        hosts,
        host_inflight,
        rps_limiter: crate::bench::rate_limiter::opt_rps_limiter(rps_limit),
        sse,
        checksum: None,
    };

    let bm = StatBenchmark::new(common);
    run_benchmark(Arc::new(bm)).await
}

/// Run Mixed (GET/STAT/PUT/DELETE).
pub async fn run_mixed(
    config: S3Config,
    bucket: String,
    region: String,
    concurrency: usize,
    duration: Duration,
    obj_size: ObjSize,
    objects: usize,
    get_distrib: f64,
    stat_distrib: f64,
    put_distrib: f64,
    delete_distrib: f64,
    clear: bool,
    autoterm: bool,
    autoterm_dur: Duration,
    autoterm_pct: f64,
    output: Option<String>,
    host_select: HostSelect,
    hosts: Vec<String>,
    no_prefix: bool,
    prefix: Option<String>,
    sse: crate::bench::sse::SseConfig,
    rps_limit: Option<f64>,
) -> anyhow::Result<()> {
    let collector = collector_with_optional_influx()?;
    let cf = crate::bench::s3_client::client_factory(config.clone(), hosts.clone());
    let host_inflight = new_host_inflight(&hosts);

    let common = Common {
        concurrency,
        duration,
        bucket: bucket.clone(),
        location: region,
        source: Arc::new({
            let p = prefix.clone().unwrap_or_else(|| "s3perf-mixed".into());
            let os = obj_size.clone();
            move || Box::new(DefaultSource::new(p.clone(), os.clone(), rand::random())) as Box<dyn Source>
        }),
        client_factory: cf,
        collector: collector.clone(),
        client_idx: 0,
        total_clients: 1,
        client_mode: false,
        clear,
        discard_output: output.is_none(),
        versioned: false,
        locking: false,
        auto_term_dur: if autoterm { Some(autoterm_dur) } else { None },
        auto_term_scale: autoterm_pct,
        rps_limit,
        host_select,
        no_prefix,
        custom_prefix: prefix,
        obj_size,
        objects,
        versions: 1,
        bench_data: output,
        analyze_only: false,
        hosts,
        host_inflight,
        rps_limiter: crate::bench::rate_limiter::opt_rps_limiter(rps_limit),
        sse,
        checksum: None,
    };

    let distrib = crate::bench::mixed::MixedDistrib {
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
    config: S3Config,
    bucket: String,
    region: String,
    concurrency: usize,
    duration: Duration,
    obj_size: ObjSize,
    objects: usize,
    get_distrib: f64,
    stat_distrib: f64,
    put_distrib: f64,
    delete_distrib: f64,
    clear: bool,
    autoterm: bool,
    autoterm_dur: Duration,
    autoterm_pct: f64,
    output: Option<String>,
    host_select: HostSelect,
    hosts: Vec<String>,
    no_prefix: bool,
    prefix: Option<String>,
    sse: SseConfig,
    rps_limit: Option<f64>,
) -> anyhow::Result<()> {
    use crate::bench::versioned::{VersionedBenchmark, VersionedDistrib};
    let collector = collector_with_optional_influx()?;
    let cf = crate::bench::s3_client::client_factory(config.clone(), hosts.clone());
    let host_inflight = new_host_inflight(&hosts);

    let common = Common {
        concurrency, duration, bucket: bucket.clone(), location: region,
        source: Arc::new({
            let p = prefix.clone().unwrap_or_else(|| "s3perf-ver".into());
            let os = obj_size.clone();
            move || Box::new(DefaultSource::new(p.clone(), os.clone(), rand::random())) as Box<dyn Source>
        }),
        client_factory: cf, collector: collector.clone(),
        client_idx: 0, total_clients: 1, client_mode: false, clear,
        discard_output: output.is_none(), versioned: true, locking: false,
        auto_term_dur: if autoterm { Some(autoterm_dur) } else { None },
        auto_term_scale: autoterm_pct, rps_limit, host_select,
        no_prefix, custom_prefix: prefix, obj_size, objects, versions: 1,
        bench_data: output, analyze_only: false, hosts, host_inflight, rps_limiter: crate::bench::rate_limiter::opt_rps_limiter(rps_limit), sse, checksum: None,
    };

    let dist = VersionedDistrib { get: get_distrib, stat: stat_distrib, put: put_distrib, delete: delete_distrib };
    let bm = VersionedBenchmark::new(common, dist);
    run_benchmark(Arc::new(bm)).await
}

/// Run retention / object-lock workloads.
pub async fn run_retention(
    config: S3Config,
    bucket: String,
    region: String,
    concurrency: usize,
    duration: Duration,
    obj_size: ObjSize,
    objects: usize,
    versions: usize,
    clear: bool,
    autoterm: bool,
    autoterm_dur: Duration,
    autoterm_pct: f64,
    output: Option<String>,
    host_select: HostSelect,
    hosts: Vec<String>,
    no_prefix: bool,
    prefix: Option<String>,
    sse: SseConfig,
    rps_limit: Option<f64>,
) -> anyhow::Result<()> {
    use crate::bench::retention::RetentionBenchmark;
    let collector = collector_with_optional_influx()?;
    let cf = crate::bench::s3_client::client_factory(config.clone(), hosts.clone());
    let host_inflight = new_host_inflight(&hosts);

    let common = Common {
        concurrency, duration, bucket: bucket.clone(), location: region,
        source: Arc::new({
            let p = prefix.clone().unwrap_or_else(|| "s3perf-ret".into());
            let os = obj_size.clone();
            move || Box::new(DefaultSource::new(p.clone(), os.clone(), rand::random())) as Box<dyn Source>
        }),
        client_factory: cf, collector: collector.clone(),
        client_idx: 0, total_clients: 1, client_mode: false, clear,
        discard_output: output.is_none(), versioned: true, locking: true,
        auto_term_dur: if autoterm { Some(autoterm_dur) } else { None },
        auto_term_scale: autoterm_pct, rps_limit, host_select,
        no_prefix, custom_prefix: prefix, obj_size, objects, versions,
        bench_data: output, analyze_only: false, hosts, host_inflight, rps_limiter: crate::bench::rate_limiter::opt_rps_limiter(rps_limit), sse, checksum: None,
    };

    let bm = RetentionBenchmark::new(common);
    run_benchmark(Arc::new(bm)).await
}

/// Run multipart download stress.
pub async fn run_multipart(
    config: S3Config,
    bucket: String,
    region: String,
    concurrency: usize,
    duration: Duration,
    part_size: usize,
    parts: usize,
    obj_name: String,
    clear: bool,
    autoterm: bool,
    autoterm_dur: Duration,
    autoterm_pct: f64,
    output: Option<String>,
    host_select: HostSelect,
    hosts: Vec<String>,
    no_prefix: bool,
    prefix: Option<String>,
    sse: SseConfig,
    rps_limit: Option<f64>,
) -> anyhow::Result<()> {
    use crate::bench::multipart::MultipartBenchmark;
    let collector = collector_with_optional_influx()?;
    let cf = crate::bench::s3_client::client_factory(config.clone(), hosts.clone());
    let host_inflight = new_host_inflight(&hosts);
    let obj_size = ObjSize::Fixed(part_size as i64);

    let common = Common {
        concurrency, duration, bucket: bucket.clone(), location: region,
        source: Arc::new({
            let p = prefix.clone().unwrap_or_else(|| "s3perf-mp".into());
            let os = obj_size.clone();
            move || Box::new(DefaultSource::new(p.clone(), os.clone(), rand::random())) as Box<dyn Source>
        }),
        client_factory: cf, collector: collector.clone(),
        client_idx: 0, total_clients: 1, client_mode: false, clear,
        discard_output: output.is_none(), versioned: false, locking: false,
        auto_term_dur: if autoterm { Some(autoterm_dur) } else { None },
        auto_term_scale: autoterm_pct, rps_limit, host_select,
        no_prefix, custom_prefix: prefix, obj_size, objects: 0, versions: 1,
        bench_data: output, analyze_only: false, hosts, host_inflight, rps_limiter: crate::bench::rate_limiter::opt_rps_limiter(rps_limit), sse, checksum: None,
    };

    let bm = MultipartBenchmark::new(common, part_size, parts, obj_name);
    run_benchmark(Arc::new(bm)).await
}

/// Run multipart upload stress.
pub async fn run_multipart_put(
    config: S3Config,
    bucket: String,
    region: String,
    concurrency: usize,
    duration: Duration,
    parts: usize,
    part_size: usize,
    part_concurrency: usize,
    clear: bool,
    autoterm: bool,
    autoterm_dur: Duration,
    autoterm_pct: f64,
    output: Option<String>,
    host_select: HostSelect,
    hosts: Vec<String>,
    no_prefix: bool,
    prefix: Option<String>,
    sse: SseConfig,
    rps_limit: Option<f64>,
) -> anyhow::Result<()> {
    use crate::bench::multipart_put::MultipartPutBenchmark;
    let collector = collector_with_optional_influx()?;
    let cf = crate::bench::s3_client::client_factory(config.clone(), hosts.clone());
    let host_inflight = new_host_inflight(&hosts);
    let obj_size = ObjSize::Fixed(part_size as i64);

    let common = Common {
        concurrency, duration, bucket: bucket.clone(), location: region,
        source: Arc::new({
            let p = prefix.clone().unwrap_or_else(|| "s3perf-mpp".into());
            let os = obj_size.clone();
            move || Box::new(DefaultSource::new(p.clone(), os.clone(), rand::random())) as Box<dyn Source>
        }),
        client_factory: cf, collector: collector.clone(),
        client_idx: 0, total_clients: 1, client_mode: false, clear,
        discard_output: output.is_none(), versioned: false, locking: false,
        auto_term_dur: if autoterm { Some(autoterm_dur) } else { None },
        auto_term_scale: autoterm_pct, rps_limit, host_select,
        no_prefix, custom_prefix: prefix, obj_size, objects: 0, versions: 1,
        bench_data: output, analyze_only: false, hosts, host_inflight, rps_limiter: crate::bench::rate_limiter::opt_rps_limiter(rps_limit), sse, checksum: None,
    };

    let bm = MultipartPutBenchmark::new(common, parts, part_size, part_concurrency);
    run_benchmark(Arc::new(bm)).await
}

/// Run snowball Tar uploads.
pub async fn run_snowball(
    config: S3Config,
    bucket: String,
    region: String,
    concurrency: usize,
    duration: Duration,
    obj_size: ObjSize,
    objs_per: usize,
    clear: bool,
    autoterm: bool,
    autoterm_dur: Duration,
    autoterm_pct: f64,
    output: Option<String>,
    host_select: HostSelect,
    hosts: Vec<String>,
    no_prefix: bool,
    prefix: Option<String>,
    sse: SseConfig,
    rps_limit: Option<f64>,
) -> anyhow::Result<()> {
    use crate::bench::snowball::SnowballBenchmark;
    let collector = collector_with_optional_influx()?;
    let cf = crate::bench::s3_client::client_factory(config.clone(), hosts.clone());
    let host_inflight = new_host_inflight(&hosts);
    let obj_bytes = match &obj_size {
        ObjSize::Fixed(s) => *s as usize,
        _ => 512 * 1024,
    };

    let common = Common {
        concurrency, duration, bucket: bucket.clone(), location: region,
        source: Arc::new({
            let p = prefix.clone().unwrap_or_else(|| "s3perf-sb".into());
            let os = obj_size.clone();
            move || Box::new(DefaultSource::new(p.clone(), os.clone(), rand::random())) as Box<dyn Source>
        }),
        client_factory: cf, collector: collector.clone(),
        client_idx: 0, total_clients: 1, client_mode: false, clear,
        discard_output: output.is_none(), versioned: false, locking: false,
        auto_term_dur: if autoterm { Some(autoterm_dur) } else { None },
        auto_term_scale: autoterm_pct, rps_limit, host_select,
        no_prefix, custom_prefix: prefix, obj_size, objects: 0, versions: 1,
        bench_data: output, analyze_only: false, hosts, host_inflight, rps_limiter: crate::bench::rate_limiter::opt_rps_limiter(rps_limit), sse, checksum: None,
    };

    let bm = SnowballBenchmark::new(common, obj_bytes, objs_per);
    run_benchmark(Arc::new(bm)).await
}

/// Run fan-out copy workload.
pub async fn run_fanout(
    config: S3Config,
    bucket: String,
    region: String,
    concurrency: usize,
    duration: Duration,
    obj_size: ObjSize,
    copies: usize,
    clear: bool,
    autoterm: bool,
    autoterm_dur: Duration,
    autoterm_pct: f64,
    output: Option<String>,
    host_select: HostSelect,
    hosts: Vec<String>,
    no_prefix: bool,
    prefix: Option<String>,
    sse: SseConfig,
    rps_limit: Option<f64>,
) -> anyhow::Result<()> {
    use crate::bench::fanout::FanoutBenchmark;
    let collector = collector_with_optional_influx()?;
    let cf = crate::bench::s3_client::client_factory(config.clone(), hosts.clone());
    let host_inflight = new_host_inflight(&hosts);
    let obj_bytes = match &obj_size {
        ObjSize::Fixed(s) => *s as usize,
        _ => 1024 * 1024,
    };

    let common = Common {
        concurrency, duration, bucket: bucket.clone(), location: region,
        source: Arc::new({
            let p = prefix.clone().unwrap_or_else(|| "s3perf-fo".into());
            let os = obj_size.clone();
            move || Box::new(DefaultSource::new(p.clone(), os.clone(), rand::random())) as Box<dyn Source>
        }),
        client_factory: cf, collector: collector.clone(),
        client_idx: 0, total_clients: 1, client_mode: false, clear,
        discard_output: output.is_none(), versioned: false, locking: false,
        auto_term_dur: if autoterm { Some(autoterm_dur) } else { None },
        auto_term_scale: autoterm_pct, rps_limit, host_select,
        no_prefix, custom_prefix: prefix, obj_size, objects: 0, versions: 1,
        bench_data: output, analyze_only: false, hosts, host_inflight, rps_limiter: crate::bench::rate_limiter::opt_rps_limiter(rps_limit), sse, checksum: None,
    };

    let bm = FanoutBenchmark::new(common, copies, obj_bytes);
    run_benchmark(Arc::new(bm)).await
}

/// Run AppendObject workload.
pub async fn run_append(
    config: S3Config,
    bucket: String,
    region: String,
    concurrency: usize,
    duration: Duration,
    obj_size: ObjSize,
    clear: bool,
    output: Option<String>,
    host_select: HostSelect,
    hosts: Vec<String>,
    no_prefix: bool,
    prefix: Option<String>,
    sse: SseConfig,
    rps_limit: Option<f64>,
) -> anyhow::Result<()> {
    use crate::bench::append::AppendBenchmark;
    let collector = collector_with_optional_influx()?;
    let cf = crate::bench::s3_client::client_factory(config.clone(), hosts.clone());
    let host_inflight = new_host_inflight(&hosts);
    let obj_bytes = match &obj_size {
        ObjSize::Fixed(s) => *s as usize,
        _ => 10 * 1024 * 1024, // default 10MiB
    };

    let common = Common {
        concurrency, duration, bucket: bucket.clone(), location: region,
        source: Arc::new({
            let p = prefix.clone().unwrap_or_else(|| "s3perf-ap".into());
            let os = obj_size.clone();
            move || Box::new(DefaultSource::new(p.clone(), os.clone(), rand::random())) as Box<dyn Source>
        }),
        client_factory: cf, collector: collector.clone(),
        client_idx: 0, total_clients: 1, client_mode: false, clear,
        discard_output: output.is_none(), versioned: false, locking: false,
        auto_term_dur: None, auto_term_scale: 0.075, rps_limit, host_select,
        no_prefix, custom_prefix: prefix, obj_size, objects: 0, versions: 1,
        bench_data: output, analyze_only: false, hosts, host_inflight, rps_limiter: crate::bench::rate_limiter::opt_rps_limiter(rps_limit), sse, checksum: None,
    };

    let bm = AppendBenchmark::new(common, Some(obj_bytes as i64));
    run_benchmark(Arc::new(bm)).await
}

/// Run in-memory ZIP upload workload.
pub async fn run_zip(
    config: S3Config,
    bucket: String,
    region: String,
    concurrency: usize,
    duration: Duration,
    obj_size: ObjSize,
    entries: usize,
    clear: bool,
    autoterm: bool,
    autoterm_dur: Duration,
    autoterm_pct: f64,
    output: Option<String>,
    host_select: HostSelect,
    hosts: Vec<String>,
    no_prefix: bool,
    prefix: Option<String>,
    sse: SseConfig,
    rps_limit: Option<f64>,
) -> anyhow::Result<()> {
    use crate::bench::zip::ZipBenchmark;
    let collector = collector_with_optional_influx()?;
    let cf = crate::bench::s3_client::client_factory(config.clone(), hosts.clone());
    let host_inflight = new_host_inflight(&hosts);

    let common = Common {
        concurrency,
        duration,
        bucket: bucket.clone(),
        location: region,
        source: Arc::new({
            let p = prefix.clone().unwrap_or_else(|| "s3perf-zip".into());
            let os = obj_size.clone();
            move || Box::new(DefaultSource::new(p.clone(), os.clone(), rand::random())) as Box<dyn Source>
        }),
        client_factory: cf,
        collector: collector.clone(),
        client_idx: 0,
        total_clients: 1,
        client_mode: false,
        clear,
        discard_output: output.is_none(),
        versioned: false,
        locking: false,
        auto_term_dur: if autoterm { Some(autoterm_dur) } else { None },
        auto_term_scale: autoterm_pct,
        rps_limit,
        host_select,
        no_prefix,
        custom_prefix: prefix,
        obj_size,
        objects: 0,
        versions: 1,
        bench_data: output,
        analyze_only: false,
        hosts,
        host_inflight,
        rps_limiter: crate::bench::rate_limiter::opt_rps_limiter(rps_limit),
        sse,
        checksum: None,
    };

    let bm = ZipBenchmark::new(common, entries);
    run_benchmark(Arc::new(bm)).await
}

/// Run a benchmark from YAML (`s3perf run <file>`).
pub async fn execute_run_yaml(cfg: crate::config::RunFileConfig) -> anyhow::Result<()> {
    use crate::cli::app::{parse_duration, parse_obj_size, parse_size};
    cfg.validate()
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let r = &cfg.s3perf.remote;
    let p = &cfg.s3perf.params;
    let s3_config = crate::bench::s3_client::S3Config {
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
            ObjSize::Bucketed { buckets, .. } => buckets.first().map(|(s, _)| *s).unwrap_or(1 << 20),
        };
        obj_size = ObjSize::Random { max };
    }
    let host_select: HostSelect = "weighed"
        .parse()
        .map_err(|e: String| anyhow::anyhow!(e))?;
    let hosts: Vec<String> = vec![r.host.clone()];
    let bucket = r.bucket.clone();
    let region = r.region.clone();
    let sse = SseConfig::None;
    let autoterm_dur = parse_duration(&p.autoterm.dur).map_err(|e| anyhow::anyhow!(e))?;
    let dist = p.distribution.as_ref();
    let (g, st, pu, del) = match dist {
        Some(d) => (d.get / 100.0, d.stat / 100.0, d.put / 100.0, d.delete / 100.0),
        None => (0.45, 0.05, 0.25, 0.25),
    };
    let out = cfg.s3perf.bench_data.clone();
    let ps = parse_size("5MiB").map_err(|e| anyhow::anyhow!(e))? as usize;

    match cfg.s3perf.benchmark.as_str() {
        "mixed" => {
            run_mixed(
                s3_config,
                bucket,
                region,
                p.concurrent,
                duration,
                obj_size,
                p.objects,
                g,
                st,
                pu,
                del,
                !p.no_clear,
                p.autoterm.enabled,
                autoterm_dur,
                p.autoterm.pct / 100.0,
                out,
                host_select,
                hosts,
                false,
                None,
                sse,
                None,
            )
            .await
        }
        "get" => {
            run_get(
                s3_config,
                bucket,
                region,
                p.concurrent,
                duration,
                obj_size,
                p.objects,
                p.versions.max(1),
                None,
                false,
                !p.no_clear,
                p.autoterm.enabled,
                autoterm_dur,
                p.autoterm.pct / 100.0,
                out,
                host_select,
                hosts,
                false,
                None,
                sse,
                None,
            )
            .await
        }
        "put" => {
            run_put(
                s3_config,
                bucket,
                region,
                p.concurrent,
                duration,
                obj_size,
                false,
                None,
                false,
                !p.no_clear,
                p.autoterm.enabled,
                autoterm_dur,
                p.autoterm.pct / 100.0,
                out,
                host_select,
                hosts,
                false,
                None,
                sse,
                None,
            )
            .await
        }
        "delete" => {
            run_delete(
                s3_config,
                bucket,
                region,
                p.concurrent,
                duration,
                obj_size,
                p.objects,
                100,
                !p.no_clear,
                p.autoterm.enabled,
                autoterm_dur,
                p.autoterm.pct / 100.0,
                out,
                host_select,
                hosts,
                false,
                None,
                sse,
                None,
            )
            .await
        }
        "list" => {
            run_list(
                s3_config,
                bucket,
                region,
                p.concurrent,
                duration,
                obj_size,
                p.objects,
                p.versions > 1,
                !p.no_clear,
                out,
                host_select,
                hosts,
                false,
                None,
                sse,
                None,
            )
            .await
        }
        "stat" => {
            run_stat(
                s3_config,
                bucket,
                region,
                p.concurrent,
                duration,
                obj_size,
                p.objects,
                !p.no_clear,
                out,
                host_select,
                hosts,
                false,
                None,
                sse,
                None,
            )
            .await
        }
        "versioned" => {
            run_versioned(
                s3_config,
                bucket,
                region,
                p.concurrent,
                duration,
                obj_size,
                p.objects,
                g,
                st,
                pu,
                del,
                !p.no_clear,
                p.autoterm.enabled,
                autoterm_dur,
                p.autoterm.pct / 100.0,
                out,
                host_select,
                hosts,
                false,
                None,
                sse,
                None,
            )
            .await
        }
        "retention" => {
            run_retention(
                s3_config,
                bucket,
                region,
                p.concurrent,
                duration,
                obj_size,
                p.objects,
                if p.versions > 0 { p.versions } else { 5 },
                !p.no_clear,
                p.autoterm.enabled,
                autoterm_dur,
                p.autoterm.pct / 100.0,
                out,
                host_select,
                hosts,
                false,
                None,
                sse,
                None,
            )
            .await
        }
        "multipart" => {
            run_multipart(
                s3_config,
                bucket,
                region,
                p.concurrent,
                duration,
                ps,
                200,
                "s3perf-multipart.bin".into(),
                !p.no_clear,
                p.autoterm.enabled,
                autoterm_dur,
                p.autoterm.pct / 100.0,
                out,
                host_select,
                hosts,
                false,
                None,
                sse,
                None,
            )
            .await
        }
        "multipart-put" => {
            run_multipart_put(
                s3_config,
                bucket,
                region,
                p.concurrent,
                duration,
                8,
                ps,
                4,
                !p.no_clear,
                p.autoterm.enabled,
                autoterm_dur,
                p.autoterm.pct / 100.0,
                out,
                host_select,
                hosts,
                false,
                None,
                sse,
                None,
            )
            .await
        }
        "snowball" => {
            run_snowball(
                s3_config,
                bucket,
                region,
                p.concurrent,
                duration,
                obj_size,
                4,
                !p.no_clear,
                p.autoterm.enabled,
                autoterm_dur,
                p.autoterm.pct / 100.0,
                out,
                host_select,
                hosts,
                false,
                None,
                sse,
                None,
            )
            .await
        }
        "fanout" => {
            run_fanout(
                s3_config,
                bucket,
                region,
                p.concurrent,
                duration,
                obj_size,
                4,
                !p.no_clear,
                p.autoterm.enabled,
                autoterm_dur,
                p.autoterm.pct / 100.0,
                out,
                host_select,
                hosts,
                false,
                None,
                sse,
                None,
            )
            .await
        }
        "append" => {
            run_append(
                s3_config,
                bucket,
                region,
                p.concurrent,
                duration,
                obj_size,
                !p.no_clear,
                out,
                host_select,
                hosts,
                false,
                None,
                sse,
                None,
            )
            .await
        }
        "zip" => {
            run_zip(
                s3_config,
                bucket,
                region,
                p.concurrent,
                duration,
                obj_size,
                8,
                !p.no_clear,
                p.autoterm.enabled,
                autoterm_dur,
                p.autoterm.pct / 100.0,
                out,
                host_select,
                hosts,
                false,
                None,
                sse,
                None,
            )
            .await
        }
        other => anyhow::bail!("unsupported benchmark type: {other}"),
    }
}

/// Shared benchmark orchestration (prepare/start/analyze/cleanup).
pub async fn run_benchmark(bm: Arc<dyn Benchmark>) -> anyhow::Result<()> {
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
    println!("Prepare: creating bucket and seeding objects...");
    monitor.set_status("prepare");
    let ctx = CancellationToken::new();
    bm.prepare(&ctx).await?;
    if let Some((ref st, _)) = tui_state {
        st.set_phase("Benchmark", "Benchmarking");
        st.set_progress(0.33);
    }

    // Phase 2: Start
    println!("Benchmark: running for {}s...", dur.as_secs());
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
        common.collector.auto_term(
            ctx,
            "",
            common.auto_term_scale,
            100,
            25,
            autoterm_dur,
        )
    } else {
        ctx
    };

    let bm_clone = Arc::clone(&bm);
    let bench_ctx = ctx.clone();
    let mut bench_handle = tokio::spawn(async move {
        bm_clone.start(&bench_ctx, wait_rx).await
    });

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
    println!("Analyzing results...");
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
    println!();
    println!("==========================================");
    println!("  s3perf benchmark results");
    println!("==========================================");
    println!("  operations: {}", ops.len());
    println!("  successful: {}", ok_ops.len());
    println!("  failed: {}", err_ops.len());
    if let Some(th) = &agg.mixed_server_stats {
        println!("  throughput: {:.2} MiB/s, {:.2} obj/s", th.avg_mbps, th.avg_ops);
        println!("  wall time: {:.1}s", th.duration_secs);
    }
    println!();
    for op_analysis in &agg.operations {
        println!("  [{}]", op_analysis.op_type);
        println!(
            "    throughput: {:.2} MiB/s, {:.2} obj/s",
            op_analysis.throughput.avg_mbps,
            op_analysis.throughput.avg_ops,
        );
        if let Some(ss) = &op_analysis.single_sized {
            println!(
                "    latency (ms): avg={:.1} median={:.1} P90={:.1} P99={:.1}",
                ss.avg_duration_ms, ss.median_duration_ms, ss.p90_duration_ms, ss.p99_duration_ms,
            );
        }
        println!("    errors: {}", op_analysis.errors);
        for e in &op_analysis.first_errors {
            println!("    error detail: {e}");
        }
        println!();
    }
    println!("==========================================");

    // Phase 4: Cleanup
    if common.clear {
        println!("Cleanup: removing benchmark objects...");
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
            println!("Saving data to: {path}");
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

    Ok(())
}

/// Load and pretty-print aggregated results from `.csv.zst` / `.json.zst`.
pub fn analyze_file(path: &str) -> anyhow::Result<()> {
    println!("Analyzing benchmark file: {path}");
    let file = std::fs::File::open(path)?;
    let mut reader = std::io::BufReader::new(file);

    let ops = if path.ends_with(".csv.zst") {
        crate::aggregate::read_csv_zst(&mut reader)?
    } else if path.ends_with(".json.zst") {
        let agg = crate::aggregate::read_json_zst(&mut reader)?;
        println!("{agg:#?}");
        return Ok(());
    } else {
        anyhow::bail!("unsupported file format: {path} (expected .csv.zst or .json.zst)");
    };

    let agg = crate::aggregate::analyze(&ops, Duration::from_secs(1), 20);
    println!("{agg:#?}");
    Ok(())
}

/// Compare aggregates from two prior runs.
pub fn compare_files(before: &str, after: &str) -> anyhow::Result<()> {
    println!("Comparing benchmarks: {before} vs {after}");

    let read_agg = |path: &str| -> anyhow::Result<crate::aggregate::Aggregated> {
        let file = std::fs::File::open(path)?;
        let mut reader = std::io::BufReader::new(file);
        if path.ends_with(".json.zst") {
            Ok(crate::aggregate::read_json_zst(&mut reader)?)
        } else if path.ends_with(".csv.zst") {
            let ops = crate::aggregate::read_csv_zst(&mut reader)?;
            Ok(crate::aggregate::analyze(&ops, Duration::from_secs(1), 20))
        } else {
            anyhow::bail!("unsupported file format: {path}")
        }
    };

    let before_agg = read_agg(before)?;
    let after_agg = read_agg(after)?;
    let result = crate::aggregate::compare(&before_agg, &after_agg);

    println!("Comparison:");
    for diff in &result.diffs {
        println!(
            "  {}: {:.2} → {:.2} MiB/s ({:+.1}%)  |  {:.2} → {:.2} obj/s ({:+.1}%)",
            diff.op_type,
            diff.before_mbps,
            diff.after_mbps,
            diff.mbps_diff_pct,
            diff.before_ops,
            diff.after_ops,
            diff.ops_diff_pct,
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Iceberg benchmark entrypoints
// ---------------------------------------------------------------------------

/// Placeholder `catalog-write` entrypoint (use `catalog-commits` or `sustained` for real IO).
pub async fn run_iceberg_catalog_write_stub(page_size: usize) -> anyhow::Result<()> {
    tracing::info!(
        "iceberg catalog-write (page_size={page_size}) is stubbed; run `iceberg catalog-commits` or `iceberg sustained` instead."
    );
    Ok(())
}

use crate::iceberg::tree::TreeConfig;
use crate::iceberg::{CatalogConfig, ExternalCatalogType, RetryConfig};
use crate::bench::iceberg_read::IcebergReadBenchmark;
use crate::bench::iceberg_commits::IcebergCommitsBenchmark;
use crate::bench::iceberg_mixed::IcebergMixedBenchmark;
use crate::bench::iceberg_sustained::IcebergSustainedBenchmark;

fn build_iceberg_common(
    _config: S3Config,
    region: String,
    concurrency: usize,
    duration: Duration,
    clear: bool,
    autoterm: bool,
    autoterm_dur: Duration,
    autoterm_pct: f64,
    output: Option<String>,
    host_select: HostSelect,
    hosts: Vec<String>,
    remote_hosts: Option<String>,
) -> Common {
    let collector = collector_with_optional_influx()
        .expect("failed to initialize OpsCollector / Influx writer");
    let host_inflight = new_host_inflight(&hosts);
    Common {
        concurrency, duration,
        bucket: "iceberg-bench".into(),
        location: region,
        source: Arc::new(|| Box::new(DefaultSource::new("s3perf-iceberg".into(), ObjSize::Fixed(1024), rand::random()))),
        client_factory: Arc::new(|_: usize| panic!("Iceberg benchmarks don't use S3 client directly")),
        collector, client_idx: 0, total_clients: 1,
        client_mode: remote_hosts.is_some(),
        clear, discard_output: output.is_none(),
        versioned: false, locking: false,
        auto_term_dur: if autoterm { Some(autoterm_dur) } else { None },
        auto_term_scale: autoterm_pct,
        rps_limit: None, host_select, no_prefix: false,
        custom_prefix: None,
        obj_size: ObjSize::Fixed(1024),
        objects: 0, versions: 1, bench_data: output,
        analyze_only: false, hosts, host_inflight, rps_limiter: crate::bench::rate_limiter::opt_rps_limiter(None), sse: SseConfig::None, checksum: None,
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn run_iceberg_read(
    config: S3Config, region: String, concurrency: usize, duration: Duration,
    _ns_list_distrib: u32, _ns_head_distrib: u32, _ns_get_distrib: u32,
    _table_list_distrib: u32, _table_head_distrib: u32, _table_get_distrib: u32,
    _view_list_distrib: u32, _view_head_distrib: u32, _view_get_distrib: u32,
    page_size: usize, namespace_width: usize, namespace_depth: usize,
    tables_per_ns: usize, views_per_ns: usize,
    columns: usize, properties: usize, base_location: String,
    external_catalog: Option<String>, catalog_name: String,
    clear: bool, autoterm: bool, autoterm_dur: Duration, autoterm_pct: f64,
    output: Option<String>, host_select: HostSelect, hosts: Vec<String>,
    remote_hosts: Option<String>,
) -> anyhow::Result<()> {
    let common = build_iceberg_common(config, region, concurrency, duration,
        clear, autoterm, autoterm_dur, autoterm_pct, output, host_select, hosts, remote_hosts);

    let tree_config = TreeConfig {
        namespace_width, namespace_depth, tables_per_ns, views_per_ns,
        columns, properties, base_location, catalog_name,
    };

    let ext = external_catalog.map(|s| ExternalCatalogType::from_str(&s)).unwrap_or_default();

    let catalog_config = CatalogConfig {
        catalog_uri: common.hosts.first().cloned().unwrap_or_else(|| "http://localhost:9001".into()),
        warehouse: tree_config.catalog_name.clone(),
        access_key: "minioadmin".into(), secret_key: "minioadmin".into(),
        region: common.location.clone(), tls: false, external_catalog: ext,
    };

    let bm = IcebergReadBenchmark {
        common,
        catalog_config,
        catalog: None, catalog_pool: None,
        tree_config,
        dist: None,
        page_size,
        external_catalog: ExternalCatalogType::None,
        tree: std::sync::Mutex::new(None),
        namespaces: std::sync::Mutex::new(Vec::new()),
        tables: std::sync::Mutex::new(Vec::new()),
        views: std::sync::Mutex::new(Vec::new()),
        ops: std::sync::Mutex::new(Vec::new()),
    };

    run_benchmark(Arc::new(bm)).await
}

#[allow(clippy::too_many_arguments)]
pub async fn run_iceberg_commits(
    config: S3Config, region: String, concurrency: usize, duration: Duration,
    table_commits_throughput: usize, view_commits_throughput: usize,
    max_retries: usize, retry_backoff_ms: u64, backoff_max_ms: u64,
    namespace_width: usize, namespace_depth: usize,
    tables_per_ns: usize, views_per_ns: usize,
    columns: usize, properties: usize, base_location: String,
    external_catalog: Option<String>, catalog_name: String,
    clear: bool, autoterm: bool, autoterm_dur: Duration, autoterm_pct: f64,
    output: Option<String>, host_select: HostSelect, hosts: Vec<String>,
    remote_hosts: Option<String>,
) -> anyhow::Result<()> {
    let common = build_iceberg_common(config, region, concurrency, duration,
        clear, autoterm, autoterm_dur, autoterm_pct, output, host_select, hosts, remote_hosts);

    let tree_config = TreeConfig {
        namespace_width, namespace_depth, tables_per_ns, views_per_ns,
        columns, properties, base_location, catalog_name,
    };
    let ext = external_catalog.map(|s| ExternalCatalogType::from_str(&s)).unwrap_or_default();

    let catalog_config = CatalogConfig {
        catalog_uri: common.hosts.first().cloned().unwrap_or_else(|| "http://localhost:9001".into()),
        warehouse: tree_config.catalog_name.clone(),
        access_key: "minioadmin".into(), secret_key: "minioadmin".into(),
        region: common.location.clone(), tls: false, external_catalog: ext,
    };

    let bm = IcebergCommitsBenchmark {
        common, catalog_config,
        catalog: None, catalog_pool: None,
        tree_config, external_catalog: ExternalCatalogType::None,
        table_workers: table_commits_throughput,
        view_workers: view_commits_throughput,
        retry_config: RetryConfig {
            max_retries, base_backoff: Duration::from_millis(retry_backoff_ms),
            max_backoff: Duration::from_millis(backoff_max_ms),
        },
        tables: std::sync::Mutex::new(Vec::new()),
        views: std::sync::Mutex::new(Vec::new()),
        tree: std::sync::Mutex::new(None),
        ops: std::sync::Mutex::new(Vec::new()),
    };

    run_benchmark(Arc::new(bm)).await
}

#[allow(clippy::too_many_arguments)]
pub async fn run_iceberg_mixed(
    config: S3Config, region: String, concurrency: usize, duration: Duration,
    _ns_list_distrib: u32, _ns_head_distrib: u32, _ns_get_distrib: u32,
    _table_list_distrib: u32, _table_head_distrib: u32, _table_get_distrib: u32,
    _view_list_distrib: u32, _view_head_distrib: u32, _view_get_distrib: u32,
    _ns_update_distrib: u32, _table_update_distrib: u32, _view_update_distrib: u32,
    max_retries: usize, retry_backoff_ms: u64, backoff_max_ms: u64,
    page_size: usize, namespace_width: usize, namespace_depth: usize,
    tables_per_ns: usize, views_per_ns: usize,
    columns: usize, properties: usize, base_location: String,
    external_catalog: Option<String>, catalog_name: String,
    clear: bool, autoterm: bool, autoterm_dur: Duration, autoterm_pct: f64,
    output: Option<String>, host_select: HostSelect, hosts: Vec<String>,
    remote_hosts: Option<String>,
) -> anyhow::Result<()> {
    let common = build_iceberg_common(config, region, concurrency, duration,
        clear, autoterm, autoterm_dur, autoterm_pct, output, host_select, hosts, remote_hosts);

    let tree_config = TreeConfig {
        namespace_width, namespace_depth, tables_per_ns, views_per_ns,
        columns, properties, base_location, catalog_name,
    };
    let ext = external_catalog.map(|s| ExternalCatalogType::from_str(&s)).unwrap_or_default();

    let catalog_config = CatalogConfig {
        catalog_uri: common.hosts.first().cloned().unwrap_or_else(|| "http://localhost:9001".into()),
        warehouse: tree_config.catalog_name.clone(),
        access_key: "minioadmin".into(), secret_key: "minioadmin".into(),
        region: common.location.clone(), tls: false, external_catalog: ext,
    };

    let bm = IcebergMixedBenchmark {
        common, catalog_config,
        catalog: None, catalog_pool: None,
        tree_config, external_catalog: ExternalCatalogType::None,
        dist: None, page_size,
        retry_config: RetryConfig {
            max_retries, base_backoff: Duration::from_millis(retry_backoff_ms),
            max_backoff: Duration::from_millis(backoff_max_ms),
        },
        namespaces: std::sync::Mutex::new(Vec::new()),
        tables: std::sync::Mutex::new(Vec::new()),
        views: std::sync::Mutex::new(Vec::new()),
        tree: std::sync::Mutex::new(None),
        ns_update_id: std::sync::atomic::AtomicU64::new(0),
        table_update_id: std::sync::atomic::AtomicU64::new(0),
        view_update_id: std::sync::atomic::AtomicU64::new(0),
        ops: std::sync::Mutex::new(Vec::new()),
    };

    run_benchmark(Arc::new(bm)).await
}

#[allow(clippy::too_many_arguments)]
pub async fn run_iceberg_sustained(
    config: S3Config, region: String, concurrency: usize, duration: Duration,
    num_files: usize, rows_per_file: usize, files_per_commit: usize,
    tpcds: bool, scale_factor: f64, tpcds_table: Option<String>,
    cache_dir: String, skip_upload: bool, simulate_read: bool,
    read_concurrent: usize, read_rps_limit: f64,
    max_retries: usize, retry_backoff_ms: u64, backoff_max_ms: u64,
    _s3_host: Option<String>, _s3_access_key: Option<String>,
    _s3_secret_key: Option<String>, _s3_tls: bool,
    namespace_width: usize, namespace_depth: usize,
    tables_per_ns: usize, columns: usize, properties: usize,
    base_location: String, external_catalog: Option<String>,
    catalog_name: String,
    clear: bool, autoterm: bool, autoterm_dur: Duration, autoterm_pct: f64,
    output: Option<String>, host_select: HostSelect, hosts: Vec<String>,
    remote_hosts: Option<String>,
) -> anyhow::Result<()> {
    let common = build_iceberg_common(config, region, concurrency, duration,
        clear, autoterm, autoterm_dur, autoterm_pct, output, host_select, hosts, remote_hosts);

    let tree_config = TreeConfig {
        namespace_width, namespace_depth, tables_per_ns,
        views_per_ns: 0, columns, properties, base_location, catalog_name,
    };
    let ext = external_catalog.map(|s| ExternalCatalogType::from_str(&s)).unwrap_or_default();

    let catalog_config = CatalogConfig {
        catalog_uri: common.hosts.first().cloned().unwrap_or_else(|| "http://localhost:9001".into()),
        warehouse: tree_config.catalog_name.clone(),
        access_key: "minioadmin".into(), secret_key: "minioadmin".into(),
        region: common.location.clone(), tls: false, external_catalog: ext,
    };

    let bm = IcebergSustainedBenchmark {
        common, catalog_config,
        catalog: None, catalog_pool: None,
        tree_config, external_catalog: ExternalCatalogType::None,
        num_files, rows_per_file, files_per_commit,
        tpcds, scale_factor, tpcds_table,
        cache_dir, skip_upload, simulate_read,
        read_concurrent, read_rps_limit,
        retry_config: RetryConfig {
            max_retries, base_backoff: Duration::from_millis(retry_backoff_ms),
            max_backoff: Duration::from_millis(backoff_max_ms),
        },
        tables: std::sync::Mutex::new(Vec::new()),
        tree: std::sync::Mutex::new(None),
        ops: std::sync::Mutex::new(Vec::new()),
    };

    run_benchmark(Arc::new(bm)).await
}

/// Merge multiple compressed CSV benchmarks.
pub fn merge_files(files: &[String]) -> anyhow::Result<()> {
    println!("Merge: combining {} CSV.zst datasets...", files.len());
    let mut op_sets = Vec::new();

    for path in files {
        let file = std::fs::File::open(path)?;
        let mut reader = std::io::BufReader::new(file);
        let ops = if path.ends_with(".csv.zst") {
            crate::aggregate::read_csv_zst(&mut reader)?
        } else {
            anyhow::bail!("merge only supports .csv.zst files: {path}");
        };
        op_sets.push(ops);
    }

    let merged = crate::aggregate::merge(&op_sets);
    println!("Merged operation count: {}", merged.len());

    let agg = crate::aggregate::analyze(&merged, Duration::from_secs(1), 20);
    println!("{agg:#?}");
    Ok(())
}
