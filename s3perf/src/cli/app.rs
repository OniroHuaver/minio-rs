//! Clap definitions: global flags and subcommands.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// High-throughput S3-compatible object storage benchmark tool.
#[derive(Parser, Debug)]
#[command(name = "s3perf", version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// S3 endpoint (`host:port`).
    #[arg(long, env = "S3PERF_HOST", default_value = "localhost:9000")]
    pub host: String,

    /// Access key id.
    #[arg(long, env = "S3PERF_ACCESS_KEY", default_value = "minioadmin")]
    pub access_key: String,

    /// Secret access key.
    #[arg(long, env = "S3PERF_SECRET_KEY", default_value = "minioadmin")]
    pub secret_key: String,

    /// Use TLS when talking to S3.
    #[arg(long, env = "S3PERF_TLS", default_value_t = false)]
    pub tls: bool,

    /// Skip TLS certificate verification.
    #[arg(long, default_value_t = false)]
    pub insecure: bool,

    /// Region string passed to SigV4.
    #[arg(long, env = "S3PERF_REGION", default_value = "us-east-1")]
    pub region: String,

    /// Host selection: `weighed` or `roundrobin`.
    #[arg(long, default_value = "weighed")]
    pub host_select: String,

    /// Benchmark bucket.
    #[arg(long, default_value = crate::config::DEFAULT_S3PERF_BUCKET)]
    pub bucket: String,

    /// Concurrent workers.
    #[arg(long, default_value = "20")]
    pub concurrent: usize,

    /// Object size (`10MiB`, `1GiB`, or bucket spec `4096:10740,8192:1685,...`).
    #[arg(long, default_value = "1MiB")]
    pub obj_size: String,

    /// Random object sizes (log2 distribution up to max).
    #[arg(long)]
    pub obj_randsize: bool,

    /// Benchmark duration.
    #[arg(long, default_value = "5m")]
    pub duration: String,

    /// Object count for prepare phase.
    #[arg(long, default_value = "10000")]
    pub objects: usize,

    /// Do not delete test objects or bucket after the run.
    #[arg(long, default_value_t = false)]
    pub noclear: bool,

    /// Output path for benchmark data.
    #[arg(long)]
    pub benchdata: Option<PathBuf>,

    /// Enable auto-termination when throughput stabilizes.
    #[arg(long, default_value_t = false)]
    pub autoterm: bool,

    /// Minimum steady duration before auto-termination can trigger.
    #[arg(long, default_value = "15s")]
    pub autoterm_dur: String,

    /// Auto-termination variability threshold (percent).
    #[arg(long, default_value = "7.5")]
    pub autoterm_pct: f64,

    /// Disable multi-prefix object names.
    #[arg(long, default_value_t = false)]
    pub noprefix: bool,

    /// Custom object name prefix.
    #[arg(long)]
    pub prefix: Option<String>,

    /// Extra S3 endpoints (comma-separated).
    #[arg(long)]
    pub hosts: Option<String>,

    /// Max requests per second per worker (RPS cap).
    #[arg(long)]
    pub rps_limit: Option<f64>,

    /// Distributed mode: comma-separated remote agent addresses (`host:port`).
    #[arg(long)]
    pub remote_hosts: Option<String>,

    /// InfluxDB connection URL (if set, metrics are written during the run).
    #[arg(long)]
    pub influxdb: Option<String>,

    /// Enable SSE-C (customer-managed keys).
    #[arg(long, default_value_t = false)]
    pub encrypt: bool,

    /// Enable SSE-S3 (server-managed encryption).
    #[arg(long, default_value_t = false)]
    pub sse_s3_encrypt: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Mixed workload (GET / STAT / PUT / DELETE).
    Mixed {
        /// GET fraction (0.0–1.0).
        #[arg(long, default_value = "0.45")]
        get_distrib: f64,
        /// STAT fraction.
        #[arg(long, default_value = "0.05")]
        stat_distrib: f64,
        /// PUT fraction.
        #[arg(long, default_value = "0.25")]
        put_distrib: f64,
        /// DELETE fraction.
        #[arg(long, default_value = "0.25")]
        delete_distrib: f64,
        /// Version count for versioned keys.
        #[arg(long, default_value = "1")]
        versions: usize,
    },

    /// GET workload.
    Get {
        /// Number of object versions to exercise.
        #[arg(long, default_value = "1")]
        versions: usize,
        /// Partial read range `start-end` (bytes).
        #[arg(long)]
        range: Option<String>,
        /// List existing keys under prefix instead of uploading in prepare.
        #[arg(long, default_value_t = false)]
        list_existing: bool,
    },

    /// PUT workload.
    Put {
        /// Compute Content-MD5.
        #[arg(long, default_value_t = false)]
        md5: bool,
        /// Checksum algorithm (e.g. CRC32, SHA256) when supported.
        #[arg(long)]
        checksum: Option<String>,
        /// Use POST-style upload if applicable (may fall back to PUT).
        #[arg(long, default_value_t = false)]
        post: bool,
    },

    /// DELETE workload.
    Delete {
        /// Objects per delete request (batch size).
        #[arg(long, default_value = "100")]
        batch: usize,
    },

    /// LIST workload.
    List {
        /// List object versions.
        #[arg(long, default_value_t = false)]
        versions: bool,
    },

    /// STAT (HeadObject) workload.
    Stat {},

    /// Versioned-bucket mixed workload.
    Versioned {
        /// GET fraction (0.0–1.0).
        #[arg(long, default_value = "0.45")]
        get_distrib: f64,
        /// STAT fraction.
        #[arg(long, default_value = "0.30")]
        stat_distrib: f64,
        /// PUT fraction.
        #[arg(long, default_value = "0.15")]
        put_distrib: f64,
        /// DELETE fraction.
        #[arg(long, default_value = "0.10")]
        delete_distrib: f64,
    },

    /// Object Lock retention workload.
    Retention {},

    /// Multipart upload + ranged GET per part.
    Multipart {
        /// Size of each part (e.g. `5MiB`).
        #[arg(long, default_value = "5MiB")]
        part_size: String,
        /// Number of parts.
        #[arg(long, default_value = "200")]
        parts: usize,
        /// Object key name.
        #[arg(long, default_value = "s3perf-multipart.bin")]
        obj_name: String,
    },

    /// Concurrent multipart PUT workload.
    MultipartPut {
        /// Parts per multipart upload.
        #[arg(long, default_value = "100")]
        parts: usize,
        /// Size of each part (e.g. `5MiB`).
        #[arg(long, default_value = "5MiB")]
        part_size: String,
        /// Concurrent part uploads inside each multipart session.
        #[arg(long, default_value = "20")]
        part_concurrent: usize,
    },

    /// Snowball-style TAR upload workload.
    Snowball {
        /// Objects bundled per archive.
        #[arg(long, default_value = "50")]
        objs_per: usize,
    },

    /// Fan-out copy workload.
    Fanout {
        /// Copies per round.
        #[arg(long, default_value = "100")]
        copies: usize,
    },

    /// S3 Append workload.
    Append {},

    /// ZIP-in-memory then PutObject workload.
    Zip {
        /// Number of entries inside the ZIP.
        #[arg(long, default_value = "8")]
        entries: usize,
    },

    /// Run a benchmark from a YAML file (`s3perf run`).
    Run {
        /// Path to YAML config.
        config: PathBuf,
    },

    /// Run as remote agent (waits for coordinator benchmark config).
    Client {
        /// Listen address for the agent WebSocket.
        #[arg(default_value = "127.0.0.1:7761")]
        listen_addr: String,
    },

    /// Iceberg REST catalog benchmarks.
    #[command(subcommand)]
    Iceberg(IcebergCommand),

    /// Analyze a benchmark data file.
    Analyze {
        /// Input path (`.csv.zst` or `.json.zst`).
        file: PathBuf,
    },

    /// Compare two benchmark runs.
    Cmp {
        /// Baseline file.
        before: PathBuf,
        /// Comparison file.
        after: PathBuf,
    },

    /// Merge multiple benchmark data files.
    Merge {
        /// Input files.
        files: Vec<PathBuf>,
    },
}

