//! xlStorage -- local disk storage driver
//!
//! Implements the StorageAPI trait for local filesystem I/O:
//! file read/write, atomic rename, directory management, disk info.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use async_trait::async_trait;
use base::error::{MinioError, MinioResult};

use crate::{DiskInfo, FileStat, StorageAPI};

/// Local disk storage driver
#[derive(Debug, Clone)]
pub struct XlStorage {
    disk_path: PathBuf,
    endpoint: String,
}

impl XlStorage {
    pub fn new(disk_path: impl Into<PathBuf>, endpoint: impl Into<String>) -> Self {
        Self {
            disk_path: disk_path.into(),
            endpoint: endpoint.into(),
        }
    }

    pub fn disk_path(&self) -> &Path {
        &self.disk_path
    }

    fn abs_path(&self, volume: &str, path: &str) -> PathBuf {
        self.disk_path.join(volume).join(path)
    }

    fn volume_path(&self, volume: &str) -> PathBuf {
        self.disk_path.join(volume)
    }

    /// Validate volume name: non-empty, no `..`, no `\`
    ///
    /// `/` is allowed for nested volume paths like `.minio.sys/tmp`
    fn validate_volume(&self, volume: &str) -> MinioResult<()> {
        if volume.is_empty() {
            return Err(MinioError::Internal("volume name is empty".into()));
        }
        if volume.contains("..") || volume.contains('\\') {
            return Err(MinioError::Internal(format!(
                "invalid volume name: {volume}"
            )));
        }
        Ok(())
    }
}

#[async_trait]
impl StorageAPI for XlStorage {
    async fn disk_info(&self) -> MinioResult<DiskInfo> {
        let formatted = self
            .disk_path
            .join(".minio.sys")
            .join("format.json")
            .exists();
        let online = self.disk_path.exists();

        // TODO: use fs_stat or sysinfo crate to get real disk capacity
        Ok(DiskInfo {
            total: 0,
            free: 0,
            used: 0,
            mount_path: self.disk_path.to_string_lossy().to_string(),
            online,
            formatted,
            healing: false,
            endpoint: self.endpoint.clone(),
        })
    }

    async fn is_online(&self) -> bool {
        self.disk_path.exists()
    }

    fn endpoint(&self) -> &str {
        &self.endpoint
    }

    async fn read_all(&self, volume: &str, path: &str) -> MinioResult<Vec<u8>> {
        let file_path = self.abs_path(volume, path);
        tokio::fs::read(&file_path)
            .await
            .map_err(MinioError::DiskIO)
    }

    async fn read_range(
        &self,
        volume: &str,
        path: &str,
        offset: i64,
        length: i64,
    ) -> MinioResult<Vec<u8>> {
        use tokio::io::{AsyncReadExt, AsyncSeekExt};

        let file_path = self.abs_path(volume, path);
        let mut file = tokio::fs::File::open(&file_path)
            .await
            .map_err(MinioError::DiskIO)?;

        let file_len = file
            .metadata()
            .await
            .map_err(MinioError::DiskIO)?
            .len() as i64;

        if offset < 0 || length <= 0 || offset >= file_len {
            return Ok(Vec::new());
        }
        let actual_len = std::cmp::min(length, file_len - offset);

        file.seek(std::io::SeekFrom::Start(offset as u64))
            .await
            .map_err(MinioError::DiskIO)?;

        let mut buf = vec![0u8; actual_len as usize];
        file.read_exact(&mut buf)
            .await
            .map_err(MinioError::DiskIO)?;
        Ok(buf)
    }

