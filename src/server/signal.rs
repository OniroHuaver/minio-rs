//! Signal handling: signalfd on Linux, kqueue EVFILT_SIGNAL on macOS/BSD,
//! tokio signal fallback on other platforms.
//!
//! Linux:   block signals → signalfd (epoll-integrated fd) → AsyncFd
//! macOS:   block signals → tokio::signal (kqueue EVFILT_SIGNAL internally)
//! Other:   tokio::signal (signal-hook-registry with thread-based delivery)
//!
//! On all unix we block SIGTERM/SIGINT before tokio spawns worker threads.
//! This prevents signals from being delivered to random threads and ensures
//! signalfd (Linux) or kqueue (macOS) can monitor them properly.

// signalfd path uses libc FFI; keep `unsafe` confined to this module.
#![allow(unsafe_code)]

use std::future::Future;
use std::pin::Pin;

/// Block SIGTERM/SIGINT at the thread level before tokio spawns worker threads.
///
/// Required for signalfd to work (Linux) and prevents signals from landing on
/// arbitrary worker threads on macOS/BSD.
pub fn block_signals() {
    #[cfg(unix)]
    {
        // SAFETY: `sigset_t` is a C POD mask; zero-initialization matches `sigemptyset` initial state.
        let mut sigset: libc::sigset_t = unsafe { std::mem::zeroed() };
        unsafe {
            // SAFETY: `sigset` is a valid pointer to a zeroed `sigset_t`; we only register SIGTERM/SIGINT.
            libc::sigemptyset(&mut sigset);
            libc::sigaddset(&mut sigset, libc::SIGTERM);
            libc::sigaddset(&mut sigset, libc::SIGINT);
            // SAFETY: `sigset` is fully built; `oldset` is null (we do not read the previous mask).
            let rc = libc::pthread_sigmask(libc::SIG_BLOCK, &sigset, std::ptr::null_mut());
            if rc != 0 {
                tracing::error!(
                    rc,
                    "pthread_sigmask(SIG_BLOCK, SIGTERM|SIGINT) failed; signals may be delivered on arbitrary threads (signalfd / kqueue behaviour may be unreliable)"
                );
            }
        }
        tracing::debug!("blocked SIGTERM/SIGINT before tokio thread pool spawn");
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
    if let Err(e) = try_signalfd().await {
        tracing::warn!("signalfd failed ({}), falling back to tokio signals", e);
        tokio_signals().await;
    }
}

#[cfg(target_os = "linux")]
async fn try_signalfd() -> Result<(), std::io::Error> {
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use tokio::io::unix::AsyncFd;

    // SAFETY: `sigset_t` is POD; zero-init is valid before `sigemptyset`/`sigaddset`.
    let mut sigset: libc::sigset_t = unsafe { std::mem::zeroed() };
    unsafe {
        // SAFETY: `sigset` points to a valid mask; only standard signals are added.
        libc::sigemptyset(&mut sigset);
        libc::sigaddset(&mut sigset, libc::SIGTERM);
        libc::sigaddset(&mut sigset, libc::SIGINT);
    }

    // SAFETY: `-1` creates a new signalfd; `sigset` is valid; flags match Linux signalfd(2).
    let fd = unsafe { libc::signalfd(-1, &sigset, libc::SFD_NONBLOCK | libc::SFD_CLOEXEC) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }

    // SAFETY: `fd` is a valid signalfd file descriptor returned above on success.
    let signal_fd = unsafe { OwnedFd::from_raw_fd(fd) };
    let async_fd = AsyncFd::new(signal_fd)?;

    loop {
        let mut guard = async_fd.readable().await?;
        // SAFETY: `signalfd_siginfo` is POD; zeroed buffer is valid for signalfd read.
        let mut info: libc::signalfd_siginfo = unsafe { std::mem::zeroed() };

        match guard.try_io(|inner| {
            // SAFETY: `inner` is a valid FD; `info` has size `sizeof(signalfd_siginfo)` as required by signalfd(2).
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

// ── Tokio signal (macOS, BSD, Linux fallback) ───────────────────────────────
//
// On macOS/BSD, tokio::signal uses kqueue EVFILT_SIGNAL internally via mio.
// On Linux, it uses signal-hook-registry with a dedicated signal thread.
// The block_signals() call above ensures signals are masked before tokio
// starts, so they won't be delivered to worker threads.

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
