//! CLI argument parsing using clap derive API

use clap::{Parser, Subcommand};

/// minio-rs: High-performance S3-compatible object storage
#[derive(Parser)]
#[command(name = "minio", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start the object storage server
    Server {
        /// HTTP listening address
        #[arg(short, long, default_value = "0.0.0.0:9000")]
        address: String,

        /// Console (web UI) listening address (Phase 1: informational only)
        #[arg(short = 'C', long = "console-address")]
        console_address: Option<String>,

        /// Disk paths (at least 1 for dev/test, >=3 for production)
        #[arg(required = true)]
        disks: Vec<String>,
    },
}
