use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Connection handshake ─────────────────────────────────

/// Sent by the connecting peer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectReq {
    pub id: [u8; 16],
    pub host: String,
    /// Seconds since Unix epoch (f64 for msgpack compat).
    pub time: f64,
    pub token: String,
    /// Monotonic per-connection nonce; `0` = legacy client (only wall-clock skew check, weaker replay resistance).
    #[serde(default)]
    pub nonce: u64,
}

/// Sent back by the accepting peer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectResp {
    pub id: [u8; 16],
    pub accepted: bool,
    pub rejected_reason: String,
}

/// Sent when a mux connection fails.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MuxConnectError {
    pub error: String,
}

// ── Heartbeat ────────────────────────────────────────────

/// Pong response to a Ping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PongMsg {
    #[serde(rename = "nf")]
    pub not_found: bool,
    #[serde(rename = "e")]
    pub err: Option<String>,
    /// Seconds since Unix epoch.
    #[serde(rename = "t")]
    pub t: f64,
}

// ── Helper types used by handlers ─────────────────────────

/// Map<string, string> — commonly used for key-value metadata payloads.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MSS(pub HashMap<String, String>);

impl MSS {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn with(map: HashMap<String, String>) -> Self {
        Self(map)
    }

    pub fn with_entries(entries: impl IntoIterator<Item = (String, String)>) -> Self {
        Self(entries.into_iter().collect())
    }

    pub fn get(&self, key: &str) -> Option<&String> {
        self.0.get(key)
    }

    pub fn insert(&mut self, key: String, value: String) -> Option<String> {
        self.0.insert(key, value)
    }
}

impl std::ops::Deref for MSS {
    type Target = HashMap<String, String>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for MSS {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Byte payload wrapper for typed handlers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Bytes(#[serde(with = "serde_bytes")] pub Vec<u8>);

impl Bytes {
    pub fn new(data: Vec<u8>) -> Self {
        Self(data)
    }

    pub fn into_inner(self) -> Vec<u8> {
        self.0
    }
}

impl std::ops::Deref for Bytes {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Vec<u8>> for Bytes {
    fn from(v: Vec<u8>) -> Self {
        Self(v)
    }
}

impl From<Bytes> for Vec<u8> {
    fn from(b: Bytes) -> Self {
        b.0
    }
}

// ── Test types (used in integration tests) ────────────────

/// Test request type for grid round-trip tests.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TestRequest {
    #[serde(rename = "orgNum")]
    pub org_num: u64,
    #[serde(rename = "orgString")]
    pub org_string: String,
}

/// Test response type for grid round-trip tests.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TestResponse {
    #[serde(rename = "orgNum")]
    pub org_num: u64,
    #[serde(rename = "orgString")]
    pub org_string: String,
    #[serde(rename = "embedded")]
    pub embedded: Option<TestRequest>,
}
