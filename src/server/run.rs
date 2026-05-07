//! Server startup flow: disk check, EC pool init, HTTP serve, graceful shutdown

use std::sync::Arc;

use crate::base::error::{MinioError, MinioResult};
use crate::object::{ErasureObjects, ObjectAPI, StandaloneObjects};
use crate::s3::AppState;
use crate::storage::{DiskInfo, StorageAPI};
use uuid::Uuid;

use crate::server::banner::print_banner;
use crate::server::disk::check_disks;
use crate::server::signal;

/// Server configuration assembled from CLI arguments
pub struct ServerConfig {
    pub address: String,
    pub console_address: Option<String>,
    pub disks: Vec<String>,
}

/// Start the object storage server.
///
/// The full startup sequence:
/// 1. Check and prepare all disk paths
/// 2. Build `ErasureObjects` with automatic EC parity selection
/// 3. Construct shared `AppState`
/// 4. Build axum Router
/// 5. Collect disk information for the startup banner
/// 6. Bind TCP listener with SO_REUSEPORT for multi-process accept
/// 7. Print startup banner
/// 8. Serve HTTP with graceful shutdown (signalfd on Linux, tokio signals elsewhere)
pub async fn run(config: ServerConfig) -> MinioResult<()> {
    // 0. Acquire instance lock — prevents duplicate server processes
    let _instance_lock = crate::server::lock::acquire()?;

    // 1. Disk check and preparation
    let checked = check_disks(&config.disks).await?;

    // 2. Build object store: standalone for 1-2 disks, EC for >=3
    let disks: Vec<Arc<dyn StorageAPI>> = checked
        .iter()
        .map(|d| d.xl_storage.clone() as Arc<dyn StorageAPI>)
        .collect();
    let objects: Arc<dyn ObjectAPI> = if disks.len() < 3 {
        tracing::info!("standalone mode ({} disk(s), no EC)", disks.len());
        Arc::new(StandaloneObjects::new(disks.into_iter().next().unwrap()))
    } else {
        tracing::info!("erasure coding mode ({} disks)", disks.len());
        Arc::new(ErasureObjects::new(disks)?)
    };

    // 3. Build AppState
    let state = Arc::new(AppState {
        object_api: objects.clone() as Arc<dyn ObjectAPI>,
        instance_id: Uuid::now_v7().to_string(),
        region: "us-east-1".to_string(),
        credentials: std::env::var("MINIO_ROOT_USER").ok().and_then(|ak| {
            std::env::var("MINIO_ROOT_PASSWORD")
                .ok()
                .map(|sk| (ak, sk))
        }),
    });

    // 4. Build S3 HTTP router
    let app = crate::s3::router(state);

    // 5. Collect disk info for startup banner
    let disk_infos: Vec<DiskInfo> = {
        let mut infos = Vec::with_capacity(checked.len());
        for disk in &checked {
            match disk.xl_storage.disk_info().await {
                Ok(info) => infos.push(info),
                Err(e) => {
                    tracing::warn!("failed to get disk info for {}: {e}", disk.path.display());
                }
            }
        }
        infos
    };

    // 6. Bind TCP listener (SO_REUSEPORT for multi-process accept perf)
    let listener = bind_tcp_listener(&config.address)?;

    // 7. Print startup banner
    print_banner(&config.address, config.console_address.as_deref(), &disk_infos);

    // 8. Start HTTP server with graceful shutdown
    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(signal::shutdown_signal())
        .await
        .map_err(|e| MinioError::Internal(format!("HTTP server error: {e}")))?;

    tracing::info!("server stopped");
    Ok(())
}

/// Bind a TCP listener with SO_REUSEPORT + SO_REUSEADDR.
///
/// SO_REUSEPORT allows multiple processes to bind to the same port; the
/// kernel distributes incoming connections across them, improving accept
/// throughput. SO_REUSEADDR permits immediate rebind after a restart.
fn bind_tcp_listener(addr: &str) -> MinioResult<tokio::net::TcpListener> {
    use std::net::{SocketAddr, ToSocketAddrs};

    let socket_addr: SocketAddr = addr
        .to_socket_addrs()
        .ok()
        .and_then(|mut iter| iter.next())
        .ok_or_else(|| MinioError::Internal(format!("invalid bind address: {addr}")))?;

    let socket = match socket_addr {
        SocketAddr::V4(_) => tokio::net::TcpSocket::new_v4(),
        SocketAddr::V6(_) => tokio::net::TcpSocket::new_v6(),
    }
    .map_err(MinioError::DiskIO)?;

    socket
        .set_reuseaddr(true)
        .map_err(MinioError::DiskIO)?;

    #[cfg(unix)]
    socket
        .set_reuseport(true)
        .map_err(MinioError::DiskIO)?;

    socket.bind(socket_addr).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AddrInUse {
            MinioError::PortInUse(format!(
                "port {} is already in use — is another instance running?",
                socket_addr.port()
            ))
        } else {
            MinioError::DiskIO(e)
        }
    })?;

    // backlog 1024 matches tokio's internal default
    socket
        .listen(1024)
        .map_err(MinioError::DiskIO)
}
