//! Namespace lock — per-object reader/writer mutual exclusion.
//!
//! # Architecture
//!
//! ```text
//! NsLockMap (public entry point, shared via Arc)
//!   └── LockProvider (internal dispatch)
//!         ├── LocalLockProvider    — in-process, Phase 1
//!         └── DistLockProvider    — grid RPC, Phase 2 (stub)
//! ```
//!
//! ## Local provider
//!
//! One `HashMap<resource, Arc<ResourceLock>>` protected by `std::sync::Mutex`.
//! Each `ResourceLock` is a reader/writer mutex with writer fairness (pending
//! writers gate new readers to prevent starvation).  Lock acquire uses
//! `tokio::sync::Notify` for async waiting; unlock is synchronous so it can be
//! called from `Drop`.
//!
//! Reference counting: entries are reaped from the map when all guards for that
//! resource have been dropped AND no waiters remain.
//!
//! ## Distributed provider (Phase 2)
//!
//! Will integrate with the grid RPC framework:
//! - Quorum: N/2+1 for write lock, N/2 for read lock
//! - Parallel Lock/RLock RPC broadcast to all nodes
//! - Background Refresh goroutine every 10s
//! - Force-unlock on quorum loss → cancels inflight operation
//! - Async unlock via parallel Unlock RPCs

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Notify;

// ── Public types ──────────────────────────────────────────────

/// Factory for per-object namespace locks.
///
/// Shared across all S3 operations via `Arc<NsLockMap>`.
/// Dispatches to the appropriate provider (local or distributed).
pub struct NsLockMap {
    provider: LockProvider,
}

/// RAII guard — releases the lock when dropped.
#[must_use = "lock guard is dropped immediately if not bound"]
pub struct NsLockGuard {
    resource: String,
    kind: LockKind,
    map: Arc<NsLockMap>,
}

// ── Internal enums ───────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LockKind {
    Read,
    Write,
}

enum LockProvider {
    Local(LocalLockProvider),
    #[allow(dead_code)]
    Distributed(DistLockProvider),
}

// ── NsLockMap (public API) ───────────────────────────────────

impl NsLockMap {
    /// Create a local (in-process) lock provider.
    pub fn new() -> Self {
        Self {
            provider: LockProvider::Local(LocalLockProvider::new()),
        }
    }

    /// Create a distributed lock provider (Phase 2 — not yet implemented).
    #[allow(dead_code)]
    pub fn new_distributed() -> Self {
        Self {
            provider: LockProvider::Distributed(DistLockProvider::new()),
        }
    }

    /// Acquire an exclusive write lock on `resource`.
    pub async fn lock(self: &Arc<Self>, resource: &str) -> NsLockGuard {
        self.provider.lock(resource).await;
        NsLockGuard {
            resource: resource.to_string(),
            kind: LockKind::Write,
            map: Arc::clone(self),
        }
    }

    /// Acquire a shared read lock on `resource`.
    ///
    /// Writer-fair: pending writers gate new readers to prevent starvation.
    pub async fn rlock(self: &Arc<Self>, resource: &str) -> NsLockGuard {
        self.provider.rlock(resource).await;
        NsLockGuard {
            resource: resource.to_string(),
            kind: LockKind::Read,
            map: Arc::clone(self),
        }
    }

    /// Release a lock. Called from `NsLockGuard::drop`.
    fn unlock(&self, resource: &str, kind: LockKind) {
        self.provider.unlock(resource, kind);
    }
}

impl Default for NsLockMap {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for NsLockGuard {
    fn drop(&mut self) {
        self.map.unlock(&self.resource, self.kind);
    }
}

// ── LockProvider dispatch ─────────────────────────────────────

impl LockProvider {
    async fn lock(&self, resource: &str) {
        match self {
            Self::Local(p) => p.lock(resource).await,
            Self::Distributed(p) => p.lock(resource).await,
        }
    }

    async fn rlock(&self, resource: &str) {
        match self {
            Self::Local(p) => p.rlock(resource).await,
            Self::Distributed(p) => p.rlock(resource).await,
        }
    }

    fn unlock(&self, resource: &str, kind: LockKind) {
        match self {
            Self::Local(p) => p.unlock(resource, kind),
            Self::Distributed(p) => p.unlock(resource, kind),
        }
    }
}

// ── Local lock provider ──────────────────────────────────────

/// In-process lock provider.
///
/// One `ResourceLock` per unique resource string, reference-counted.
/// Reaped from the map when idle (no locks held, no waiters).
struct LocalLockProvider {
    locks: Mutex<HashMap<String, Arc<ResourceLock>>>,
}

struct ResourceLock {
    state: Mutex<ResourceState>,
    notify: Notify,
}

#[derive(Debug)]
struct ResourceState {
    readers: usize,
    write_held: bool,
    pending_writers: usize,
    /// Number of active guards referencing this resource.
    refs: usize,
}

impl LocalLockProvider {
    fn new() -> Self {
        Self {
            locks: Mutex::new(HashMap::new()),
        }
    }

