//! Iceberg REST Catalog benchmark entrypoints.

use crate::aggregate::Aggregated;
use crate::bench::iceberg_commits::IcebergCommitsBenchmark;
use crate::bench::iceberg_mixed::IcebergMixedBenchmark;
use crate::bench::iceberg_read::IcebergReadBenchmark;
use crate::bench::iceberg_sustained::IcebergSustainedBenchmark;
use crate::bench::sse::SseConfig;
use crate::bench::Common;
use crate::generator::{DefaultSource, ObjSize};
use crate::iceberg::tree::TreeConfig;
use crate::iceberg::{CatalogConfig, ExternalCatalogType, RetryConfig};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::{collector_with_optional_influx, new_host_inflight, run_benchmark, BenchConfig};

/// Placeholder `catalog-write` entrypoint (use `catalog-commits` or `sustained` for real IO).
pub async fn run_iceberg_catalog_write_stub(page_size: usize) -> anyhow::Result<Aggregated> {
    tracing::info!(
        "iceberg catalog-write (page_size={page_size}) is stubbed; run `iceberg catalog-commits` or `iceberg sustained` instead."
    );
    Ok(Aggregated {
        mixed: false,
        operations: vec![],
        mixed_server_stats: None,
        mixed_throughput_by_host: std::collections::HashMap::new(),
    })
}

fn build_iceberg_common(bc: &BenchConfig, remote_hosts: Option<String>) -> anyhow::Result<Common> {
    let collector = collector_with_optional_influx()?;
    let host_inflight = new_host_inflight(&bc.hosts);
    Ok(Common {
        concurrency: bc.concurrency,
        duration: bc.duration,
        bucket: "iceberg-bench".into(),
        location: bc.region.clone(),
        source: Arc::new(|| {
            Box::new(DefaultSource::new(
                "s3perf-iceberg".into(),
                ObjSize::Fixed(1024),
                rand::random(),
            ))
        }),
        client_factory: Arc::new(|_: usize| {
            panic!("Iceberg benchmarks don't use S3 client directly")
        }),
        collector,
        client_idx: 0,
        total_clients: 1,
        client_mode: remote_hosts.is_some(),
        clear: bc.clear,
        discard_output: bc.output.is_none(),
        versioned: false,
        locking: false,
        auto_term_dur: if bc.autoterm {
            Some(bc.autoterm_dur)
        } else {
            None
        },
        auto_term_scale: bc.autoterm_pct,
        rps_limit: None,
        host_select: bc.host_select,
        no_prefix: false,
        custom_prefix: None,
        obj_size: ObjSize::Fixed(1024),
        objects: 0,
        versions: 1,
        bench_data: bc.output.clone(),
        analyze_only: false,
        hosts: bc.hosts.clone(),
        host_inflight,
        rps_limiter: crate::bench::rate_limiter::opt_rps_limiter(None),
        sse: SseConfig::None,
        checksum: None,
    })
}

pub async fn run_iceberg_read(
    bc: &BenchConfig,
    remote_hosts: Option<String>,
    page_size: usize,
    namespace_width: usize,
    namespace_depth: usize,
    tables_per_ns: usize,
    views_per_ns: usize,
    columns: usize,
    properties: usize,
    base_location: String,
    external_catalog: Option<String>,
    catalog_name: String,
) -> anyhow::Result<Aggregated> {
    let common = build_iceberg_common(bc, remote_hosts)?;

    let tree_config = TreeConfig {
        namespace_width,
        namespace_depth,
        tables_per_ns,
        views_per_ns,
        columns,
        properties,
        base_location,
        catalog_name,
    };

    let ext = external_catalog
        .map(|s| ExternalCatalogType::from_str(&s))
        .unwrap_or_default();

    let catalog_config = CatalogConfig {
        catalog_uri: common
            .hosts
            .first()
            .cloned()
            .unwrap_or_else(|| "http://localhost:9001".into()),
        warehouse: tree_config.catalog_name.clone(),
        access_key: "minioadmin".into(),
        secret_key: "minioadmin".into(),
        region: common.location.clone(),
        tls: false,
        external_catalog: ext,
    };

    let bm = IcebergReadBenchmark {
        common,
        catalog_config,
        catalog: None,
        catalog_pool: None,
        tree_config,
        dist: None,
        page_size,
        external_catalog: ExternalCatalogType::None,
        tree: Mutex::new(None),
        namespaces: Mutex::new(Vec::new()),
        tables: Mutex::new(Vec::new()),
        views: Mutex::new(Vec::new()),
        ops: Mutex::new(Vec::new()),
    };

    run_benchmark(Arc::new(bm)).await
}

