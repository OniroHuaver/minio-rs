//! YAML-driven run configuration for `s3perf run <file>`.
//!
//! Supports load, validation, and flattening into CLI argv (`to_cli_args`).
//! Root YAML key is `s3perf` (schema version `api: v1`).

use serde::Deserialize;
use std::fs;

// --- Constants ---
/// Supported benchmark names.
pub const VALID_BENCHMARKS: &[&str] = &[
    "mixed", "get", "put", "delete", "list", "stat",
    "versioned", "retention", "multipart", "multipart-put",
    "snowball", "fanout", "append", "zip",
];

/// Default benchmark bucket name (aligned with CLI defaults).
pub const DEFAULT_S3PERF_BUCKET: &str = "s3perf-benchmark-bucket";

/// YAML key to CLI flag renames for legacy-style keys in config files.
const RENAME_MAP: &[(&str, &str)] = &[
    ("sse-c-encrypt", "encrypt"),
    ("sse-s3-encrypt", "sse-s3-encrypt"),
    ("remote-hosts", "remote-hosts"),
    ("server-profile", "serverprof"),
    ("no-clear", "noclear"),
    ("keep-data", "keepdata"),
    ("sign-version", "signversion"),
    ("bench-data", "benchdata"),
    ("rps-limit", "rpslimit"),
    ("obj.rand-size", "obj.randsize"),
    ("obj.part-size", "obj.partsize"),
];

// --- Top-level ---
/// Parsed run file (`s3perf:` root).
#[derive(Debug, Deserialize)]
pub struct RunFileConfig {
    pub s3perf: RunSpec,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct RunSpec {
    /// API version; must be `"v1"`.
    pub api: String,

    /// Benchmark name (required).
    pub benchmark: String,

    /// Quiet mode (less console output).
    #[serde(default)]
    pub quiet: bool,

    /// Optional output path for bench data files.
    #[serde(default)]
    pub bench_data: Option<String>,

    /// Remote S3 endpoint and credentials.
    #[serde(default)]
    pub remote: RemoteConfig,

    /// Benchmark tuning parameters.
    #[serde(default)]
    pub params: BenchmarkParams,
}

// --- Remote (S3) ---
/// S3 endpoint and credentials for the run file.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct RemoteConfig {
    /// S3 endpoint `host:port`.
    #[serde(default = "default_host")]
    pub host: String,

    /// Access key id.
    #[serde(default = "default_access_key")]
    pub access_key: String,

    /// Secret access key.
    #[serde(default = "default_secret_key")]
    pub secret_key: String,

    /// AWS-style region string.
    #[serde(default = "default_region")]
    pub region: String,

    /// Use TLS for S3 endpoint.
    #[serde(default)]
    pub tls: bool,

    /// Skip TLS certificate verification.
    #[serde(default)]
    pub insecure: bool,

    /// Benchmark bucket name.
    #[serde(default = "default_bucket")]
    pub bucket: String,
}

impl Default for RemoteConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            access_key: default_access_key(),
            secret_key: default_secret_key(),
            region: default_region(),
            tls: false,
            insecure: false,
            bucket: default_bucket(),
        }
    }
}

// --- Benchmark parameters ---
/// Timing, concurrency, and object settings (`params`).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct BenchmarkParams {
    /// Run duration (`5m`, `30s`, `1h`, etc.).
    #[serde(default = "default_duration_str")]
    pub duration: String,

    /// Concurrent workers.
    #[serde(default = "default_concurrent")]
    pub concurrent: usize,

    /// Object count during prepare phase.
    #[serde(default = "default_objects")]
    pub objects: usize,

    /// Version count (versioned workloads).
    #[serde(default)]
    pub versions: usize,

    #[serde(default)]
    pub obj: ObjParams,

    /// Operation mix percentages for mixed / versioned workloads.
    #[serde(default)]
    pub distribution: Option<DistribParams>,

    #[serde(default)]
    pub autoterm: AutotermParams,

    /// Do not delete objects after run (`--noclear`).
    #[serde(default)]
    pub no_clear: bool,
}

impl Default for BenchmarkParams {
    fn default() -> Self {
        Self {
            duration: default_duration_str(),
            concurrent: default_concurrent(),
            objects: default_objects(),
            versions: 0,
            obj: ObjParams::default(),
            distribution: None,
            autoterm: AutotermParams::default(),
            no_clear: false,
        }
    }
}

