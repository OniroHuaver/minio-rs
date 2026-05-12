//! Binary entrypoint for the `s3perf` S3 benchmark CLI.

// Distributed path and Iceberg catalog subtypes aren't exercised in every binary config.
#![allow(dead_code)]

mod aggregate;
mod api;
mod bench;
mod cli;
mod client;
mod config;
mod generator;
mod iceberg;
mod influxdb;
mod server;
mod tui;

use clap::Parser;
use cli::app::{parse_duration, parse_obj_size, parse_size, Cli, Commands};
use cli::{
    execute_run_yaml, run_delete, run_get, run_list, run_mixed, run_put, run_stat, run_zip,
    BenchConfig,
};
use bench::s3_client::S3Config;
use bench::sse::SseConfig;
use bench::HostSelect;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let cli = Cli::parse();
    cli::set_influx_url(cli.influxdb.clone());

    // Shared S3 endpoint config from global flags
    let s3_config = S3Config {
        host: cli.host.clone(),
        access_key: cli.access_key.clone(),
        secret_key: cli.secret_key.clone(),
        region: cli.region.clone(),
        tls: cli.tls,
        insecure: cli.insecure,
        no_verify_ssl: cli.insecure,
        ca_pem: None,
    };

    let host_select: HostSelect = cli.host_select.parse().map_err(|e: String| anyhow::anyhow!(e))?;
    let hosts: Vec<String> = cli
        .hosts
        .as_deref()
        .map(|h| h.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_else(|| vec![cli.host.clone()]);

    let duration = parse_duration(&cli.duration).map_err(|e| anyhow::anyhow!(e))?;
    let obj_size = parse_obj_size(&cli.obj_size).map_err(|e| anyhow::anyhow!(e))?;

    let sse = if cli.encrypt {
        SseConfig::random_ssec()
    } else if cli.sse_s3_encrypt {
        SseConfig::SseS3
    } else {
        SseConfig::None
    };

    let bc = BenchConfig {
        s3_config,
        bucket: cli.bucket.clone(),
        region: cli.region.clone(),
        concurrency: cli.concurrent,
        duration,
        obj_size,
        objects: cli.objects,
        clear: !cli.noclear,
        autoterm: cli.autoterm,
        autoterm_dur: parse_duration(&cli.autoterm_dur).map_err(|e| anyhow::anyhow!(e))?,
        autoterm_pct: cli.autoterm_pct / 100.0,
        output: cli.benchdata.as_ref().map(|p| p.to_string_lossy().to_string()),
        host_select,
        hosts,
        no_prefix: cli.noprefix,
        prefix: cli.prefix.clone(),
        sse,
        rps_limit: cli.rps_limit,
    };

    if let Some(bench_sub) = cli::distributed_bench_subcommand(&cli.command) {
        if cli::run_as_coordinator_if_requested(&cli, bench_sub).await? {
            return Ok(());
        }
    }

    match cli.command {
        Commands::Mixed {
            get_distrib,
            stat_distrib,
            put_distrib,
            delete_distrib,
            versions: _ver,
        } => {
            run_mixed(&bc, get_distrib, stat_distrib, put_distrib, delete_distrib).await?;
        }

        Commands::Get {
            versions,
            range,
            list_existing,
        } => {
            let range_bounds = range
                .as_deref()
                .and_then(|r| {
                    let parts: Vec<&str> = r.split('-').collect();
                    if parts.len() == 2 {
                        Some((
                            parts[0].parse().unwrap_or(0),
                            parts[1].parse().unwrap_or(0),
                        ))
                    } else {
                        None
                    }
                });
            run_get(&bc, versions, range_bounds, list_existing).await?;
        }

        Commands::Put {
            md5,
            checksum,
            post,
        } => {
            run_put(&bc, md5, checksum, post).await?;
        }

        Commands::Delete { batch } => {
            run_delete(&bc, batch).await?;
        }

        Commands::List { versions } => {
            run_list(&bc, versions).await?;
        }

        Commands::Stat {} => {
            run_stat(&bc).await?;
        }

        Commands::Versioned {
            get_distrib,
            stat_distrib,
            put_distrib,
            delete_distrib,
        } => {
            cli::run_versioned(&bc, get_distrib, stat_distrib, put_distrib, delete_distrib).await?;
        }

        Commands::Retention {} => {
            cli::run_retention(&bc, 5).await?;
        }

        Commands::Multipart {
            part_size,
            parts,
            obj_name,
        } => {
            let ps = parse_size(&part_size).map_err(|e| anyhow::anyhow!(e))? as usize;
            cli::run_multipart(&bc, ps, parts, obj_name).await?;
        }

        Commands::MultipartPut {
            parts,
            part_size,
            part_concurrent,
        } => {
            let ps = parse_size(&part_size).map_err(|e| anyhow::anyhow!(e))? as usize;
            cli::run_multipart_put(&bc, parts, ps, part_concurrent).await?;
        }

        Commands::Snowball { objs_per } => {
            cli::run_snowball(&bc, objs_per).await?;
        }

        Commands::Fanout { copies } => {
            cli::run_fanout(&bc, copies).await?;
        }

        Commands::Append {} => {
            cli::run_append(&bc).await?;
        }

        Commands::Run { config } => {
            let cfg = config::RunFileConfig::load(&config.to_string_lossy())
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            execute_run_yaml(cfg).await?;
        }

        Commands::Zip { entries } => {
            run_zip(&bc, entries).await?;
        }

        Commands::Analyze { file } => {
            cli::analyze_file(&file.to_string_lossy())?;
        }

        Commands::Cmp { before, after } => {
            cli::compare_files(&before.to_string_lossy(), &after.to_string_lossy())?;
        }

        Commands::Merge { files } => {
            let paths: Vec<String> = files.iter().map(|f| f.to_string_lossy().to_string()).collect();
            cli::merge_files(&paths)?;
        }

        Commands::Client { listen_addr } => {
            client::run_client(&listen_addr).await.map_err(|e| anyhow::anyhow!("{e}"))?;
        }

        Commands::Iceberg(sub) => match sub {
            cli::app::IcebergCommand::CatalogRead {
                ns_list_distrib: _, ns_head_distrib: _, ns_get_distrib: _,
                table_list_distrib: _, table_head_distrib: _, table_get_distrib: _,
                view_list_distrib: _, view_head_distrib: _, view_get_distrib: _,
                page_size, namespace_width, namespace_depth, tables_per_ns, views_per_ns,
                columns, properties, base_location, external_catalog, catalog_name,
            } => {
                cli::run_iceberg_read(
                    &bc, cli.remote_hosts.clone(),
                    page_size, namespace_width, namespace_depth,
                    tables_per_ns, views_per_ns, columns, properties,
                    base_location, external_catalog, catalog_name,
                ).await?;
            }
            cli::app::IcebergCommand::CatalogCommits {
                table_commits_throughput, view_commits_throughput,
                max_retries, retry_backoff_ms, backoff_max_ms,
                namespace_width, namespace_depth, tables_per_ns, views_per_ns,
                columns, properties, base_location, external_catalog, catalog_name,
            } => {
                cli::run_iceberg_commits(
                    &bc, cli.remote_hosts.clone(),
                    table_commits_throughput, view_commits_throughput,
                    max_retries, retry_backoff_ms, backoff_max_ms,
                    namespace_width, namespace_depth, tables_per_ns, views_per_ns,
                    columns, properties, base_location, external_catalog, catalog_name,
                ).await?;
            }
            cli::app::IcebergCommand::CatalogMixed {
                ns_list_distrib: _, ns_head_distrib: _, ns_get_distrib: _,
                table_list_distrib: _, table_head_distrib: _, table_get_distrib: _,
                view_list_distrib: _, view_head_distrib: _, view_get_distrib: _,
                ns_update_distrib: _, table_update_distrib: _, view_update_distrib: _,
                max_retries, retry_backoff_ms, backoff_max_ms, page_size,
                namespace_width, namespace_depth, tables_per_ns, views_per_ns,
                columns, properties, base_location, external_catalog, catalog_name,
            } => {
                cli::run_iceberg_mixed(
                    &bc, cli.remote_hosts.clone(),
                    max_retries, retry_backoff_ms, backoff_max_ms, page_size,
                    namespace_width, namespace_depth, tables_per_ns, views_per_ns,
                    columns, properties, base_location, external_catalog, catalog_name,
                ).await?;
            }
            cli::app::IcebergCommand::CatalogWrite { page_size } => {
                cli::run_iceberg_catalog_write_stub(page_size).await?;
            }
            cli::app::IcebergCommand::Sustained {
                num_files, rows_per_file, files_per_commit, tpcds, scale_factor,
                tpcds_table, cache_dir, skip_upload, simulate_read,
                read_concurrent, read_rps_limit,
                max_retries, retry_backoff_ms, backoff_max_ms,
                namespace_width, namespace_depth, tables_per_ns,
                columns, properties, base_location, external_catalog, catalog_name,
                ..
            } => {
                cli::run_iceberg_sustained(
                    &bc, cli.remote_hosts.clone(),
                    num_files, rows_per_file, files_per_commit, tpcds, scale_factor,
                    tpcds_table, cache_dir, skip_upload, simulate_read,
                    read_concurrent, read_rps_limit,
                    max_retries, retry_backoff_ms, backoff_max_ms,
                    namespace_width, namespace_depth, tables_per_ns,
                    columns, properties, base_location, external_catalog, catalog_name,
                ).await?;
            }
        },
    }

    Ok(())
}
