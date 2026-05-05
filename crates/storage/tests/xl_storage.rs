//! xlStorage tests
//!
//! Tests all IO operations of the local disk storage layer (xlStorage).

use std::path::PathBuf;
use storage::{StorageAPI, XlStorage};

fn temp_disk() -> (XlStorage, PathBuf) {
    let dir = std::env::temp_dir().join(format!("minio_xl_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let storage = XlStorage::new(&dir, "test-endpoint");
    (storage, dir)
}

fn cleanup(dir: &PathBuf) {
    let _ = std::fs::remove_dir_all(dir);
}

// ==================== Disk initialization ====================

/// Tests xlStorage initialization
#[tokio::test]
async fn test_new_xl_storage() {
    // Normal create (create dir first, then init)
    let tmp = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&tmp).unwrap();
    let storage = XlStorage::new(&tmp, "test-ep");
    assert!(storage.is_online().await);
    assert_eq!(storage.endpoint(), "test-ep");
    cleanup(&tmp);

    // Empty path -- no error
    let storage = XlStorage::new("", "test");
    assert_eq!(storage.endpoint(), "test");
}

/// Tests xlStorage.disk_info disk info retrieval
#[tokio::test]
async fn test_xl_storage_get_disk_info() {
    let (storage, dir) = temp_disk();

    let info = storage.disk_info().await.unwrap();
    assert!(info.online);
    assert!(!info.healing);
    assert!(!info.formatted); // New directory has no format.json

    cleanup(&dir);
}

// ==================== Volume management ====================

/// Tests xlStorage.make_volume volume creation
#[tokio::test]
async fn test_xl_storage_make_vol() {
    let (storage, dir) = temp_disk();

    // Normal creation
    storage.make_volume("success-vol").await.unwrap();
    assert!(dir.join("success-vol").exists());

    // Re-create same volume -- create_dir_all is idempotent, no error
    assert!(storage.make_volume("success-vol").await.is_ok());

    // Nested path volume names are also supported
    storage.make_volume("nested/dir/vol").await.unwrap();
    assert!(dir.join("nested/dir/vol").exists());

    cleanup(&dir);
}

/// Tests xlStorage.delete_volume volume deletion
#[tokio::test]
async fn test_xl_storage_delete_vol() {
    let (storage, dir) = temp_disk();

    // Create -> delete
    storage.make_volume("to-delete").await.unwrap();
    assert!(dir.join("to-delete").exists());
    storage.delete_volume("to-delete").await.unwrap();
    assert!(!dir.join("to-delete").exists());

    // Non-existent volume -- idempotent success
    assert!(storage.delete_volume("nonexistent-vol").await.is_ok());

    cleanup(&dir);
}

// ==================== File IO ====================

/// Tests xlStorage.read_all reading full file content
#[tokio::test]
async fn test_xl_storage_read_all() {
    let (storage, dir) = temp_disk();
    storage.make_volume("bucket").await.unwrap();

    let data = b"Hello, MinIO XL Storage v2!";
    storage.write_all("bucket", "readme.txt", data).await.unwrap();

    // Normal read
    let read = storage.read_all("bucket", "readme.txt").await.unwrap();
    assert_eq!(read, data);

    // Non-existent file
    assert!(storage.read_all("bucket", "no-file").await.is_err());

    // Non-existent volume
    assert!(storage.read_all("no-vol", "x").await.is_err());

    cleanup(&dir);
}

/// Tests xlStorage.read_range range read
#[tokio::test]
async fn test_xl_storage_read_range() {
    let (storage, dir) = temp_disk();
    storage.make_volume("bucket").await.unwrap();

    let data: Vec<u8> = (0..100u8).collect();
    storage.write_all("bucket", "range-test", &data).await.unwrap();

    // Range read
    let part = storage.read_range("bucket", "range-test", 10, 20).await.unwrap();
    assert_eq!(part, &data[10..30]);

    // Negative offset -> returns empty
    let part = storage.read_range("bucket", "range-test", -1, 20).await.unwrap();
    assert!(part.is_empty());

    // Out of range -> returns empty
    let part = storage.read_range("bucket", "range-test", 200, 10).await.unwrap();
    assert!(part.is_empty());

    cleanup(&dir);
}

/// Tests xlStorage.append_file append write
#[tokio::test]
async fn test_xl_storage_append_file() {
    let (storage, dir) = temp_disk();
    storage.make_volume("bucket").await.unwrap();

    // New file
    storage.append_file("bucket", "log.txt", b"line1").await.unwrap();
    assert_eq!(storage.read_all("bucket", "log.txt").await.unwrap(), b"line1");

    // Append
    storage.append_file("bucket", "log.txt", b"\nline2").await.unwrap();
    assert_eq!(storage.read_all("bucket", "log.txt").await.unwrap(), b"line1\nline2");

    // Nested path
    storage.append_file("bucket", "a/b/c/data.bin", b"nested").await.unwrap();
    assert_eq!(storage.read_all("bucket", "a/b/c/data.bin").await.unwrap(), b"nested");

    cleanup(&dir);
}

/// Tests xlStorage.write_all overwrite
#[tokio::test]
async fn test_xl_storage_write_all() {
    let (storage, dir) = temp_disk();
    storage.make_volume("bucket").await.unwrap();

    storage.write_all("bucket", "config.json", b"v1").await.unwrap();
    assert_eq!(storage.read_all("bucket", "config.json").await.unwrap(), b"v1");

    // Overwrite
    storage.write_all("bucket", "config.json", b"v2-overwrite").await.unwrap();
    assert_eq!(storage.read_all("bucket", "config.json").await.unwrap(), b"v2-overwrite");

    cleanup(&dir);
}

/// Tests xlStorage.delete file deletion
#[tokio::test]
async fn test_xl_storage_delete_file() {
    let (storage, dir) = temp_disk();
    storage.make_volume("bucket").await.unwrap();
    storage.write_all("bucket", "tmp.txt", b"temp").await.unwrap();

    assert!(storage.file_exists("bucket", "tmp.txt").await.unwrap());

    // Normal delete
    storage.delete("bucket", "tmp.txt").await.unwrap();
    assert!(!storage.file_exists("bucket", "tmp.txt").await.unwrap());

    // Idempotent delete
    assert!(storage.delete("bucket", "tmp.txt").await.is_ok());

    // Delete non-existent file
    assert!(storage.delete("bucket", "never-exists").await.is_ok());

    cleanup(&dir);
}

/// Tests xlStorage.rename file rename/move
#[tokio::test]
async fn test_xl_storage_rename_file() {
    let (storage, dir) = temp_disk();
    storage.make_volume("src").await.unwrap();
    storage.make_volume("dst").await.unwrap();

    let content = b"rename me";
    storage.write_all("src", "file-a", content).await.unwrap();

    // Cross-volume rename
    storage.rename("src", "file-a", "dst", "file-b").await.unwrap();
    assert!(!storage.file_exists("src", "file-a").await.unwrap());
    assert_eq!(storage.read_all("dst", "file-b").await.unwrap(), content);

    // Same-volume rename (overwrite destination)
    storage.write_all("dst", "old", b"old-data").await.unwrap();
    storage.rename("dst", "file-b", "dst", "old").await.unwrap();
    assert_eq!(storage.read_all("dst", "old").await.unwrap(), content);

    // Source does not exist -> error
    let result = storage.rename("dst", "ghost", "dst", "x").await;
    assert!(result.is_err());

    cleanup(&dir);
}

// ==================== Directory operations ====================

/// Tests xlStorage.list_dir directory listing
#[tokio::test]
async fn test_xl_storage_list_dir() {
    let (storage, dir) = temp_disk();
    storage.make_volume("bucket").await.unwrap();

    storage.write_all("bucket", "a.txt", b"a").await.unwrap();
    storage.write_all("bucket", "b.txt", b"b").await.unwrap();
    storage.write_all("bucket", "c.txt", b"c").await.unwrap();

    // Full listing
    let mut files = storage.list_dir("bucket", "", 0).await.unwrap();
    files.sort();
    assert_eq!(files, vec!["a.txt", "b.txt", "c.txt"]);

    // Count limit
    let limited = storage.list_dir("bucket", "", 2).await.unwrap();
    assert_eq!(limited.len(), 2);

    cleanup(&dir);
}

// ==================== File stats ====================

/// Tests xlStorage.stat_file file stats
#[tokio::test]
async fn test_xl_storage_stat_info_file() {
    let (storage, dir) = temp_disk();
    storage.make_volume("bucket").await.unwrap();
    storage.write_all("bucket", "stat.txt", b"hello!").await.unwrap();

    let stat = storage.stat_file("bucket", "stat.txt").await.unwrap();
    assert_eq!(stat.size, 6);
    assert!(!stat.is_dir);
    assert!(stat.mod_time > 0);

    // Non-existent file
    assert!(storage.stat_file("bucket", "no-file").await.is_err());

    cleanup(&dir);
}

/// Tests xlStorage.file_exists file existence check
#[tokio::test]
async fn test_xl_storage_file_exists() {
    let (storage, dir) = temp_disk();
    storage.make_volume("bucket").await.unwrap();

    assert!(!storage.file_exists("bucket", "not-here").await.unwrap());

    storage.write_all("bucket", "here", b"yes").await.unwrap();
    assert!(storage.file_exists("bucket", "here").await.unwrap());

    storage.delete("bucket", "here").await.unwrap();
    assert!(!storage.file_exists("bucket", "here").await.unwrap());

    cleanup(&dir);
}

// ==================== Tests below need more infrastructure, kept as ignore for now ====================

#[ignore]
#[test]
fn test_xl_storage_read_version_legacy() {
    // TODO: implement when ReadVersion is available
}

#[ignore]
#[tokio::test]
async fn test_xl_storage_read_version() {
    // TODO: implement when ReadVersion is available
}

#[ignore]
#[tokio::test]
async fn test_xl_storage_read_file_with_verify() {
    // TODO: implement when bitrot verification is available
}

#[ignore]
#[tokio::test]
async fn test_xl_storage_format_file_change() {
    // TODO: implement when format.json checking is available
}

#[ignore]
#[tokio::test]
async fn test_xl_storage_delete_version() {
    // TODO: implement when versioned delete is available
}

#[ignore]
#[tokio::test]
async fn test_xl_storage_list_vols() {
    // TODO: implement when list_vols is available
}

#[ignore]
#[tokio::test]
async fn test_xl_storage_stat_vol() {
    // TODO: implement when stat_vol is available
}

#[ignore]
#[test]
fn test_xl_storage_is_dir_empty() {
    // TODO: implement when is_dir_empty is available
}
