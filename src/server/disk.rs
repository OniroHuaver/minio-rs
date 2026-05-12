//! Disk path checking and preparation
//!
//! On startup each disk path is verified:
//! 1. Directory created if absent
//! 2. `.minio.sys/tmp/` and `.minio.sys/multipart/` created
//! 3. An `XlStorage` instance is created and wrapped in `Arc`

use std::path::PathBuf;
use std::sync::Arc;

use crate::base::error::{MinioError, MinioResult};
use crate::storage::XlStorage;

/// A disk that has been checked and is ready for use
pub struct CheckedDisk {
    pub path: PathBuf,
    pub xl_storage: Arc<XlStorage>,
}

/// Validate and prepare each disk path for use.
///
/// For each path:
/// - Creates the directory if it does not exist
/// - Ensures `.minio.sys/tmp/` and `.minio.sys/multipart/` sub-directories exist
/// - Creates an `XlStorage` instance (local disk, empty endpoint string)
pub async fn check_disks(disk_paths: &[String]) -> MinioResult<Vec<CheckedDisk>> {
    let mut checked = Vec::with_capacity(disk_paths.len());

    for path_str in disk_paths {
        let path = PathBuf::from(path_str);

        // Create disk directory if it does not exist
        match tokio::fs::metadata(&path).await {
            Ok(_) => {} // already exists
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tokio::fs::create_dir_all(&path).await?;
                tracing::info!("created disk directory: {}", path.display());
            }
            Err(e) => return Err(MinioError::DiskIO(e)),
        }

        // Ensure .minio.sys sub-directories exist (MinIO convention)
        let sys_tmp = path.join(".minio.sys").join("tmp");
        let sys_multipart = path.join(".minio.sys").join("multipart");
        tokio::fs::create_dir_all(&sys_tmp).await?;
        tokio::fs::create_dir_all(&sys_multipart).await?;

        let xl_storage = Arc::new(XlStorage::new(&path, ""));

        checked.push(CheckedDisk { path, xl_storage });
    }

    Ok(checked)
}
