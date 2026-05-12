//! Distributed benchmarking coordinator (`--remote-hosts`) built on JSON over WebSockets.

use crate::server::protocol::*;
use crate::server::ws_client::{connect_with_retry, ClientWs};
use chrono::{Duration, Utc};
use std::collections::HashMap;
use tracing::{error, info};

pub mod protocol;
pub mod ws_client;

// ---------------------------------------------------------------------------
// Connections
// ---------------------------------------------------------------------------
pub struct Connections {
    pub hosts: Vec<String>,
    conns: Vec<ClientWs>,
    server_info: ServerInfo,
}

impl Connections {
    pub fn new(hosts: Vec<String>) -> Self {
        let server_info = ServerInfo {
            id: uuid::Uuid::new_v4().to_string(),
            secret: uuid::Uuid::new_v4().to_string(),
            version: COORD_PROTOCOL_VERSION,
        };
        Self {
            hosts,
            conns: Vec::new(),
            server_info,
        }
    }

    /// Establish WebSocket connections to every remote agent concurrently.
    pub async fn connect_all(&mut self) -> Result<(), String> {
        let mut conns = Vec::new();
        for host in &self.hosts {
            let ws = connect_with_retry(host, &self.server_info, 3).await?;
            info!("connected remote agent {}", host);
            conns.push(ws);
        }
        self.conns = conns;
        Ok(())
    }

    pub async fn round_trip(
        &mut self,
        client_idx: usize,
        req: &ServerRequest,
    ) -> Result<ClientReply, String> {
        self.conns[client_idx].round_trip(req).await
    }

    pub async fn close_all(&mut self) {
        for conn in self.conns.drain(..) {
            let _ = conn.close().await;
        }
    }

    pub fn len(&self) -> usize {
        self.conns.len()
    }
}

// ---------------------------------------------------------------------------
// Coordinator driver
// ---------------------------------------------------------------------------

pub async fn run_server_benchmark(
    client_hosts: &[String],
    command_name: &str,
    flags: HashMap<String, String>,
    args: Vec<String>,
) -> Result<(), String> {
    if client_hosts.is_empty() {
        return Err("no remote benchmarking agents specified".into());
    }

    let hosts: Vec<_> = client_hosts
        .iter()
        .flat_map(|h| parse_hosts(h))
        .collect();

    if hosts.is_empty() {
        return Err("parsed hosts is empty".into());
    }

    info!("distributed benchmark: {} across {} agents", command_name, hosts.len());

    let mut conns = Connections::new(hosts);
    conns.connect_all().await?;

    let total = conns.len();

    // Dispatch benchmark payloads
    {
        let start_time = Utc::now() + Duration::seconds(2);
        for i in 0..conns.len() {
            let req = ServerRequest {
                op: ServerRequestOp::Benchmark,
                benchmark: Some(BenchmarkConfig {
                    command: command_name.to_string(),
                    flags: flags.clone(),
                    args: args.clone(),
                }),
                stage: "prepare".into(),
                client_idx: i,
                total_clients: total,
                start_time,
                aggregate: false,
            };
            conns.round_trip(i, &req).await?;
        }
        info!("benchmark configuration replicated to {} agents", total);
    }

    for stage in &["prepare", "benchmark", "cleanup"] {
        info!("stage: {stage}");
        let start_time = Utc::now() + Duration::seconds(2);

        for i in 0..conns.len() {
            let req = ServerRequest {
                op: ServerRequestOp::StartStage,
                benchmark: None,
                stage: stage.to_string(),
                client_idx: i,
                total_clients: total,
                start_time,
                aggregate: false,
            };
            let _ = conns.round_trip(i, &req).await;
        }

        let mut done = vec![false; conns.len()];
        loop {
            if done.iter().all(|&d| d) {
                break;
            }
            for i in 0..conns.len() {
                if done[i] {
                    continue;
                }
                let req = ServerRequest {
                    op: ServerRequestOp::StageStatus,
                    benchmark: None,
                    stage: stage.to_string(),
                    client_idx: i,
                    total_clients: total,
                    start_time: Utc::now(),
                    aggregate: false,
                };
                match conns.round_trip(i, &req).await {
                    Ok(reply) => {
                        if reply.stage_info.finished {
                            done[i] = true;
                        }
                    }
                    Err(e) => error!("agent {i} error: {e}"),
                }
            }
            if !done.iter().all(|&d| d) {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
    }

    {
        let start_time = Utc::now();
        for i in 0..conns.len() {
            let req = ServerRequest {
                op: ServerRequestOp::Disconnect,
                benchmark: None,
                stage: "done".into(),
                client_idx: i,
                total_clients: total,
                start_time,
                aggregate: false,
            };
            let _ = conns.round_trip(i, &req).await;
        }
    }

    conns.close_all().await;
    info!("distributed benchmark finished");
    Ok(())
}