pub async fn run_iceberg_commits(
    bc: &BenchConfig,
    remote_hosts: Option<String>,
    table_commits_throughput: usize,
    view_commits_throughput: usize,
    max_retries: usize,
    retry_backoff_ms: u64,
    backoff_max_ms: u64,
    namespace_width: usize,
    namespace_depth: usize,
    tables_per_ns: usize,
    views_per_ns: usize,
    columns: usize,
    properties: usize,
    base_location: String,
    external_catalog: Option<String>,
    catalog_name: String,
) -> anyhow::Result<Aggregated> {
    let common = build_iceberg_common(bc, remote_hosts)?;

    let tree_config = TreeConfig {
        namespace_width,
        namespace_depth,
        tables_per_ns,
        views_per_ns,
        columns,
        properties,
        base_location,
        catalog_name,
    };
    let ext = external_catalog
        .map(|s| ExternalCatalogType::from_str(&s))
        .unwrap_or_default();

    let catalog_config = CatalogConfig {
        catalog_uri: common
            .hosts
            .first()
            .cloned()
            .unwrap_or_else(|| "http://localhost:9001".into()),
        warehouse: tree_config.catalog_name.clone(),
        access_key: "minioadmin".into(),
        secret_key: "minioadmin".into(),
        region: common.location.clone(),
        tls: false,
        external_catalog: ext,
    };

    let bm = IcebergCommitsBenchmark {
        common,
        catalog_config,
        catalog: None,
        catalog_pool: None,
        tree_config,
        external_catalog: ExternalCatalogType::None,
        table_workers: table_commits_throughput,
        view_workers: view_commits_throughput,
        retry_config: RetryConfig {
            max_retries,
            base_backoff: Duration::from_millis(retry_backoff_ms),
            max_backoff: Duration::from_millis(backoff_max_ms),
        },
        tables: Mutex::new(Vec::new()),
        views: Mutex::new(Vec::new()),
        tree: Mutex::new(None),
        ops: Mutex::new(Vec::new()),
    };

    run_benchmark(Arc::new(bm)).await
}

pub async fn run_iceberg_mixed(
    bc: &BenchConfig,
    remote_hosts: Option<String>,
    max_retries: usize,
    retry_backoff_ms: u64,
    backoff_max_ms: u64,
    page_size: usize,
    namespace_width: usize,
    namespace_depth: usize,
    tables_per_ns: usize,
    views_per_ns: usize,
    columns: usize,
    properties: usize,
    base_location: String,
    external_catalog: Option<String>,
    catalog_name: String,
) -> anyhow::Result<Aggregated> {
    let common = build_iceberg_common(bc, remote_hosts)?;

    let tree_config = TreeConfig {
        namespace_width,
        namespace_depth,
        tables_per_ns,
        views_per_ns,
        columns,
        properties,
        base_location,
        catalog_name,
    };
    let ext = external_catalog
        .map(|s| ExternalCatalogType::from_str(&s))
        .unwrap_or_default();

    let catalog_config = CatalogConfig {
        catalog_uri: common
            .hosts
            .first()
            .cloned()
            .unwrap_or_else(|| "http://localhost:9001".into()),
        warehouse: tree_config.catalog_name.clone(),
        access_key: "minioadmin".into(),
        secret_key: "minioadmin".into(),
        region: common.location.clone(),
        tls: false,
        external_catalog: ext,
    };

    let bm = IcebergMixedBenchmark {
        common,
        catalog_config,
        catalog: None,
        catalog_pool: None,
        tree_config,
        external_catalog: ExternalCatalogType::None,
        dist: None,
        page_size,
        retry_config: RetryConfig {
            max_retries,
            base_backoff: Duration::from_millis(retry_backoff_ms),
            max_backoff: Duration::from_millis(backoff_max_ms),
        },
        namespaces: Mutex::new(Vec::new()),
        tables: Mutex::new(Vec::new()),
        views: Mutex::new(Vec::new()),
        tree: Mutex::new(None),
        ns_update_id: std::sync::atomic::AtomicU64::new(0),
        table_update_id: std::sync::atomic::AtomicU64::new(0),
        view_update_id: std::sync::atomic::AtomicU64::new(0),
        ops: Mutex::new(Vec::new()),
    };

    run_benchmark(Arc::new(bm)).await
}

pub async fn run_iceberg_sustained(
    bc: &BenchConfig,
    remote_hosts: Option<String>,
    num_files: usize,
    rows_per_file: usize,
    files_per_commit: usize,
    tpcds: bool,
    scale_factor: f64,
    tpcds_table: Option<String>,
    cache_dir: String,
    skip_upload: bool,
    simulate_read: bool,
    read_concurrent: usize,
    read_rps_limit: f64,
    max_retries: usize,
    retry_backoff_ms: u64,
    backoff_max_ms: u64,
    namespace_width: usize,
    namespace_depth: usize,
    tables_per_ns: usize,
    columns: usize,
    properties: usize,
    base_location: String,
    external_catalog: Option<String>,
    catalog_name: String,
) -> anyhow::Result<Aggregated> {
    let common = build_iceberg_common(bc, remote_hosts)?;

    let tree_config = TreeConfig {
        namespace_width,
        namespace_depth,
        tables_per_ns,
        views_per_ns: 0,
        columns,
        properties,
        base_location,
        catalog_name,
    };
    let ext = external_catalog
        .map(|s| ExternalCatalogType::from_str(&s))
        .unwrap_or_default();

    let catalog_config = CatalogConfig {
        catalog_uri: common
            .hosts
            .first()
            .cloned()
            .unwrap_or_else(|| "http://localhost:9001".into()),
        warehouse: tree_config.catalog_name.clone(),
        access_key: "minioadmin".into(),
        secret_key: "minioadmin".into(),
        region: common.location.clone(),
        tls: false,
        external_catalog: ext,
    };

    let bm = IcebergSustainedBenchmark {
        common,
        catalog_config,
        catalog: None,
        catalog_pool: None,
        tree_config,
        external_catalog: ExternalCatalogType::None,
        num_files,
        rows_per_file,
        files_per_commit,
        tpcds,
        scale_factor,
        tpcds_table,
        cache_dir,
        skip_upload,
        simulate_read,
        read_concurrent,
        read_rps_limit,
        retry_config: RetryConfig {
            max_retries,
            base_backoff: Duration::from_millis(retry_backoff_ms),
            max_backoff: Duration::from_millis(backoff_max_ms),
        },
        tables: Mutex::new(Vec::new()),
        tree: Mutex::new(None),
        ops: Mutex::new(Vec::new()),
    };

    run_benchmark(Arc::new(bm)).await
}
