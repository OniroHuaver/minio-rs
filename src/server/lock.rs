//! Instance file lock — ensures only one server process runs per directory.
//!
//! Uses `flock` (via `fs2`) on `.minio.lock` in the current working directory.
//! The lock is OS-level and auto-released when the process exits, so a crash
//! won't leave a stale lock behind.

use std::fs::{self, File};
use std::io::{self, Write};

use fs2::FileExt;

use crate::base::error::{MinioError, MinioResult};

/// RAII guard that holds the exclusive instance lock.
pub struct InstanceLock {
    _file: File,
}

/// Try to acquire the exclusive instance lock.
///
/// Creates `.minio.lock` in the current working directory, acquires an
/// exclusive OS-level advisory lock on it, and writes this process's PID.
/// Returns an error if another process already holds the lock.
pub fn acquire() -> MinioResult<InstanceLock> {
    let cwd = std::env::current_dir().map_err(|e| {
        MinioError::Internal(format!("failed to get current directory: {e}"))
    })?;
    let lock_path = cwd.join(".minio.lock");

    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|e| MinioError::Internal(format!("cannot open lock file {}: {e}", lock_path.display())))?;

    file.try_lock_exclusive().map_err(|e| {
        if e.kind() == io::ErrorKind::WouldBlock {
            // Another instance holds the lock. Try to read its PID for a helpful message.
            let pid_hint = fs::read_to_string(&lock_path)
                .ok()
                .and_then(|s| s.trim().parse::<u32>().ok())
                .map(|pid| format!(" (PID {pid})"))
                .unwrap_or_default();
            MinioError::Internal(format!(
                "another server instance{pid_hint} is already running in {}",
                cwd.display()
            ))
        } else {
            MinioError::Internal(format!("failed to lock {}: {e}", lock_path.display()))
        }
    })?;

    // Write our PID into the lock file for diagnostics
    let pid = std::process::id();
    writeln!(&file, "{pid}").map_err(|e| {
        MinioError::Internal(format!("failed to write PID to lock file: {e}"))
    })?;

    Ok(InstanceLock { _file: file })
}