    async fn lock(&self, resource: &str) {
        let entry = self.get_or_create(resource);
        loop {
            let notified = {
                let mut state = entry.state.lock().unwrap();
                if !state.write_held && state.readers == 0 {
                    state.write_held = true;
                    state.refs += 1;
                    return;
                }
                state.pending_writers += 1;
                entry.notify.notified()
            };
            notified.await;
        }
    }

    async fn rlock(&self, resource: &str) {
        let entry = self.get_or_create(resource);
        loop {
            let notified = {
                let mut state = entry.state.lock().unwrap();
                if !state.write_held && state.pending_writers == 0 {
                    state.readers += 1;
                    state.refs += 1;
                    return;
                }
                entry.notify.notified()
            };
            notified.await;
        }
    }

    fn unlock(&self, resource: &str, kind: LockKind) {
        let mut locks = match self.locks.lock() {
            Ok(l) => l,
            Err(_) => return,
        };
        let entry = match locks.get(resource) {
            Some(e) => Arc::clone(e),
            None => return,
        };
        {
            let mut state = entry.state.lock().unwrap();
            match kind {
                LockKind::Write => state.write_held = false,
                LockKind::Read => state.readers = state.readers.saturating_sub(1),
            }
            state.refs = state.refs.saturating_sub(1);
        }
        // Wake waiters so they can re-check conditions
        entry.notify.notify_waiters();
        // Reap idle entries
        {
            let state = entry.state.lock().unwrap();
            if state.refs == 0 && !state.write_held && state.pending_writers == 0 {
                locks.remove(resource);
            }
        }
    }

    fn get_or_create(&self, resource: &str) -> Arc<ResourceLock> {
        let mut locks = self.locks.lock().unwrap();
        locks
            .entry(resource.to_string())
            .or_insert_with(|| {
                Arc::new(ResourceLock {
                    state: Mutex::new(ResourceState {
                        readers: 0,
                        write_held: false,
                        pending_writers: 0,
                        refs: 0,
                    }),
                    notify: Notify::new(),
                })
            })
            .clone()
    }
}

// ── Distributed lock provider (Phase 2 stub) ─────────────────

/// Distributed lock provider.
///
/// Phase 2 will integrate with the grid RPC framework (`src/grid/`):
///
/// - `lock()` / `rlock()` → parallel RPC broadcast to all nodes
/// - Quorum check: N/2+1 for write, N/2 for read
/// - Background Refresh goroutine (10s interval)
/// - Force-unlock on quorum loss with context cancellation
/// - Async unlock via parallel Unlock RPCs
///
/// Reference: MinIO Go `dsync.DRWMutex` (drwmutex.go:112-123)
struct DistLockProvider {
    /// Timeout for individual lock RPC calls.
    #[allow(dead_code)]
    timeout: Duration,
}

impl DistLockProvider {
    fn new() -> Self {
        Self {
            timeout: Duration::from_secs(30),
        }
    }

    // TODO:
    async fn lock(&self, _resource: &str) {
        unimplemented!("distributed lock (Phase 2): requires grid RPC framework");
    }

    // TODO:
    async fn rlock(&self, _resource: &str) {
        unimplemented!("distributed read lock (Phase 2): requires grid RPC framework");
    }

    // TODO:
    fn unlock(&self, _resource: &str, _kind: LockKind) {
        unimplemented!("distributed unlock (Phase 2): requires grid RPC framework");
    }
}

// ── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn test_write_lock_exclusive() {
        let map = Arc::new(NsLockMap::new());
        let counter = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for _ in 0..10 {
            let map = Arc::clone(&map);
            let counter = Arc::clone(&counter);
            handles.push(tokio::spawn(async move {
                let _guard = map.lock("obj").await;
                let v = counter.load(Ordering::SeqCst);
                tokio::time::sleep(Duration::from_micros(100)).await;
                counter.store(v + 1, Ordering::SeqCst);
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        assert_eq!(counter.load(Ordering::SeqCst), 10);
    }

    #[tokio::test]
    async fn test_read_locks_concurrent() {
        let map = Arc::new(NsLockMap::new());
        let _r1 = map.rlock("obj").await;
        let _r2 = map.rlock("obj").await;
        // Two readers coexist without deadlock
    }

    #[tokio::test]
    async fn test_write_blocks_read() {
        let map = Arc::new(NsLockMap::new());
        let ready = Arc::new(AtomicUsize::new(0));

        let _writer = map.lock("obj").await;

        let map2 = Arc::clone(&map);
        let ready2 = Arc::clone(&ready);
        let reader = tokio::spawn(async move {
            ready2.store(1, Ordering::SeqCst);
            let _r = map2.rlock("obj").await;
            ready2.store(2, Ordering::SeqCst);
        });

        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(ready.load(Ordering::SeqCst), 1);
        drop(_writer);
        tokio::time::sleep(Duration::from_millis(50)).await;
        reader.await.unwrap();
        assert_eq!(ready.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_entry_reaped_on_idle() {
        let map = Arc::new(NsLockMap::new());
        {
            let _g = map.lock("obj").await;
        } // drop → unlock → reap
        // After guard is dropped, the entry should be removed.
        // If it's not reaped, the lock count would be wrong on next acquire.
        // Verify by acquiring again (would deadlock if state leaked)
        let _g2 = map.lock("obj").await;
    }
}
