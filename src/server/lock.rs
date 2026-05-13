// `libc::kill` for stale PID check — confined to this module.
#![allow(unsafe_code)]

//! Instance file lock — ensures only one server process runs per directory.
//!
//! Uses `flock` (via `fs2`) on `.minio.lock` in the current working directory.
//! The lock is OS-level and auto-released when the process exits, so a crash
//! won't leave a stale lock behind.  As a belt-and-suspenders measure, we also
//! write the PID into the file and check whether a lock-holder is still alive
//! before reporting a conflict.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Duration;

use fs2::FileExt;

use crate::base::error::{MinioError, MinioResult};

/// RAII guard that holds the exclusive instance lock.
///
/// On drop, removes the lock file (best-effort — a warning is logged on failure,
/// the kernel already released the `flock` via fd close).
pub struct InstanceLock {
    _file: File,
    path: PathBuf,
}

impl Drop for InstanceLock {
    fn drop(&mut self) {
        if let Err(e) = fs::remove_file(&self.path) {
            tracing::warn!(
                "failed to remove lock file {}: {e}",
                self.path.display()
            );
        }
    }
}

/// Try to acquire the exclusive instance lock.
///
/// Creates `.minio.lock` in the current working directory, acquires an
/// exclusive OS-level advisory lock on it, and writes this process's PID.
///
/// If the lock is held by a dead process (stale lock), it is broken and
/// re-acquired automatically.
pub fn acquire() -> MinioResult<InstanceLock> {
    /// Max attempts to remove a stale lock before giving up (avoids unbounded
    /// recursion / stack overflow when `remove_file` fails repeatedly).
    const STALE_LOCK_MAX_ATTEMPTS: u32 = 8;
    const STALE_LOCK_RETRY_SLEEP: Duration = Duration::from_secs(1);

    let cwd = std::env::current_dir().map_err(|e| {
        MinioError::Internal(format!("failed to get current directory: {e}"))
    })?;
    let lock_path = cwd.join(".minio.lock");

    let mut stale_break_attempts = 0u32;

    loop {
        // Open WITHOUT truncate so we can read an existing PID before overwriting.
        let file = fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&lock_path)
            .map_err(|e| {
                MinioError::Internal(format!("cannot open lock file {}: {e}", lock_path.display()))
            })?;

        match file.try_lock_exclusive() {
            Ok(()) => {
                // Acquired — now truncate and write our PID.
                file.set_len(0).map_err(|e| {
                    MinioError::Internal(format!("failed to truncate lock file: {e}"))
                })?;
                let pid = std::process::id();
                writeln!(&file, "{pid}").map_err(|e| {
                    MinioError::Internal(format!("failed to write PID to lock file: {e}"))
                })?;
                return Ok(InstanceLock {
                    _file: file,
                    path: lock_path,
                });
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                // Lock held — check whether the holder is still alive.
                let existing_pid = fs::read_to_string(&lock_path)
                    .ok()
                    .and_then(|s| s.trim().parse::<u32>().ok());

                if let Some(pid) = existing_pid {
                    if !pid_is_alive(pid) {
                        // Stale lock: the process holding it is dead.
                        tracing::warn!(
                            "stale lock detected (PID {pid} is dead), cleaning up and retrying…"
                        );
                        drop(file);
                        if let Err(e) = fs::remove_file(&lock_path) {
                            tracing::warn!(
                                path = %lock_path.display(),
                                %e,
                                "failed to remove stale lock file (will retry)",
                            );
                        }
                        stale_break_attempts += 1;
                        if stale_break_attempts >= STALE_LOCK_MAX_ATTEMPTS {
                            return Err(MinioError::Internal(format!(
                                "could not reclaim stale .minio.lock in {} after {} attempts (check permissions or remove the file manually)",
                                cwd.display(),
                                STALE_LOCK_MAX_ATTEMPTS
                            )));
                        }
                        std::thread::sleep(STALE_LOCK_RETRY_SLEEP);
                        continue;
                    }
                    return Err(MinioError::Internal(format!(
                        "another server instance (PID {pid}) is already running in {}",
                        cwd.display()
                    )));
                }

                return Err(MinioError::Internal(format!(
                    "another server instance is already running in {}",
                    cwd.display()
                )));
            }
            Err(e) => {
                return Err(MinioError::Internal(format!(
                    "failed to lock {}: {e}",
                    lock_path.display()
                )));
            }
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Check whether a process with the given PID is still running.
#[cfg(unix)]
fn pid_is_alive(pid: u32) -> bool {
    // kill(pid, 0) is the standard POSIX existence check — no signal is sent.
    // Per POSIX.1, if pid refers to a zombie whose status has not been waited
    // for, kill(0) still returns 0, which is what we want (the zombie hasn't
    // released its fds yet, so the flock is still valid).
    // SAFETY: passing a plain pid_t (i32) and signal 0; no memory or lifetime
    // invariants are involved — this is a POSIX syscall with fully defined behaviour.
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(not(unix))]
fn pid_is_alive(_pid: u32) -> bool {
    // No portable process-existence check on non-Unix platforms.
    // Assume alive to avoid breaking a valid lock.
    true
}