/// Object sizing (`params.obj`).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ObjParams {
    /// Fixed object size (e.g. `1MiB`, `10MiB`).
    #[serde(default = "default_obj_size")]
    pub size: String,

    /// Random object sizes up to `--obj-size`.
    #[serde(default)]
    pub rand_size: bool,
}

impl Default for ObjParams {
    fn default() -> Self {
        Self {
            size: default_obj_size(),
            rand_size: false,
        }
    }
}

/// Operation mix (`params.distribution`).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DistribParams {
    /// GET share (percentage, e.g. 45.0).
    #[serde(default)]
    pub get: f64,

    #[serde(default)]
    pub stat: f64,

    #[serde(default)]
    pub put: f64,

    #[serde(default)]
    pub delete: f64,
}

impl Default for DistribParams {
    fn default() -> Self {
        Self {
            get: 0.0,
            stat: 0.0,
            put: 0.0,
            delete: 0.0,
        }
    }
}

/// Auto-stop tuning (`params.autoterm`).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct AutotermParams {
    /// Enable throughput auto-stop.
    #[serde(default)]
    pub enabled: bool,

    /// Minimum stable duration before auto-stop triggers.
    #[serde(default = "default_autoterm_dur")]
    pub dur: String,

    /// Coefficient of variation threshold (percent).
    #[serde(default = "default_autoterm_pct")]
    pub pct: f64,
}

impl Default for AutotermParams {
    fn default() -> Self {
        Self {
            enabled: false,
            dur: default_autoterm_dur(),
            pct: default_autoterm_pct(),
        }
    }
}

// --- Defaults ---

fn default_host() -> String { "localhost:9000".into() }
fn default_access_key() -> String { "minioadmin".into() }
fn default_secret_key() -> String { "minioadmin".into() }
fn default_region() -> String { "us-east-1".into() }
fn default_bucket() -> String { DEFAULT_S3PERF_BUCKET.into() }
fn default_duration_str() -> String { "5m".into() }
fn default_concurrent() -> usize { 20 }
fn default_objects() -> usize { 10000 }
fn default_obj_size() -> String { "1MiB".into() }
fn default_autoterm_dur() -> String { "15s".into() }
fn default_autoterm_pct() -> f64 { 7.5 }

// --- Errors ---

/// Errors loading or validating a run file.
#[derive(Debug)]
pub enum RunConfigError {
    Io {
        path: String,
        source: std::io::Error,
    },
    Parse {
        path: String,
        source: serde_yaml::Error,
    },
    Validation(String),
}

impl std::fmt::Display for RunConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(f, "failed to read config file '{}': {}", path, source)
            }
            Self::Parse { path, source } => {
                write!(f, "failed to parse config file '{}': {}", path, source)
            }
            Self::Validation(msg) => write!(f, "config validation failed: {msg}"),
        }
    }
}

impl std::error::Error for RunConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
            Self::Validation(_) => None,
        }
    }
}

impl RunFileConfig {
    /// Load a YAML run file from disk.
    pub fn load(path: &str) -> Result<Self, RunConfigError> {
        let content = fs::read_to_string(path).map_err(|e| RunConfigError::Io {
            path: path.to_string(),
            source: e,
        })?;

        let config: Self =
            serde_yaml::from_str(&content).map_err(|e| RunConfigError::Parse {
                path: path.to_string(),
                source: e,
            })?;

        Ok(config)
    }

    /// Validate API version, benchmark name, and required remote fields.
    pub fn validate(&self) -> Result<(), RunConfigError> {
        if self.s3perf.api != "v1" {
            return Err(RunConfigError::Validation(format!(
                "unsupported API version '{}', only 'v1' is supported",
                self.s3perf.api
            )));
        }

        let bm = self.s3perf.benchmark.as_str();
        if !VALID_BENCHMARKS.contains(&bm) {
            let valid = VALID_BENCHMARKS.join(", ");
            return Err(RunConfigError::Validation(format!(
                "invalid benchmark type '{}'; valid values: {}",
                bm, valid
            )));
        }

        let r = &self.s3perf.remote;
        if r.host.is_empty() {
            return Err(RunConfigError::Validation("remote.host cannot be empty".into()));
        }
        if r.access_key.is_empty() {
            return Err(RunConfigError::Validation(
                "remote.access-key cannot be empty".into(),
            ));
        }
        if r.secret_key.is_empty() {
            return Err(RunConfigError::Validation(
                "remote.secret-key cannot be empty".into(),
            ));
        }
        if r.bucket.is_empty() {
            return Err(RunConfigError::Validation("remote.bucket cannot be empty".into()));
        }

        Ok(())
    }