/// Iceberg subcommands.
#[derive(Subcommand, Debug, Clone)]
pub enum IcebergCommand {
    /// Catalog read mix (LIST / HEAD / GET).
    CatalogRead {
        #[arg(long, default_value = "10")] ns_list_distrib: u32,
        #[arg(long, default_value = "10")] ns_head_distrib: u32,
        #[arg(long, default_value = "10")] ns_get_distrib: u32,
        #[arg(long, default_value = "10")] table_list_distrib: u32,
        #[arg(long, default_value = "10")] table_head_distrib: u32,
        #[arg(long, default_value = "10")] table_get_distrib: u32,
        #[arg(long, default_value = "10")] view_list_distrib: u32,
        #[arg(long, default_value = "10")] view_head_distrib: u32,
        #[arg(long, default_value = "10")] view_get_distrib: u32,
        #[arg(long, default_value = "10")] page_size: usize,
        #[arg(long, default_value = "2")] namespace_width: usize,
        #[arg(long, default_value = "3")] namespace_depth: usize,
        #[arg(long, default_value = "5")] tables_per_ns: usize,
        #[arg(long, default_value = "5")] views_per_ns: usize,
        #[arg(long, default_value = "10")] columns: usize,
        #[arg(long, default_value = "5")] properties: usize,
        #[arg(long, default_value = "s3://benchmark")] base_location: String,
        #[arg(long)] external_catalog: Option<String>,
        #[arg(long, default_value = "benchmarkcatalog")] catalog_name: String,
    },
    /// Catalog metadata commits (table / view).
    CatalogCommits {
        #[arg(long, default_value = "0")] table_commits_throughput: usize,
        #[arg(long, default_value = "0")] view_commits_throughput: usize,
        #[arg(long, default_value = "4")] max_retries: usize,
        #[arg(long, default_value = "100")] retry_backoff_ms: u64,
        #[arg(long, default_value = "60000")] backoff_max_ms: u64,
        #[arg(long, default_value = "2")] namespace_width: usize,
        #[arg(long, default_value = "3")] namespace_depth: usize,
        #[arg(long, default_value = "5")] tables_per_ns: usize,
        #[arg(long, default_value = "5")] views_per_ns: usize,
        #[arg(long, default_value = "10")] columns: usize,
        #[arg(long, default_value = "5")] properties: usize,
        #[arg(long, default_value = "s3://benchmark")] base_location: String,
        #[arg(long)] external_catalog: Option<String>,
        #[arg(long, default_value = "benchmarkcatalog")] catalog_name: String,
    },
    /// Mixed catalog read/write workload.
    CatalogMixed {
        #[arg(long, default_value = "10")] ns_list_distrib: u32,
        #[arg(long, default_value = "10")] ns_head_distrib: u32,
        #[arg(long, default_value = "10")] ns_get_distrib: u32,
        #[arg(long, default_value = "10")] table_list_distrib: u32,
        #[arg(long, default_value = "10")] table_head_distrib: u32,
        #[arg(long, default_value = "10")] table_get_distrib: u32,
        #[arg(long, default_value = "10")] view_list_distrib: u32,
        #[arg(long, default_value = "10")] view_head_distrib: u32,
        #[arg(long, default_value = "10")] view_get_distrib: u32,
        #[arg(long, default_value = "5")] ns_update_distrib: u32,
        #[arg(long, default_value = "5")] table_update_distrib: u32,
        #[arg(long, default_value = "5")] view_update_distrib: u32,
        #[arg(long, default_value = "5")] max_retries: usize,
        #[arg(long, default_value = "100")] retry_backoff_ms: u64,
        #[arg(long, default_value = "2000")] backoff_max_ms: u64,
        #[arg(long, default_value = "10")] page_size: usize,
        #[arg(long, default_value = "2")] namespace_width: usize,
        #[arg(long, default_value = "3")] namespace_depth: usize,
        #[arg(long, default_value = "5")] tables_per_ns: usize,
        #[arg(long, default_value = "5")] views_per_ns: usize,
        #[arg(long, default_value = "10")] columns: usize,
        #[arg(long, default_value = "5")] properties: usize,
        #[arg(long, default_value = "s3://benchmark")] base_location: String,
        #[arg(long)] external_catalog: Option<String>,
        #[arg(long, default_value = "benchmarkcatalog")] catalog_name: String,
    },
    /// Catalog write stub (use `catalog-commits` or `sustained` for data paths).
    CatalogWrite {
        #[arg(long, default_value = "10")] page_size: usize,
    },

