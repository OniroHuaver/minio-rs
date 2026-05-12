//! CLI wiring: parsing, benchmarks, orchestration.

pub mod app;
pub mod analysis;
pub mod iceberg;
pub mod runner;

use crate::bench::collector::OpsCollector;
use crate::bench::s3_client::S3Config;
use crate::bench::sse::SseConfig;
use crate::bench::{Common, HostSelect};
use crate::generator::{DefaultSource, ObjSize, Source};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

// Re-export public API from sub-modules
pub use analysis::{analyze_file, compare_files, merge_files};
pub use iceberg::{
    run_iceberg_catalog_write_stub, run_iceberg_commits, run_iceberg_mixed, run_iceberg_read,
    run_iceberg_sustained,
};
pub use runner::{
    execute_run_yaml, run_append, run_benchmark, run_delete, run_fanout, run_get, run_list,
    run_mixed, run_multipart, run_multipart_put, run_put, run_retention, run_snowball, run_stat,
    run_versioned, run_zip,
};

/// Influx URL from the outer `Cli` (`main` installs it once per process).
static INFLUX_URL: OnceLock<Option<String>> = OnceLock::new();
/// Background Influx fan-out task handle for this run.
static INFLUX_JOIN: Mutex<Option<tokio::task::JoinHandle<()>>> = Mutex::new(None);

pub fn set_influx_url(url: Option<String>) {
    let _ = INFLUX_URL.set(url);
}

// ---------------------------------------------------------------------------
// BenchConfig — shared benchmark parameters bundled from CLI / YAML
// ---------------------------------------------------------------------------
/// All parameters that are shared across every benchmark command.
/// Constructed once in `main` (or `execute_run_yaml`) and passed by reference.
pub struct BenchConfig {
    pub s3_config: S3Config,
    pub bucket: String,
    pub region: String,
    pub concurrency: usize,
    pub duration: Duration,
    pub obj_size: ObjSize,
    pub objects: usize,
    pub clear: bool,
    pub autoterm: bool,
    pub autoterm_dur: Duration,
    pub autoterm_pct: f64,
    pub output: Option<String>,
    pub host_select: HostSelect,
    pub hosts: Vec<String>,
    pub no_prefix: bool,
    pub prefix: Option<String>,
    pub sse: SseConfig,
    pub rps_limit: Option<f64>,
}

impl BenchConfig {
    /// Build a `Common` with the shared fields filled in.
    /// Callers override the few fields that differ per command.
    pub fn build_common(
        &self,
        collector: Arc<OpsCollector>,
        source_prefix: &str,
    ) -> Common {
        let cf = crate::bench::s3_client::client_factory(self.s3_config.clone(), self.hosts.clone());
        let host_inflight = new_host_inflight(&self.hosts);
        Common {
            concurrency: self.concurrency,
            duration: self.duration,
            bucket: self.bucket.clone(),
            location: self.region.clone(),
            source: Arc::new({
                let p = self.prefix.clone().unwrap_or_else(|| source_prefix.into());
                let os = self.obj_size.clone();
                move || Box::new(DefaultSource::new(p.clone(), os.clone(), rand::random())) as Box<dyn Source>
            }),
            client_factory: cf,
            collector,
            client_idx: 0,
            total_clients: 1,
            client_mode: false,
            clear: self.clear,
            discard_output: self.output.is_none(),
            versioned: false,
            locking: false,
            auto_term_dur: if self.autoterm { Some(self.autoterm_dur) } else { None },
            auto_term_scale: self.autoterm_pct,
            rps_limit: self.rps_limit,
            host_select: self.host_select,
            no_prefix: self.no_prefix,
            custom_prefix: self.prefix.clone(),
            obj_size: self.obj_size.clone(),
            objects: self.objects,
            versions: 1,
            bench_data: self.output.clone(),
            analyze_only: false,
            hosts: self.hosts.clone(),
            host_inflight,
            rps_limiter: crate::bench::rate_limiter::opt_rps_limiter(self.rps_limit),
            sse: self.sse.clone(),
            checksum: None,
        }
    }
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
    *INFLUX_JOIN.lock().expect("lock poisoned") = None;
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
    *INFLUX_JOIN.lock().expect("lock poisoned") = Some(h);
    Ok(coll)
}

fn take_influx_join() -> Option<tokio::task::JoinHandle<()>> {
    INFLUX_JOIN.lock().expect("lock poisoned").take()
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
