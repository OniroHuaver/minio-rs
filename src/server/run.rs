//! Server startup flow: disk check, EC pool init, HTTP serve, graceful shutdown

use std::sync::Arc;

use crate::base::error::{MinioError, MinioResult};
use crate::object::{ErasureObjects, ObjectAPI};
use crate::s3::AppState;
use crate::storage::{DiskInfo, StorageAPI};
use tokio::signal;
use uuid::Uuid;

use crate::server::banner::print_banner;
use crate::server::disk::check_disks;

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
/// 4. Build axum Router (temporary placeholder; replace with `crate::s3::router(state)` later)
/// 5. Collect disk information for the startup banner
/// 6. Bind TCP listener
/// 7. Print startup banner
/// 8. Serve HTTP with graceful shutdown on SIGINT/SIGTERM
pub async fn run(config: ServerConfig) -> MinioResult<()> {
    // 1. Disk check and preparation
    let checked = check_disks(&config.disks).await?;

    // 2. Build ErasureObjects with automatic EC parity
    let disks: Vec<Arc<dyn StorageAPI>> = checked
        .iter()
        .map(|d| d.xl_storage.clone() as Arc<dyn StorageAPI>)
        .collect();
    let objects = Arc::new(ErasureObjects::new(disks)?);

    // 3. Build AppState
    let state = Arc::new(AppState {
        object_api: objects.clone() as Arc<dyn ObjectAPI>,
        instance_id: Uuid::now_v7().to_string(),
        region: "us-east-1".to_string(),
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

    // 6. Bind TCP listener before serving
    let listener = tokio::net::TcpListener::bind(&config.address).await?;

    // 7. Print startup banner
    print_banner(&config.address, config.console_address.as_deref(), &disk_infos);

    // 8. Start HTTP server with graceful shutdown
    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|e| MinioError::Internal(format!("HTTP server error: {e}")))?;

    tracing::info!("server stopped");
    Ok(())
}

/// Wait for SIGINT (Ctrl+C) or SIGTERM, then initiate graceful shutdown.
async fn shutdown_signal() {
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
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("shutdown signal received, initiating graceful shutdown...");
}
