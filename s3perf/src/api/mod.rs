//! HTTP monitoring: [`BenchmarkMonitor`] state and optional axum server ([`http`]).

pub mod http;

use crate::aggregate::Aggregated;
use crate::bench::Operation;
use chrono::{DateTime, Utc};
use std::sync::{Arc, Mutex};

/// Thread-safe benchmark monitor (shared via `Arc`/`Mutex` internally).
#[derive(Clone)]
pub struct BenchmarkMonitor {
    state: Arc<Mutex<BenchmarkState>>,
}

/// Internal monitor state.
pub struct BenchmarkState {
    pub start_time: Option<DateTime<Utc>>,
    pub operations: Vec<Operation>,
    pub collecting: bool,
    pub done: bool,
    pub filename: Option<String>,
    pub last_error: String,
    pub last_status: String,
    pub aggregated: Option<Aggregated>,
}

/// Serializable snapshot for `/v1/status`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BenchmarkStatus {
    pub last_status: String,
    pub last_error: String,
    pub filename: Option<String>,
    pub data_ready: bool,
    pub running: bool,
}

impl BenchmarkMonitor {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(BenchmarkState {
                start_time: None,
                operations: Vec::new(),
                collecting: false,
                done: false,
                filename: None,
                last_error: String::new(),
                last_status: "initialized".into(),
                aggregated: None,
            })),
        }
    }

    pub fn start(&self) {
        let mut state = self.state.lock().unwrap();
        state.start_time = Some(Utc::now());
        state.collecting = true;
        state.done = false;
        state.last_status = "running".into();
    }

    pub fn status(&self) -> BenchmarkStatus {
        let state = self.state.lock().unwrap();
        BenchmarkStatus {
            last_status: state.last_status.clone(),
            last_error: state.last_error.clone(),
            filename: state.filename.clone(),
            data_ready: state.aggregated.is_some(),
            running: state.collecting && !state.done,
        }
    }

    pub fn set_done(&self, filename: Option<String>) {
        let mut state = self.state.lock().unwrap();
        state.done = true;
        state.collecting = false;
        state.filename = filename;
        state.last_status = "done".into();
    }

    pub fn set_status(&self, msg: &str) {
        let mut state = self.state.lock().unwrap();
        state.last_status = msg.to_string();
    }

    pub fn set_error(&self, msg: &str) {
        let mut state = self.state.lock().unwrap();
        state.last_error = msg.to_string();
    }

    pub fn set_aggregated(&self, agg: Aggregated) {
        let mut state = self.state.lock().unwrap();
        state.aggregated = Some(agg);
    }

    pub fn add_ops(&self, ops: &[Operation]) {
        let mut state = self.state.lock().unwrap();
        state.operations.extend_from_slice(ops);
    }
}

impl Default for BenchmarkMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::Operation;

    fn dummy_op() -> Operation {
        Operation {
            start: Utc::now(),
            end: Utc::now(),
            first_byte: None,
            last_byte: None,
            op_type: "GET".into(),
            err: String::new(),
            file: "test".into(),
            client_id: "c1".into(),
            endpoint: "http://localhost".into(),
            obj_per_op: 1,
            size: 1024,
            thread: 0,
            categories: 0,
        }
    }

    #[test]
    fn test_new() {
        let m = BenchmarkMonitor::new();
        let s = m.status();
        assert_eq!(s.last_status, "initialized");
        assert!(!s.running);
        assert!(!s.data_ready);
        assert!(s.last_error.is_empty());
        assert!(s.filename.is_none());
    }

    #[test]
    fn test_start_and_status() {
        let m = BenchmarkMonitor::new();
        m.start();
        let s = m.status();
        assert!(s.running);
        assert_eq!(s.last_status, "running");
    }

    #[test]
    fn test_set_done() {
        let m = BenchmarkMonitor::new();
        m.start();
        m.set_done(Some("result.json".into()));
        let s = m.status();
        assert!(!s.running);
        assert!(!s.data_ready);
        assert_eq!(s.filename.as_deref(), Some("result.json"));
    }

    #[test]
    fn test_set_status() {
        let m = BenchmarkMonitor::new();
        m.set_status("preparing");
        assert_eq!(m.status().last_status, "preparing");
    }

    #[test]
    fn test_set_error() {
        let m = BenchmarkMonitor::new();
        m.set_error("something went wrong");
        assert_eq!(m.status().last_error, "something went wrong");
    }

    #[test]
    fn test_add_ops() {
        let m = BenchmarkMonitor::new();
        let op = dummy_op();
        m.add_ops(&[op]);
        let state = m.state.lock().unwrap();
        assert_eq!(state.operations.len(), 1);
        assert_eq!(state.operations[0].op_type, "GET");
    }

    #[test]
    fn test_set_aggregated() {
        let m = BenchmarkMonitor::new();
        let agg = Aggregated {
            mixed: false,
            operations: Vec::new(),
            mixed_server_stats: None,
            mixed_throughput_by_host: std::collections::HashMap::new(),
        };
        m.set_aggregated(agg);
        let s = m.status();
        assert!(s.data_ready);
    }

    #[test]
    fn test_clone_shares_state() {
        let m1 = BenchmarkMonitor::new();
        let m2 = m1.clone();
        m1.set_status("shared");
        assert_eq!(m2.status().last_status, "shared");
    }

    #[test]
    fn test_default() {
        let m = BenchmarkMonitor::default();
        assert_eq!(m.status().last_status, "initialized");
    }
}