    /// Sustained write / read simulation workload.
    Sustained {
        #[arg(long, default_value = "10")] num_files: usize,
        #[arg(long, default_value = "10000")] rows_per_file: usize,
        #[arg(long, default_value = "1")] files_per_commit: usize,
        #[arg(long, default_value_t = false)] tpcds: bool,
        #[arg(long, default_value = "0.01")] scale_factor: f64,
        #[arg(long)] tpcds_table: Option<String>,
        #[arg(long, default_value = "/tmp/s3perf-iceberg-cache")] cache_dir: String,
        #[arg(long, default_value_t = true)] skip_upload: bool,
        #[arg(long, default_value_t = false)] simulate_read: bool,
        #[arg(long, default_value = "20")] read_concurrent: usize,
        #[arg(long, default_value = "400")] read_rps_limit: f64,
        #[arg(long, default_value = "4")] max_retries: usize,
        #[arg(long, default_value = "100")] retry_backoff_ms: u64,
        #[arg(long, default_value = "60000")] backoff_max_ms: u64,
        #[arg(long)] s3_host: Option<String>,
        #[arg(long)] s3_access_key: Option<String>,
        #[arg(long)] s3_secret_key: Option<String>,
        #[arg(long, default_value_t = false)] s3_tls: bool,
        #[arg(long, default_value = "1")] namespace_width: usize,
        #[arg(long, default_value = "1")] namespace_depth: usize,
        #[arg(long, default_value = "1")] tables_per_ns: usize,
        #[arg(long, default_value = "10")] columns: usize,
        #[arg(long, default_value = "5")] properties: usize,
        #[arg(long, default_value = "s3://benchmark")] base_location: String,
        #[arg(long)] external_catalog: Option<String>,
        #[arg(long, default_value = "benchmarkcatalog")] catalog_name: String,
    },
}

