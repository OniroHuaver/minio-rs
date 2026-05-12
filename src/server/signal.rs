//! Signal handling: signalfd-based on Linux, tokio fallback on other platforms.
//!
//! On Linux we block SIGTERM/SIGINT at the thread level before tokio spawns any
//! worker threads, then create a signalfd and wrap it in a tokio AsyncFd so the
//! epoll reactor wakes us when a signal arrives. On other platforms we use
//! tokio's built-in signal primitives.

// Linux signalfd path uses libc FFI; keep `unsafe` confined to this module.
#![allow(unsafe_code)]

use std::future::Future;
use std::pin::Pin;

/// Block SIGTERM/SIGINT before the tokio runtime spawns worker threads.
/// On Linux this is required for signalfd to receive the signals.
/// On other platforms this is a no-op.
pub fn block_signals() {
    #[cfg(target_os = "linux")]
    {
        let mut sigset: libc::sigset_t = unsafe { std::mem::zeroed() };
        unsafe {
            libc::sigemptyset(&mut sigset);
            libc::sigaddset(&mut sigset, libc::SIGTERM);
            libc::sigaddset(&mut sigset, libc::SIGINT);
            libc::pthread_sigmask(libc::SIG_BLOCK, &sigset, std::ptr::null_mut());
        }
        tracing::debug!("blocked SIGTERM/SIGINT for signalfd integration");
    }
}

/// Future that resolves once a shutdown signal (SIGINT or SIGTERM) arrives.
pub fn shutdown_signal() -> Pin<Box<dyn Future<Output = ()> + Send>> {
    #[cfg(target_os = "linux")]
    {
        Box::pin(linux_signalfd())
    }
    #[cfg(not(target_os = "linux"))]
    {
        Box::pin(tokio_signals())
    }
}

// ── Linux signalfd ──────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
async fn linux_signalfd() {
    // Try signalfd first; fall back to tokio signals on failure.
    if let Err(e) = try_signalfd().await {
        tracing::warn!("signalfd failed ({}), falling back to tokio signals", e);
        tokio_signals().await;
    }
}

#[cfg(target_os = "linux")]
async fn try_signalfd() -> Result<(), std::io::Error> {
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use tokio::io::unix::AsyncFd;

    // Build the signal set matching what we blocked in block_signals()
    let mut sigset: libc::sigset_t = unsafe { std::mem::zeroed() };
    unsafe {
        libc::sigemptyset(&mut sigset);
        libc::sigaddset(&mut sigset, libc::SIGTERM);
        libc::sigaddset(&mut sigset, libc::SIGINT);
    }

    let fd = unsafe {
        libc::signalfd(-1, &sigset, libc::SFD_NONBLOCK | libc::SFD_CLOEXEC)
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }

    let signal_fd = unsafe { OwnedFd::from_raw_fd(fd) };
    let async_fd = AsyncFd::new(signal_fd)?;

    loop {
        let mut guard = async_fd.readable().await?;
        let mut info: libc::signalfd_siginfo = unsafe { std::mem::zeroed() };

        match guard.try_io(|inner| {
            let n = unsafe {
                libc::read(
                    inner.as_raw_fd(),
                    &mut info as *mut _ as *mut libc::c_void,
                    std::mem::size_of::<libc::signalfd_siginfo>(),
                )
            };
            if n < 0 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(info.ssi_signo)
            }
        }) {
            Ok(Ok(signo)) => {
                let name = if signo == libc::SIGTERM as u32 { "SIGTERM" } else { "SIGINT" };
                tracing::info!("received {name} via signalfd, initiating graceful shutdown…");
                return Ok(());
            }
            Ok(Err(e)) => return Err(e),
            Err(_would_block) => continue,
        }
    }
}

// ── Tokio signal fallback ───────────────────────────────────────────────────

#[cfg(not(target_os = "linux"))]
async fn tokio_signals() {
    use tokio::signal;

    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("received SIGINT, initiating graceful shutdown…"),
        _ = terminate => tracing::info!("received SIGTERM, initiating graceful shutdown…"),
    }
}

// Need this fallback on Linux too (used when signalfd setup fails)
#[cfg(target_os = "linux")]
async fn tokio_signals() {
    use tokio::signal;

    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    tokio::select! {
        _ = ctrl_c => tracing::info!("received SIGINT, initiating graceful shutdown…"),
        _ = terminate => tracing::info!("received SIGTERM, initiating graceful shutdown…"),
    }
}
