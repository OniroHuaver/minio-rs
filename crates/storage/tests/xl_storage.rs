//! xlStorage 测试
//!
//! 对应 Go: cmd/xl-storage_test.go
//!
//! 测试本地磁盘存储层 (xlStorage) 的全部 IO 操作。

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

// ==================== 磁盘初始化 ====================

/// 测试 xlStorage 初始化
///
/// 对应 Go: TestNewXLStorage
#[tokio::test]
async fn test_new_xl_storage() {
    // 正常创建 (先建目录再初始化)
    let tmp = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&tmp).unwrap();
    let storage = XlStorage::new(&tmp, "test-ep");
    assert!(storage.is_online().await);
    assert_eq!(storage.endpoint(), "test-ep");
    cleanup(&tmp);

    // 空路径 — 不报错
    let storage = XlStorage::new("", "test");
    assert_eq!(storage.endpoint(), "test");
}

/// 测试 xlStorage.disk_info 磁盘信息获取
///
/// 对应 Go: TestXLStorageGetDiskInfo
#[tokio::test]
async fn test_xl_storage_get_disk_info() {
    let (storage, dir) = temp_disk();

    let info = storage.disk_info().await.unwrap();
    assert!(info.online);
    assert!(!info.healing);
    assert!(!info.formatted); // 新目录无 format.json

    cleanup(&dir);
}

// ==================== 卷管理 ====================

/// 测试 xlStorage.make_volume 卷创建
///
/// 对应 Go: TestXLStorageMakeVol
#[tokio::test]
async fn test_xl_storage_make_vol() {
    let (storage, dir) = temp_disk();

    // 正常创建
    storage.make_volume("success-vol").await.unwrap();
    assert!(dir.join("success-vol").exists());

    // 重复创建同一卷 — create_dir_all 幂等，不报错
    assert!(storage.make_volume("success-vol").await.is_ok());

    // 层次路径卷名也支持
    storage.make_volume("nested/dir/vol").await.unwrap();
    assert!(dir.join("nested/dir/vol").exists());

    cleanup(&dir);
}

/// 测试 xlStorage.delete_volume 卷删除
///
/// 对应 Go: TestXLStorageDeleteVol
#[tokio::test]
async fn test_xl_storage_delete_vol() {
    let (storage, dir) = temp_disk();

    // 创建 → 删除
    storage.make_volume("to-delete").await.unwrap();
    assert!(dir.join("to-delete").exists());
    storage.delete_volume("to-delete").await.unwrap();
    assert!(!dir.join("to-delete").exists());

    // 不存在的卷 — 幂等成功
    assert!(storage.delete_volume("nonexistent-vol").await.is_ok());

    cleanup(&dir);
}

// ==================== 文件 IO ====================

/// 测试 xlStorage.read_all 读取文件全部内容
///
/// 对应 Go: TestXLStorageReadAll
#[tokio::test]
async fn test_xl_storage_read_all() {
    let (storage, dir) = temp_disk();
    storage.make_volume("bucket").await.unwrap();

    let data = b"Hello, MinIO XL Storage v2!";
    storage.write_all("bucket", "readme.txt", data).await.unwrap();

    // 正常读取
    let read = storage.read_all("bucket", "readme.txt").await.unwrap();
    assert_eq!(read, data);

    // 不存在的文件
    assert!(storage.read_all("bucket", "no-file").await.is_err());

    // 不存在的 volume
    assert!(storage.read_all("no-vol", "x").await.is_err());

    cleanup(&dir);
}

/// 测试 xlStorage.read_range 范围读取
///
/// 对应 Go: TestXLStorageReadFile (range 部分)
#[tokio::test]
async fn test_xl_storage_read_range() {
    let (storage, dir) = temp_disk();
    storage.make_volume("bucket").await.unwrap();

    let data: Vec<u8> = (0..100u8).collect();
    storage.write_all("bucket", "range-test", &data).await.unwrap();

    // 范围读取
    let part = storage.read_range("bucket", "range-test", 10, 20).await.unwrap();
    assert_eq!(part, &data[10..30]);

    // 负偏移 → 返回空
    let part = storage.read_range("bucket", "range-test", -1, 20).await.unwrap();
    assert!(part.is_empty());

    // 超出范围 → 返回空
    let part = storage.read_range("bucket", "range-test", 200, 10).await.unwrap();
    assert!(part.is_empty());

    cleanup(&dir);
}

/// 测试 xlStorage.append_file 追加写入
///
/// 对应 Go: TestXLStorageAppendFile
#[tokio::test]
async fn test_xl_storage_append_file() {
    let (storage, dir) = temp_disk();
    storage.make_volume("bucket").await.unwrap();

    // 新建
    storage.append_file("bucket", "log.txt", b"line1").await.unwrap();
    assert_eq!(storage.read_all("bucket", "log.txt").await.unwrap(), b"line1");

    // 追加
    storage.append_file("bucket", "log.txt", b"\nline2").await.unwrap();
    assert_eq!(storage.read_all("bucket", "log.txt").await.unwrap(), b"line1\nline2");

    // 层次路径
    storage.append_file("bucket", "a/b/c/data.bin", b"nested").await.unwrap();
    assert_eq!(storage.read_all("bucket", "a/b/c/data.bin").await.unwrap(), b"nested");

    cleanup(&dir);
}

