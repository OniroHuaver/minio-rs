//! JSON types for distributed coordinator / agent control plane.

use crate::bench::Operation;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const COORD_PROTOCOL_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// ServerInfo
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub id: String,
    pub secret: String,
    pub version: u32,
}

impl ServerInfo {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty() {
            return Err("no server id sent".into());
        }
        if self.version != COORD_PROTOCOL_VERSION {
            return Err(format!(
                "version mismatch: server={}, client={}",
                self.version, COORD_PROTOCOL_VERSION
            ));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Server → Client
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerRequestOp {
    Disconnect,
    Benchmark,
    StartStage,
    StageStatus,
    StageAbort,
    SendOps,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkConfig {
    pub command: String,
    pub flags: HashMap<String, String>,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerRequest {
    pub op: ServerRequestOp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub benchmark: Option<BenchmarkConfig>,
    pub stage: String,       // "prepare","benchmark","cleanup","done"
    pub client_idx: usize,
    pub total_clients: usize,
    pub start_time: DateTime<Utc>,
    pub aggregate: bool,
}

// ---------------------------------------------------------------------------
// Client → Server
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientReplyType {
    BenchmarkStarted,
    BenchmarkStatus,
    AbortRequested,
    Ops,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom: Option<HashMap<String, String>>,
    pub progress: f64,
    pub started: bool,
    pub finished: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientReply {
    pub time: DateTime<Utc>,
    #[serde(rename = "stage_info")]
    pub stage_info: StageInfo,
    #[serde(rename = "type")]
    pub reply_type: ClientReplyType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ops: Option<Vec<Operation>>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub fn parse_hosts(input: &str) -> Vec<String> {
    input
        .split(',')
        .flat_map(|s| {
            let s = s.trim();
            if s.is_empty() {
                return vec![];
            }
            expand_brace(s)
        })
        .collect()
}

fn expand_brace(s: &str) -> Vec<String> {
    if let Some(start) = s.find('{') {
        if let Some(end) = s[start..].find('}') {
            let end = start + end;
            let range = &s[start + 1..end];
            if let Some(dot_idx) = range.find("...") {
                let lo: usize = range[..dot_idx].parse().unwrap_or(1);
                let hi: usize = range[dot_idx + 3..].parse().unwrap_or(lo);
                let prefix = &s[..start];
                let suffix = &s[end + 1..];
                return (lo..=hi)
                    .map(|i| format!("{prefix}{i}{suffix}"))
                    .collect();
            }
        }
    }
    vec![s.to_string()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hosts() {
        let hosts = parse_hosts("host1:7761,host2:7761");
        assert_eq!(hosts.len(), 2);
    }

    #[test]
    fn test_parse_hosts_brace() {
        let hosts = parse_hosts("host{1...3}:7761");
        assert_eq!(hosts, vec!["host1:7761", "host2:7761", "host3:7761"]);
    }

    #[test]
    fn test_server_info_validate() {
        let info = ServerInfo {
            id: "test".into(),
            secret: "s".into(),
            version: 1,
        };
        assert!(info.validate().is_ok());

        let bad_ver = ServerInfo {
            id: "t".into(),
            secret: "s".into(),
            version: 99,
        };
        assert!(bad_ver.validate().is_err());
    }
}