    async fn write_all(&self, volume: &str, path: &str, data: &[u8]) -> MinioResult<()> {
        let file_path = self.abs_path(volume, path);
        if let Some(parent) = file_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(MinioError::DiskIO)?;
        }
        tokio::fs::write(&file_path, data)
            .await
            .map_err(MinioError::DiskIO)
    }

    async fn append_file(&self, volume: &str, path: &str, data: &[u8]) -> MinioResult<()> {
        use tokio::io::AsyncWriteExt;

        let file_path = self.abs_path(volume, path);
        if let Some(parent) = file_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(MinioError::DiskIO)?;
        }
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)
            .await
            .map_err(MinioError::DiskIO)?;
        file.write_all(data).await.map_err(MinioError::DiskIO)?;
        Ok(())
    }

    async fn delete(&self, volume: &str, path: &str) -> MinioResult<()> {
        let file_path = self.abs_path(volume, path);
        match tokio::fs::remove_file(&file_path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(MinioError::DiskIO(e)),
        }
    }

    async fn rename(
        &self,
        src_volume: &str,
        src_path: &str,
        dst_volume: &str,
        dst_path: &str,
    ) -> MinioResult<()> {
        let src = self.abs_path(src_volume, src_path);
        let dst = self.abs_path(dst_volume, dst_path);
        if let Some(parent) = dst.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(MinioError::DiskIO)?;
        }
        tokio::fs::rename(&src, &dst)
            .await
            .map_err(MinioError::DiskIO)
    }

    async fn list_dir(
        &self,
        volume: &str,
        dir_path: &str,
        count: usize,
    ) -> MinioResult<Vec<String>> {
        let dir = self.abs_path(volume, dir_path);
        let mut entries = Vec::new();
        let mut read_dir = tokio::fs::read_dir(&dir)
            .await
            .map_err(MinioError::DiskIO)?;
        // count=0 means unlimited, but cap internally to prevent OOM
        const MAX_LIST_LIMIT: usize = 100_000;
        let limit = if count == 0 { MAX_LIST_LIMIT } else { std::cmp::min(count, MAX_LIST_LIMIT) };
        while let Some(entry) = read_dir
            .next_entry()
            .await
            .map_err(MinioError::DiskIO)?
        {
            entries.push(entry.file_name().to_string_lossy().to_string());
            if entries.len() >= limit {
                break;
            }
        }
        Ok(entries)
    }

    async fn make_volume(&self, volume: &str) -> MinioResult<()> {
        self.validate_volume(volume)?;
        let dir = self.volume_path(volume);
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(MinioError::DiskIO)
    }

    async fn delete_volume(&self, volume: &str) -> MinioResult<()> {
        self.validate_volume(volume)?;
        let dir = self.volume_path(volume);
        match tokio::fs::remove_dir_all(&dir).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tracing::debug!(
                    volume = volume,
                    dir = %dir.display(),
                    "delete_volume: directory already absent (idempotent)"
                );
                Ok(())
            }
            Err(e) => Err(MinioError::DiskIO(e)),
        }
    }

    async fn stat_file(&self, volume: &str, path: &str) -> MinioResult<FileStat> {
        let file_path = self.abs_path(volume, path);
        let metadata = tokio::fs::metadata(&file_path)
            .await
            .map_err(MinioError::DiskIO)?;
        let mod_time = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_nanos() as i64)
            .unwrap_or_else(|| {
                tracing::warn!(
                    file = %file_path.display(),
                    "failed to get file mtime, falling back to 0 (may affect version ordering)"
                );
                0
            });
        Ok(FileStat {
            size: metadata.len() as i64,
            mod_time,
            is_dir: metadata.is_dir(),
        })
    }

    async fn file_exists(&self, volume: &str, path: &str) -> MinioResult<bool> {
        let file_path = self.abs_path(volume, path);
        match tokio::fs::metadata(&file_path).await {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(MinioError::DiskIO(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_dir() -> std::path::PathBuf {
        let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let pid = std::process::id();
        std::env::temp_dir().join(format!("minio_test_{}_{}", pid, id))
    }

    #[tokio::test]
    async fn test_make_and_delete_volume() {
        let dir = temp_dir();
        let storage = XlStorage::new(&dir, "test");
        assert!(!dir.exists());

        storage.make_volume("test-bucket").await.unwrap();
        assert!(dir.join("test-bucket").exists());

        storage.delete_volume("test-bucket").await.unwrap();
        assert!(!dir.join("test-bucket").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_write_read_delete() {
        let dir = temp_dir();
        let storage = XlStorage::new(&dir, "test");
        storage.make_volume("bucket").await.unwrap();

        let data = b"hello minio-rs xl.meta format v2";
        storage.write_all("bucket", "obj/xl.meta", data).await.unwrap();
        assert!(storage.file_exists("bucket", "obj/xl.meta").await.unwrap());

        let read = storage.read_all("bucket", "obj/xl.meta").await.unwrap();
        assert_eq!(read, data);

        storage.delete("bucket", "obj/xl.meta").await.unwrap();
        assert!(!storage.file_exists("bucket", "obj/xl.meta").await.unwrap());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_read_range() {
        let dir = temp_dir();
        let storage = XlStorage::new(&dir, "test");
        storage.make_volume("bucket").await.unwrap();

        let data: Vec<u8> = (0..100u8).collect();
        storage.write_all("bucket", "range-test", &data).await.unwrap();

        let part = storage.read_range("bucket", "range-test", 10, 20).await.unwrap();
        assert_eq!(part, &data[10..30]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_rename() {
        let dir = temp_dir();
        let storage = XlStorage::new(&dir, "test");
        storage.make_volume("bucket").await.unwrap();

        let content = b"before rename";
        storage.write_all("bucket", "src.txt", content).await.unwrap();
        storage.rename("bucket", "src.txt", "bucket", "dst.txt").await.unwrap();

        assert!(!storage.file_exists("bucket", "src.txt").await.unwrap());
        assert_eq!(storage.read_all("bucket", "dst.txt").await.unwrap(), content);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_list_dir() {
        let dir = temp_dir();
        let storage = XlStorage::new(&dir, "test");
        storage.make_volume("bucket").await.unwrap();

        storage.write_all("bucket", "a.txt", b"a").await.unwrap();
        storage.write_all("bucket", "b.txt", b"b").await.unwrap();
        storage.write_all("bucket", "c.txt", b"c").await.unwrap();

        let mut files = storage.list_dir("bucket", "", 0).await.unwrap();
        files.sort();
        assert_eq!(files, vec!["a.txt", "b.txt", "c.txt"]);

        let limited = storage.list_dir("bucket", "", 2).await.unwrap();
        assert_eq!(limited.len(), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_file_stat() {
        let dir = temp_dir();
        let storage = XlStorage::new(&dir, "test");
        storage.make_volume("bucket").await.unwrap();
        storage.write_all("bucket", "stat.txt", b"hello!").await.unwrap();

        let stat = storage.stat_file("bucket", "stat.txt").await.unwrap();
        assert_eq!(stat.size, 6);
        assert!(!stat.is_dir);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
