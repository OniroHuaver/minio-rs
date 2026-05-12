//! s3perf — High-performance S3 benchmarking library
//!
//! Re-exports the public API surface for programmatic use by other crates
//! (e.g., MCP server).

pub mod aggregate;
pub mod api;
pub mod bench;
pub mod cli;
pub mod client;
pub mod config;
pub mod generator;
pub mod iceberg;
pub mod influxdb;
pub mod server;
pub mod tui;

// Public API re-exports
pub use aggregate::Aggregated;
pub use bench::s3_client::S3Config;
pub use bench::sse::SseConfig;
pub use bench::HostSelect;
pub use cli::app::{parse_duration, parse_obj_size, parse_size};
pub use cli::BenchConfig;
pub use cli::runner::{
    execute_run_yaml, run_append, run_benchmark, run_delete, run_fanout, run_get, run_list,
    run_mixed, run_multipart, run_multipart_put, run_put, run_retention, run_snowball, run_stat,
    run_versioned, run_zip,
};
pub use generator::ObjSize;