/// Parse duration strings (`5m`, `30s`, `1h`).
pub fn parse_duration(s: &str) -> Result<std::time::Duration, String> {
    let s = s.trim();
    if let Some(s) = s.strip_suffix('h') {
        let h: f64 = s.parse().map_err(|e| format!("invalid duration: {e}"))?;
        Ok(std::time::Duration::from_secs_f64(h * 3600.0))
    } else if let Some(s) = s.strip_suffix('m') {
        let m: f64 = s.parse().map_err(|e| format!("invalid duration: {e}"))?;
        Ok(std::time::Duration::from_secs_f64(m * 60.0))
    } else if let Some(s) = s.strip_suffix('s') {
        let secs: f64 = s.parse().map_err(|e| format!("invalid duration: {e}"))?;
        Ok(std::time::Duration::from_secs_f64(secs))
    } else {
        let secs: f64 = s.parse().map_err(|e| format!("invalid duration: {e}"))?;
        Ok(std::time::Duration::from_secs_f64(secs))
    }
}

/// Parse object size strings (`10MiB`, `1GiB`, bare bytes).
pub fn parse_obj_size(s: &str) -> Result<crate::generator::ObjSize, String> {
    if s.starts_with("rand:") {
        return crate::generator::ObjSize::parse(s);
    }
    if s.contains(':') {
        return crate::generator::ObjSize::parse(s);
    }
    let size = parse_size(s)?;
    Ok(crate::generator::ObjSize::Fixed(size))
}

pub fn parse_size(s: &str) -> Result<i64, String> {
    let s = s.trim().to_lowercase();
    let (num_str, unit) = if let Some(s) = s.strip_suffix("gib") {
        (s.trim(), 1024i64.pow(3))
    } else if let Some(s) = s.strip_suffix("mib") {
        (s.trim(), 1024i64.pow(2))
    } else if let Some(s) = s.strip_suffix("kib") {
        (s.trim(), 1024)
    } else if let Some(s) = s.strip_suffix("g") {
        (s.trim(), 1000i64.pow(3))
    } else if let Some(s) = s.strip_suffix("m") {
        (s.trim(), 1000i64.pow(2))
    } else if let Some(s) = s.strip_suffix("k") {
        (s.trim(), 1000)
    } else if let Some(s) = s.strip_suffix("b") {
        (s.trim(), 1)
    } else {
        (s.as_str(), 1)
    };

    let num: f64 = num_str.parse().map_err(|e| format!("invalid size: {e}"))?;
    Ok((num * unit as f64) as i64)
}
