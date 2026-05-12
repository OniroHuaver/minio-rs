//! ANSI terminal HUD updated every ~500 ms from an async Tokio task.

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;

/// Shared HUD counters (cheap atomics on the bench hot-path).
pub struct TuiState {
    pub phase: Mutex<String>,
    pub phase_detail: Mutex<String>,
    pub progress: Mutex<Option<f64>>,
    pub start_time: Instant,

    pub total_bytes: AtomicU64,
    pub total_ops: AtomicU64,
    pub total_errors: AtomicU64,
    pub done: AtomicBool,

    pub cancel: CancellationToken,
}

impl TuiState {
    pub fn new(cancel: CancellationToken) -> Self {
        Self {
            phase: Mutex::new(String::new()),
            phase_detail: Mutex::new(String::new()),
            progress: Mutex::new(None),
            start_time: Instant::now(),

            total_bytes: AtomicU64::new(0),
            total_ops: AtomicU64::new(0),
            total_errors: AtomicU64::new(0),
            done: AtomicBool::new(false),

            cancel,
        }
    }

    pub fn set_phase(&self, phase: &str, detail: &str) {
        *self.phase.lock().expect("lock poisoned") = phase.to_string();
        *self.phase_detail.lock().expect("lock poisoned") = detail.to_string();
    }

    pub fn set_progress(&self, progress: f64) {
        *self.progress.lock().expect("lock poisoned") = Some(progress.clamp(0.0, 1.0));
    }

    #[allow(dead_code)]
    pub fn record_op(&self, bytes: i64, success: bool) {
        if bytes > 0 {
            self.total_bytes.fetch_add(bytes as u64, Ordering::Relaxed);
        }
        self.total_ops.fetch_add(1, Ordering::Relaxed);
        if !success {
            self.total_errors.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn set_done(&self) {
        self.done.store(true, Ordering::Relaxed);
    }

    pub fn spawn_render_loop(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut prev_bytes = 0u64;
            let mut prev_ops = 0u64;

            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            loop {
                tokio::select! {
                    _ = self.cancel.cancelled() => break,
                    _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {}
                }

                if self.done.load(Ordering::Relaxed) {
                    break;
                }

                let bytes = self.total_bytes.load(Ordering::Relaxed);
                let ops = self.total_ops.load(Ordering::Relaxed);
                let errors = self.total_errors.load(Ordering::Relaxed);

                let mbps = (bytes.saturating_sub(prev_bytes)) as f64 / (1024.0 * 1024.0) / 0.5;
                let ops_per_sec = (ops.saturating_sub(prev_ops)) as f64 / 0.5;

                prev_bytes = bytes;
                prev_ops = ops;

                let phase = self.phase.lock().expect("lock poisoned").clone();
                let detail = self.phase_detail.lock().expect("lock poisoned").clone();
                let progress = *self.progress.lock().expect("lock poisoned");

                render_frame(
                    &phase,
                    &detail,
                    progress,
                    self.start_time.elapsed(),
                    bytes,
                    errors,
                    ops,
                    mbps,
                    ops_per_sec,
                );
            }

            let bytes = self.total_bytes.load(Ordering::Relaxed);
            let ops = self.total_ops.load(Ordering::Relaxed);
            let errors = self.total_errors.load(Ordering::Relaxed);
            let phase = self.phase.lock().expect("lock poisoned").clone();
            let detail = self.phase_detail.lock().expect("lock poisoned").clone();
            let progress = *self.progress.lock().expect("lock poisoned");

            let mbps = (bytes.saturating_sub(prev_bytes)) as f64 / (1024.0 * 1024.0) / 0.5;
            let ops_per_sec = (ops.saturating_sub(prev_ops)) as f64 / 0.5;

            render_frame(
                &phase,
                &detail,
                progress,
                self.start_time.elapsed(),
                bytes,
                errors,
                ops,
                mbps,
                ops_per_sec,
            );
        });
    }
}