    /// Flatten into CLI argv: `[subcommand, ..."--flag=value"]`.
    pub fn to_cli_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        let r = &self.s3perf.remote;
        let p = &self.s3perf.params;

        args.push(self.s3perf.benchmark.clone());

        push_bool(&mut args, self.s3perf.quiet, "--quiet");
        push_bool(&mut args, r.tls, "--tls");
        push_bool(&mut args, r.insecure, "--insecure");
        push_bool(&mut args, p.obj.rand_size, "--obj-randsize");
        push_bool(&mut args, p.autoterm.enabled, "--autoterm");
        push_bool(&mut args, p.no_clear, "--noclear");

        if let Some(ref v) = self.s3perf.bench_data {
            if !v.is_empty() {
                args.push(format!("--benchdata={}", v));
            }
        }

        push_kv(&mut args, "--host", &r.host, &default_host());
        push_kv(
            &mut args,
            "--access-key",
            &r.access_key,
            &default_access_key(),
        );
        push_kv(
            &mut args,
            "--secret-key",
            &r.secret_key,
            &default_secret_key(),
        );
        push_kv(&mut args, "--region", &r.region, &default_region());
        push_kv(&mut args, "--bucket", &r.bucket, &default_bucket());

        push_kv(
            &mut args,
            "--duration",
            &p.duration,
            &default_duration_str(),
        );
        push_kv(
            &mut args,
            "--concurrent",
            &p.concurrent.to_string(),
            &default_concurrent().to_string(),
        );
        push_kv(
            &mut args,
            "--objects",
            &p.objects.to_string(),
            &default_objects().to_string(),
        );

        if p.versions > 1 {
            args.push(format!("--versions={}", p.versions));
        }

        push_kv(&mut args, "--obj-size", &p.obj.size, &default_obj_size());

        if let Some(ref d) = p.distribution {
            args.push(format!("--get-distrib={}", d.get / 100.0));
            args.push(format!("--stat-distrib={}", d.stat / 100.0));
            args.push(format!("--put-distrib={}", d.put / 100.0));
            args.push(format!("--delete-distrib={}", d.delete / 100.0));
        }

        if p.autoterm.enabled {
            push_kv(
                &mut args,
                "--autoterm-dur",
                &p.autoterm.dur,
                &default_autoterm_dur(),
            );
            push_kv(
                &mut args,
                "--autoterm-pct",
                &p.autoterm.pct.to_string(),
                &default_autoterm_pct().to_string(),
            );
        }

        args
    }
}

// --- Helpers ---

/// Append `flag` when `cond`.
fn push_bool(args: &mut Vec<String>, cond: bool, flag: &str) {
    if cond {
        args.push(flag.to_string());
    }
}

/// Append `flag=value` when value differs from `default`.
fn push_kv(args: &mut Vec<String>, flag: &str, value: &str, default: &str) {
    if value != default {
        args.push(format!("{}={}", flag, value));
    }
}

