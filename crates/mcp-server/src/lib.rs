//! MCP server for minio-rs: programmatic server lifecycle and S3 benchmarking.
//!
//! Exposes 4 tools via the Model Context Protocol over stdio transport.

use std::sync::Arc;
use std::time::Duration;

use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::ErrorData;
use s3perf::SseConfig;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

// ── State ──────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct McpServer {
    inner: Arc<McpServerInner>,
}

struct McpServerInner {
    server_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    server_token: Mutex<Option<CancellationToken>>,
    server_address: Mutex<Option<String>>,
}

impl McpServer {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(McpServerInner {
                server_task: Mutex::new(None),
                server_token: Mutex::new(None),
                server_address: Mutex::new(None),
            }),
        }
    }
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tool parameter types ───────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StartServerParams {
    /// Disk paths for object storage (at least 1 required)
    pub disks: Vec<String>,
    /// HTTP listen address (default: 0.0.0.0:9000)
    #[schemars(default = "default_address")]
    pub address: Option<String>,
    /// Console web UI address (optional, informational only)
    pub console_address: Option<String>,
}

fn default_address() -> String {
    "0.0.0.0:9000".to_string()
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RunBenchmarkParams {
    /// S3 endpoint host:port
    pub endpoint: String,
    /// Benchmark type: mixed, get, put, delete, list, stat
    pub benchmark: String,
    /// Access key (default: minioadmin)
    #[schemars(default = "default_access_key")]
    pub access_key: Option<String>,
    /// Secret key (default: minioadmin)
    #[schemars(default = "default_secret_key")]
    pub secret_key: Option<String>,
    /// SigV4 region (default: us-east-1)
    #[schemars(default = "default_region")]
    pub region: Option<String>,
    /// Use TLS (default: false)
    #[schemars(default)]
    pub tls: Option<bool>,
    /// Skip TLS cert verification (default: false)
    #[schemars(default)]
    pub insecure: Option<bool>,
    /// Bucket name (default: s3perf-bench)
    #[schemars(default = "default_bucket")]
    pub bucket: Option<String>,
    /// Concurrent workers (default: 4)
    #[schemars(default = "default_concurrency")]
    pub concurrency: Option<usize>,
    /// Benchmark duration e.g. 30s, 5m (default: 30s)
    #[schemars(default = "default_duration")]
    pub duration: Option<String>,
    /// Object size e.g. 1MiB, 10MiB (default: 1MiB)
    #[schemars(default = "default_obj_size")]
    pub obj_size: Option<String>,
    /// Seed object count for prepare phase (default: 100)
    #[schemars(default = "default_objects")]
    pub objects: Option<usize>,
    /// GET fraction for mixed benchmark (default: 0.45)
    #[schemars(default)]
    pub get_distrib: Option<f64>,
    /// STAT fraction for mixed benchmark (default: 0.05)
    #[schemars(default)]
    pub stat_distrib: Option<f64>,
    /// PUT fraction for mixed benchmark (default: 0.25)
    #[schemars(default)]
    pub put_distrib: Option<f64>,
    /// DELETE fraction for mixed benchmark (default: 0.25)
    #[schemars(default)]
    pub delete_distrib: Option<f64>,
}

fn default_access_key() -> String { "minioadmin".to_string() }
fn default_secret_key() -> String { "minioadmin".to_string() }
fn default_region() -> String { "us-east-1".to_string() }
fn default_bucket() -> String { "s3perf-bench".to_string() }
fn default_concurrency() -> usize { 4 }
fn default_duration() -> String { "30s".to_string() }
fn default_obj_size() -> String { "1MiB".to_string() }
fn default_objects() -> usize { 100 }

// ── Tool output types ──────────────────────────────────────────────────────

#[derive(Debug, Serialize, JsonSchema)]
struct ServerStartOutput {
    status: String,
    address: String,
}

#[derive(Debug, Serialize, JsonSchema)]
struct ServerStopOutput {
    status: String,
}

#[derive(Debug, Serialize, JsonSchema)]
struct ServerStatusOutput {
    running: bool,
    address: Option<String>,
}

// ── Error helpers ──────────────────────────────────────────────────────────

fn err_internal(msg: impl Into<String>) -> ErrorData {
    ErrorData::internal_error(msg.into(), None)
}

fn err_invalid(msg: impl Into<String>) -> ErrorData {
    ErrorData::invalid_params(msg.into(), None)
}

// ── Tool implementations ───────────────────────────────────────────────────

#[rmcp::tool_router(server_handler)]
impl McpServer {
    /// Start the minio-rs S3-compatible object storage server.
    #[rmcp::tool(description = "Start the minio-rs S3-compatible object storage server")]
    async fn start_server(
        &self,
        Parameters(params): Parameters<StartServerParams>,
    ) -> Result<Json<ServerStartOutput>, ErrorData> {
        let address = params.address.unwrap_or_else(|| "0.0.0.0:9000".to_string());

        // Check if already running
        {
            let task = self.inner.server_task.lock().await;
            if task.is_some() {
                return Err(err_invalid("server is already running. Stop it first with stop_server."));
            }
        }

        if params.disks.is_empty() {
            return Err(err_invalid("at least one disk path is required"));
        }

        let config = minio_rs::server::ServerConfig {
            address: address.clone(),
            console_address: params.console_address,
            disks: params.disks.clone(),
        };

        let token = CancellationToken::new();
        let cancel = token.clone();
        let addr = address.clone();

        let handle = tokio::spawn(async move {
            if let Err(e) = minio_rs::server::run::run(config, Some(cancel)).await {
                tracing::error!("minio-rs server exited with error: {e}");
            }
        });

        *self.inner.server_task.lock().await = Some(handle);
        *self.inner.server_token.lock().await = Some(token);
        *self.inner.server_address.lock().await = Some(addr.clone());

        // Give the server a moment to bind
        tokio::time::sleep(Duration::from_millis(500)).await;

        Ok(Json(ServerStartOutput {
            status: "started".to_string(),
            address: addr,
        }))
    }

    /// Stop the running minio-rs S3 server.
    #[rmcp::tool(description = "Stop the running minio-rs S3 server")]
    async fn stop_server(&self) -> Result<Json<ServerStopOutput>, ErrorData> {
        let token = self.inner.server_token.lock().await.take();
        let handle = self.inner.server_task.lock().await.take();

        match (token, handle) {
            (Some(token), Some(handle)) => {
                token.cancel();
                match tokio::time::timeout(Duration::from_secs(10), handle).await {
                    Ok(_) => {
                        *self.inner.server_address.lock().await = None;
                        Ok(Json(ServerStopOutput {
                            status: "stopped".to_string(),
                        }))
                    }
                    Err(_) => {
                        *self.inner.server_address.lock().await = None;
                        Err(err_internal("server did not shut down within 10 seconds"))
                    }
                }
            }
            _ => Ok(Json(ServerStopOutput {
                status: "not_running".to_string(),
            })),
        }
    }

    /// Check whether the minio-rs S3 server is currently running.
    #[rmcp::tool(description = "Check if the minio-rs S3 server is running")]
    async fn server_status(&self) -> Result<Json<ServerStatusOutput>, ErrorData> {
        let running = self.inner.server_task.lock().await.is_some();
        let address = self.inner.server_address.lock().await.clone();

        Ok(Json(ServerStatusOutput { running, address }))
    }

    /// Run an S3 performance benchmark against any S3-compatible endpoint.
    #[rmcp::tool(description = "Run an S3 benchmark (mixed/get/put/delete/list/stat) against any S3-compatible endpoint")]
    async fn run_benchmark(
        &self,
        Parameters(params): Parameters<RunBenchmarkParams>,
    ) -> Result<Json<serde_json::Value>, ErrorData> {
        let endpoint = params.endpoint.clone();
        let region = params.region.clone().unwrap_or_else(|| "us-east-1".to_string());
        let bucket = params.bucket.clone().unwrap_or_else(|| "s3perf-bench".to_string());

        let s3_config = s3perf::S3Config {
            host: endpoint.clone(),
            access_key: params.access_key.unwrap_or_else(|| "minioadmin".to_string()),
            secret_key: params.secret_key.unwrap_or_else(|| "minioadmin".to_string()),
            region: region.clone(),
            tls: params.tls.unwrap_or(false),
            insecure: params.insecure.unwrap_or(false),
            no_verify_ssl: params.insecure.unwrap_or(false),
            ca_pem: None,
        };

        let duration = s3perf::parse_duration(&params.duration.unwrap_or_else(|| "30s".to_string()))
            .map_err(|e| err_invalid(format!("invalid duration: {e}")))?;
        let obj_size = s3perf::parse_obj_size(&params.obj_size.unwrap_or_else(|| "1MiB".to_string()))
            .map_err(|e| err_invalid(format!("invalid obj_size: {e}")))?;

        let bc = s3perf::BenchConfig {
            s3_config,
            bucket,
            region,
            concurrency: params.concurrency.unwrap_or(4),
            duration,
            obj_size,
            objects: params.objects.unwrap_or(100),
            clear: true,
            autoterm: false,
            autoterm_dur: Duration::from_secs(15),
            autoterm_pct: 0.075,
            output: None,
            host_select: s3perf::HostSelect::RoundRobin,
            hosts: vec![endpoint],
            no_prefix: false,
            prefix: None,
            sse: SseConfig::default(),
            rps_limit: None,
        };

        let agg = match params.benchmark.as_str() {
            "mixed" => {
                let g = params.get_distrib.unwrap_or(0.45);
                let s = params.stat_distrib.unwrap_or(0.05);
                let p = params.put_distrib.unwrap_or(0.25);
                let d = params.delete_distrib.unwrap_or(0.25);
                s3perf::run_mixed(&bc, g, s, p, d).await
            }
            "get" => s3perf::run_get(&bc, 1, None, false).await,
            "put" => s3perf::run_put(&bc, false, None, false).await,
            "delete" => s3perf::run_delete(&bc, 100).await,
            "list" => s3perf::run_list(&bc, false).await,
            "stat" => s3perf::run_stat(&bc).await,
            other => {
                return Err(err_invalid(format!(
                    "unsupported benchmark type: '{other}'. Supported: mixed, get, put, delete, list, stat"
                )));
            }
        }
        .map_err(|e| err_internal(format!("benchmark failed: {e}")))?;

        serde_json::to_value(&agg)
            .map(Json)
            .map_err(|e| err_internal(format!("serialization error: {e}")))
    }
}