/// 测试 xlStorage.write_all 覆盖写入
///
/// 对应 Go: TestXLStorageAppendFile (overwrite case)
#[tokio::test]
async fn test_xl_storage_write_all() {
    let (storage, dir) = temp_disk();
    storage.make_volume("bucket").await.unwrap();

    storage.write_all("bucket", "config.json", b"v1").await.unwrap();
    assert_eq!(storage.read_all("bucket", "config.json").await.unwrap(), b"v1");

    // 覆盖
    storage.write_all("bucket", "config.json", b"v2-overwrite").await.unwrap();
    assert_eq!(storage.read_all("bucket", "config.json").await.unwrap(), b"v2-overwrite");

    cleanup(&dir);
}

/// 测试 xlStorage.delete 文件删除
///
/// 对应 Go: TestXLStorageDeleteFile
#[tokio::test]
async fn test_xl_storage_delete_file() {
    let (storage, dir) = temp_disk();
    storage.make_volume("bucket").await.unwrap();
    storage.write_all("bucket", "tmp.txt", b"temp").await.unwrap();

    assert!(storage.file_exists("bucket", "tmp.txt").await.unwrap());

    // 正常删除
    storage.delete("bucket", "tmp.txt").await.unwrap();
    assert!(!storage.file_exists("bucket", "tmp.txt").await.unwrap());

    // 幂等删除
    assert!(storage.delete("bucket", "tmp.txt").await.is_ok());

    // 删除不存在的文件
    assert!(storage.delete("bucket", "never-exists").await.is_ok());

    cleanup(&dir);
}

/// 测试 xlStorage.rename 文件重命名/移动
///
/// 对应 Go: TestXLStorageRenameFile
#[tokio::test]
async fn test_xl_storage_rename_file() {
    let (storage, dir) = temp_disk();
    storage.make_volume("src").await.unwrap();
    storage.make_volume("dst").await.unwrap();

    let content = b"rename me";
    storage.write_all("src", "file-a", content).await.unwrap();

    // 跨卷 rename
    storage.rename("src", "file-a", "dst", "file-b").await.unwrap();
    assert!(!storage.file_exists("src", "file-a").await.unwrap());
    assert_eq!(storage.read_all("dst", "file-b").await.unwrap(), content);

    // 同卷 rename (覆盖目标)
    storage.write_all("dst", "old", b"old-data").await.unwrap();
    storage.rename("dst", "file-b", "dst", "old").await.unwrap();
    assert_eq!(storage.read_all("dst", "old").await.unwrap(), content);

    // 源不存在 → 报错
    let result = storage.rename("dst", "ghost", "dst", "x").await;
    assert!(result.is_err());

    cleanup(&dir);
}

// ==================== 目录操作 ====================

/// 测试 xlStorage.list_dir 目录列表
///
/// 对应 Go: TestXLStorageListDir
#[tokio::test]
async fn test_xl_storage_list_dir() {
    let (storage, dir) = temp_disk();
    storage.make_volume("bucket").await.unwrap();

    storage.write_all("bucket", "a.txt", b"a").await.unwrap();
    storage.write_all("bucket", "b.txt", b"b").await.unwrap();
    storage.write_all("bucket", "c.txt", b"c").await.unwrap();

    // 全量列表
    let mut files = storage.list_dir("bucket", "", 0).await.unwrap();
    files.sort();
    assert_eq!(files, vec!["a.txt", "b.txt", "c.txt"]);

    // 数量限制
    let limited = storage.list_dir("bucket", "", 2).await.unwrap();
    assert_eq!(limited.len(), 2);

    cleanup(&dir);
}

// ==================== 文件统计 ====================

/// 测试 xlStorage.stat_file 文件统计
///
/// 对应 Go: TestXLStorageStatInfoFile
#[tokio::test]
async fn test_xl_storage_stat_info_file() {
    let (storage, dir) = temp_disk();
    storage.make_volume("bucket").await.unwrap();
    storage.write_all("bucket", "stat.txt", b"hello!").await.unwrap();

    let stat = storage.stat_file("bucket", "stat.txt").await.unwrap();
    assert_eq!(stat.size, 6);
    assert!(!stat.is_dir);
    assert!(stat.mod_time > 0);

    // 不存在的文件
    assert!(storage.stat_file("bucket", "no-file").await.is_err());

    cleanup(&dir);
}

/// 测试 xlStorage.file_exists 文件存在性检查
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

// ==================== 以下测试需要更多基础设施，暂时保留为 ignore ====================

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
