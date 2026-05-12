//! minio-rs server binary entry point
//!
//! Initialises tracing, blocks signals (for signalfd on Linux), parses CLI
//! arguments via clap, and hands control to `server::run()`.

use clap::Parser;
use minio_rs::server::cmd;
use minio_rs::server::run as srv;
use minio_rs::server::signal;
use minio_rs::server::ServerConfig;
use tracing_subscriber::EnvFilter;

fn main() {
    // Block SIGTERM/SIGINT before tokio spawns worker threads (Linux signalfd).
    // Must happen before the runtime is built so worker threads inherit the mask.
    signal::block_signals();

    // Initialise structured logging with non-blocking, buffered writer.
    // Events are pushed into a lock-free channel and flushed by a background
    // worker, so log calls never block the caller.
    let (non_blocking, _guard) = tracing_appender::non_blocking(std::io::stderr());
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(true)
        .with_file(true)
        .with_line_number(true)
        .with_writer(non_blocking)
        .init();

    // Build runtime manually so signal mask is in place before threads spawn
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");

    rt.block_on(async {
        // Parse CLI
        let cli = cmd::Cli::parse();
        let cmd::Commands::Server {
            address,
            console_address,
            disks,
        } = cli.command;

        // Build config
        let config = ServerConfig {
            address,
            console_address,
            disks,
        };

        // Start server; exit with code 1 on error
        if let Err(e) = srv::run(config).await {
            tracing::error!("server exited with error: {e}");
            std::process::exit(1);
        }
    });
}
