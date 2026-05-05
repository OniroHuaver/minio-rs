//! minio-rs server binary entry point
//!
//! Initialises tracing, parses CLI arguments via clap, and hands control to `server::run()`.

use clap::Parser;
use server::cmd;
use server::server as srv;
use server::ServerConfig;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    // Initialise structured logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(true)
        .with_file(true)
        .with_line_number(true)
        .init();

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
}