// --- Tests ---

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_minimal() {
        let yaml = r#"
s3perf:
  api: v1
  benchmark: mixed
  remote:
    host: localhost:9000
    access-key: minioadmin
    secret-key: minioadmin
    bucket: s3perf-benchmark-bucket
  params:
    duration: 5m
    concurrent: 20
    objects: 10000
    obj:
      size: 1MiB
"#;
        let config: RunFileConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.s3perf.api, "v1");
        assert_eq!(config.s3perf.benchmark, "mixed");
        assert!(!config.s3perf.quiet);
        assert!(config.s3perf.bench_data.is_none());
        assert_eq!(config.s3perf.remote.host, "localhost:9000");
        assert_eq!(config.s3perf.params.duration, "5m");
        assert_eq!(config.s3perf.params.concurrent, 20);
        assert_eq!(config.s3perf.params.objects, 10000);
        assert_eq!(config.s3perf.params.obj.size, "1MiB");
        assert!(config.s3perf.params.distribution.is_none());
        assert!(!config.s3perf.params.autoterm.enabled);
        assert!(!config.s3perf.params.no_clear);
    }

    #[test]
    fn test_deserialize_full() {
        let yaml = r#"
s3perf:
  api: v1
  benchmark: versioned
  quiet: true
  bench-data: /tmp/bench.json
  remote:
    host: s3.example.com:443
    access-key: AKID123
    secret-key: s3cr3t
    region: us-west-2
    tls: true
    insecure: false
    bucket: test-bucket
  params:
    duration: 10m
    concurrent: 50
    objects: 50000
    versions: 3
    distribution:
      get: 45.0
      stat: 30.0
      put: 15.0
      delete: 10.0
    obj:
      size: 4MiB
      rand-size: true
    autoterm:
      enabled: true
      dur: 30s
      pct: 5.0
    no-clear: true
"#;
        let config: RunFileConfig = serde_yaml::from_str(yaml).unwrap();

        assert_eq!(config.s3perf.api, "v1");
        assert_eq!(config.s3perf.benchmark, "versioned");
        assert!(config.s3perf.quiet);
        assert_eq!(config.s3perf.bench_data.unwrap(), "/tmp/bench.json");

        assert_eq!(config.s3perf.remote.host, "s3.example.com:443");
        assert_eq!(config.s3perf.remote.access_key, "AKID123");
        assert_eq!(config.s3perf.remote.secret_key, "s3cr3t");
        assert_eq!(config.s3perf.remote.region, "us-west-2");
        assert!(config.s3perf.remote.tls);
        assert!(!config.s3perf.remote.insecure);
        assert_eq!(config.s3perf.remote.bucket, "test-bucket");

        assert_eq!(config.s3perf.params.duration, "10m");
        assert_eq!(config.s3perf.params.concurrent, 50);
        assert_eq!(config.s3perf.params.objects, 50000);
        assert_eq!(config.s3perf.params.versions, 3);
        assert!(config.s3perf.params.no_clear);
        assert_eq!(config.s3perf.params.obj.size, "4MiB");
        assert!(config.s3perf.params.obj.rand_size);

        let d = config.s3perf.params.distribution.unwrap();
        assert!((d.get - 45.0).abs() < 1e-9);
        assert!((d.stat - 30.0).abs() < 1e-9);
        assert!((d.put - 15.0).abs() < 1e-9);
        assert!((d.delete - 10.0).abs() < 1e-9);

        assert!(config.s3perf.params.autoterm.enabled);
        assert_eq!(config.s3perf.params.autoterm.dur, "30s");
        assert!((config.s3perf.params.autoterm.pct - 5.0).abs() < 1e-9);
    }

    #[test]
    fn test_deserialize_with_defaults() {
        let yaml = r#"
s3perf:
  api: v1
  benchmark: get
  remote:
    host: myhost:9000
    access-key: ak
    secret-key: sk
    bucket: mybucket
"#;
        let config: RunFileConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(!config.s3perf.quiet);
        assert_eq!(config.s3perf.remote.region, "us-east-1");
        assert!(!config.s3perf.remote.tls);
        assert_eq!(config.s3perf.params.duration, "5m");
        assert_eq!(config.s3perf.params.concurrent, 20);
        assert_eq!(config.s3perf.params.objects, 10000);
        assert_eq!(config.s3perf.params.obj.size, "1MiB");
        assert!(!config.s3perf.params.obj.rand_size);
        assert!(!config.s3perf.params.autoterm.enabled);
        assert!(!config.s3perf.params.no_clear);
    }

    #[test]
    fn test_validate_ok() {
        let yaml = r#"
s3perf:
  api: v1
  benchmark: mixed
  remote:
    host: localhost:9000
    access-key: minioadmin
    secret-key: minioadmin
    bucket: test
"#;
        let config: RunFileConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_bad_api() {
        let yaml = r#"
s3perf:
  api: v2
  benchmark: mixed
  remote:
    host: localhost:9000
    access-key: minioadmin
    secret-key: minioadmin
    bucket: test
"#;
        let config: RunFileConfig = serde_yaml::from_str(yaml).unwrap();
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("v2"), "unexpected message: {err}");
    }

    #[test]
    fn test_validate_bad_benchmark() {
        let yaml = r#"
s3perf:
  api: v1
  benchmark: nonexistent
  remote:
    host: localhost:9000
    access-key: minioadmin
    secret-key: minioadmin
    bucket: test
"#;
        let config: RunFileConfig = serde_yaml::from_str(yaml).unwrap();
        let err = config.validate().unwrap_err();
        assert!(
            err.to_string().contains("nonexistent"),
            "unexpected validation message: {err}"
        );
    }

    #[test]
    fn test_validate_empty_host() {
        let yaml = r#"
s3perf:
  api: v1
  benchmark: get
  remote:
    host: ""
    access-key: minioadmin
    secret-key: minioadmin
    bucket: test
"#;
        let config: RunFileConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_empty_bucket() {
        let yaml = r#"
s3perf:
  api: v1
  benchmark: get
  remote:
    host: localhost:9000
    access-key: minioadmin
    secret-key: minioadmin
    bucket: ""
"#;
        let config: RunFileConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.validate().is_err());
    }

    // ── to_cli_args ──────────────────────────────────────────────

    #[test]
    fn test_to_cli_args_defaults_omitted() {
        let yaml = r#"
s3perf:
  api: v1
  benchmark: get
  remote:
    host: localhost:9000
    access-key: minioadmin
    secret-key: minioadmin
    bucket: s3perf-benchmark-bucket
  params:
    duration: 5m
    concurrent: 20
    objects: 10000
    obj:
      size: 1MiB
"#;
        let config: RunFileConfig = serde_yaml::from_str(yaml).unwrap();
        let args = config.to_cli_args();
        assert_eq!(args, vec!["get"], "expected only subcommand for defaults: {args:?}");
    }

    #[test]
    fn test_to_cli_args_all_flags() {
        let yaml = r#"
s3perf:
  api: v1
  benchmark: mixed
  quiet: true
  bench-data: output.json
  remote:
    host: remote:9000
    access-key: admin
    secret-key: admin123
    region: eu-central-1
    tls: true
    insecure: true
    bucket: bench-bucket
  params:
    duration: 10m
    concurrent: 30
    objects: 20000
    versions: 3
    distribution:
      get: 50.0
      stat: 20.0
      put: 20.0
      delete: 10.0
    obj:
      size: 2MiB
      rand-size: true
    autoterm:
      enabled: true
      dur: 30s
      pct: 5.0
    no-clear: true
"#;
        let config: RunFileConfig = serde_yaml::from_str(yaml).unwrap();
        let args = config.to_cli_args();
        let joined = args.join(" ");

        assert_eq!(args[0], "mixed");
        assert!(joined.contains("--quiet"));
        assert!(joined.contains("--tls"));
        assert!(joined.contains("--insecure"));
        assert!(joined.contains("--obj-randsize"));
        assert!(joined.contains("--autoterm"));
        assert!(joined.contains("--noclear"));

        assert!(joined.contains("--host=remote:9000"));
        assert!(!joined.contains("--host=localhost:9000"));
        assert!(joined.contains("--access-key=admin"));
        assert!(joined.contains("--secret-key=admin123"));
        assert!(joined.contains("--region=eu-central-1"));
        assert!(joined.contains("--bucket=bench-bucket"));
        assert!(joined.contains("--duration=10m"));
        assert!(joined.contains("--concurrent=30"));
        assert!(joined.contains("--objects=20000"));
        assert!(joined.contains("--versions=3"));
        assert!(joined.contains("--obj-size=2MiB"));

        assert!(joined.contains("--get-distrib=0.5"));
        assert!(joined.contains("--stat-distrib=0.2"));

        assert!(joined.contains("--autoterm-dur=30s"));
        assert!(joined.contains("--autoterm-pct=5"));
    }

    #[test]
    fn test_to_cli_args_autoterm_disabled() {
        let yaml = r#"
s3perf:
  api: v1
  benchmark: put
  remote:
    host: localhost:9000
    access-key: minioadmin
    secret-key: minioadmin
    bucket: test
  params:
    duration: 10m
    concurrent: 20
    objects: 10000
    obj:
      size: 1MiB
    autoterm:
      enabled: false
"#;
        let config: RunFileConfig = serde_yaml::from_str(yaml).unwrap();
        let args = config.to_cli_args();
        let joined = args.join(" ");

        assert!(!joined.contains("--autoterm"));
        assert!(!joined.contains("--autoterm-dur"));
        assert!(!joined.contains("--autoterm-pct"));
    }

    #[test]
    fn test_to_cli_args_single_value_distribution() {
        let yaml = r#"
s3perf:
  api: v1
  benchmark: mixed
  remote:
    host: localhost:9000
    access-key: minioadmin
    secret-key: minioadmin
    bucket: test
  params:
    duration: 5m
    concurrent: 20
    objects: 10000
    obj:
      size: 1MiB
    distribution:
      get: 100.0
      stat: 0.0
      put: 0.0
      delete: 0.0
"#;
        let config: RunFileConfig = serde_yaml::from_str(yaml).unwrap();
        let args = config.to_cli_args();
        let joined = args.join(" ");

        assert!(joined.contains("--get-distrib=1"));
        assert!(joined.contains("--stat-distrib=0"));
    }

    #[test]
    fn test_valid_benchmarks_list() {
        let expected: &[&str] = &[
            "mixed", "get", "put", "delete", "list", "stat",
            "versioned", "retention", "multipart", "multipart-put",
            "snowball", "fanout", "append", "zip",
        ];
        for name in expected {
            assert!(
                VALID_BENCHMARKS.contains(name),
                "VALID_BENCHMARKS must contain '{name}'"
            );
        }
        assert_eq!(VALID_BENCHMARKS.len(), expected.len());
    }

    #[test]
    fn test_all_benchmarks_validate() {
        for &bm in VALID_BENCHMARKS {
            let yaml = format!(
                r#"s3perf:
  api: v1
  benchmark: {bm}
  remote:
    host: localhost:9000
    access-key: minioadmin
    secret-key: minioadmin
    bucket: test"#
            );
            let config: RunFileConfig = serde_yaml::from_str(&yaml).unwrap();
            assert!(
                config.validate().is_ok(),
                "benchmark '{bm}' should validate"
            );
        }
    }

    #[test]
    fn test_default_functions() {
        assert_eq!(default_host(), "localhost:9000");
        assert_eq!(default_access_key(), "minioadmin");
        assert_eq!(default_secret_key(), "minioadmin");
        assert_eq!(default_region(), "us-east-1");
        assert_eq!(default_bucket(), "s3perf-benchmark-bucket");
        assert_eq!(default_duration_str(), "5m");
        assert_eq!(default_concurrent(), 20);
        assert_eq!(default_objects(), 10000);
        assert_eq!(default_obj_size(), "1MiB");
        assert_eq!(default_autoterm_dur(), "15s");
        assert!((default_autoterm_pct() - 7.5).abs() < 1e-9);
    }

    #[test]
    fn test_load_nonexistent_file() {
        let err = RunFileConfig::load("/tmp/nonexistent_run_config_test.yml");
        assert!(err.is_err());
        match err {
            Err(RunConfigError::Io { path, .. }) => {
                assert_eq!(path, "/tmp/nonexistent_run_config_test.yml");
            }
            _ => panic!("expected Io error: {:?}", err),
        }
    }

    #[test]
    fn test_load_invalid_yaml() {
        let path = "/tmp/test_run_config_invalid.yml";
        std::fs::write(path, b"s3perf: [unclosed bracket").ok();
        let err = RunFileConfig::load(path);
        let _ = std::fs::remove_file(path);
        assert!(err.is_err());
        match err {
            Err(RunConfigError::Parse { .. }) => {}
            _ => panic!("expected Parse error: {:?}", err),
        }
    }

    #[test]
    fn test_load_success() {
        let yaml = r#"
s3perf:
  api: v1
  benchmark: list
  remote:
    host: localhost:9000
    access-key: minioadmin
    secret-key: minioadmin
    bucket: test-bucket
  params:
    duration: 1m
    concurrent: 10
    objects: 100
    obj:
      size: 512KiB
"#;
        let path = "/tmp/test_run_config_valid.yml";
        std::fs::write(path, yaml).unwrap();
        let config = RunFileConfig::load(path).unwrap();
        let _ = std::fs::remove_file(path);

        assert_eq!(config.s3perf.benchmark, "list");
        assert_eq!(config.s3perf.params.duration, "1m");
    }

    #[test]
    fn test_error_display_and_source() {
        use std::error::Error as _;

        let io_err = RunConfigError::Io {
            path: "cfg.yml".into(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "not found"),
        };
        let msg = io_err.to_string();
        assert!(msg.contains("cfg.yml"), "display should contain path: {msg}");
        assert!(io_err.source().is_some());

        let parse_err = RunConfigError::Parse {
            path: "bad.yml".into(),
            source: serde_yaml::from_str::<()>("[invalid").unwrap_err(),
        };
        assert!(parse_err.source().is_some());

        let val_err = RunConfigError::Validation("bad config".into());
        assert_eq!(
            val_err.to_string(),
            "config validation failed: bad config"
        );
        assert!(val_err.source().is_none());
    }
}