fn render_frame(
    phase: &str,
    detail: &str,
    progress: Option<f64>,
    elapsed: Duration,
    total_bytes: u64,
    total_errors: u64,
    total_ops: u64,
    mbps: f64,
    ops_per_sec: f64,
) {
    print!("\x1b[2J\x1b[H");

    let total_gib = total_bytes as f64 / (1024.0 * 1024.0 * 1024.0);

    println!("+--------------------------------+");
    println!("| s3perf S3 benchmark            |");
    println!("+--------------------------------+");

    let phase_display = if phase.len() > 28 {
        format!("{}...", &phase[..25])
    } else {
        phase.to_string()
    };
    println!("| Phase: {:<23}|", phase_display);
    println!("| Elapsed: {:<20.1}s|", elapsed.as_secs_f64());

    let pct = progress.unwrap_or(0.0);
    let filled = (pct * 20.0).min(20.0).max(0.0) as usize;
    let bar: String = "#".repeat(filled) + &"-".repeat(20usize.saturating_sub(filled));
    println!("| [{}] {:>3.0}%              |", bar, pct * 100.0);

    println!("+--------------------------------+");
    println!("| Throughput:                    |");
    println!("| {:<8.1} MiB/s {:<8.1} obj/s   |", mbps, ops_per_sec);
    println!("| Req: {:<6} Err: {:<6}         |", total_ops, total_errors);
    println!("| Data: {:<6.2} GiB              |", total_gib);

    let detail_display = if detail.len() > 28 {
        format!("{}...", &detail[..25])
    } else {
        detail.to_string()
    };
    if !detail_display.is_empty() {
        println!("| {:<30}|", detail_display);
    }

    println!("+--------------------------------+");

    io::stdout().flush().ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_state() {
        let cancel = CancellationToken::new();
        let state = TuiState::new(cancel.clone());
        assert!(state.phase.lock().expect("lock poisoned").is_empty());
        assert!(state.progress.lock().expect("lock poisoned").is_none());
        assert_eq!(state.total_bytes.load(Ordering::Relaxed), 0);
        assert_eq!(state.total_ops.load(Ordering::Relaxed), 0);
        assert_eq!(state.total_errors.load(Ordering::Relaxed), 0);
        assert!(!state.done.load(Ordering::Relaxed));
    }

    #[test]
    fn test_set_phase() {
        let state = TuiState::new(CancellationToken::new());
        state.set_phase("Benchmarking", "GET /bucket/key");
        assert_eq!(*state.phase.lock().expect("lock poisoned"), "Benchmarking");
        assert_eq!(*state.phase_detail.lock().expect("lock poisoned"), "GET /bucket/key");
    }

    #[test]
    fn test_set_progress() {
        let state = TuiState::new(CancellationToken::new());
        state.set_progress(0.5);
        assert_eq!(*state.progress.lock().expect("lock poisoned"), Some(0.5));
    }

    #[test]
    fn test_set_progress_clamp() {
        let state = TuiState::new(CancellationToken::new());
        state.set_progress(1.5);
        assert_eq!(*state.progress.lock().expect("lock poisoned"), Some(1.0));
        state.set_progress(-0.1);
        assert_eq!(*state.progress.lock().expect("lock poisoned"), Some(0.0));
    }

    #[test]
    fn test_record_op_success() {
        let state = TuiState::new(CancellationToken::new());
        state.record_op(4096, true);
        assert_eq!(state.total_bytes.load(Ordering::Relaxed), 4096);
        assert_eq!(state.total_ops.load(Ordering::Relaxed), 1);
        assert_eq!(state.total_errors.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_record_op_failure() {
        let state = TuiState::new(CancellationToken::new());
        state.record_op(0, false);
        assert_eq!(state.total_bytes.load(Ordering::Relaxed), 0);
        assert_eq!(state.total_ops.load(Ordering::Relaxed), 1);
        assert_eq!(state.total_errors.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_record_op_negative_bytes() {
        let state = TuiState::new(CancellationToken::new());
        state.record_op(-1, true);
        assert_eq!(state.total_bytes.load(Ordering::Relaxed), 0);
        assert_eq!(state.total_ops.load(Ordering::Relaxed), 1);
        assert_eq!(state.total_errors.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_set_done() {
        let state = TuiState::new(CancellationToken::new());
        assert!(!state.done.load(Ordering::Relaxed));
        state.set_done();
        assert!(state.done.load(Ordering::Relaxed));
    }

    #[test]
    fn test_cancel_token_propagated() {
        let cancel = CancellationToken::new();
        let state = TuiState::new(cancel.clone());
        assert!(!cancel.is_cancelled());
        cancel.cancel();
        assert!(cancel.is_cancelled());
        drop(state);
    }
}
