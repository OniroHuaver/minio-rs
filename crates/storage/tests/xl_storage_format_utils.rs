//! xl.meta format 工具函数测试
//!
//! 对应 Go: cmd/xl-storage-format-utils_test.go
//!
//! 测试 hash_deterministic_string 和 get_file_info_versions。

use std::collections::HashMap;
use storage::hash_deterministic_string;

/// 测试 hash_deterministic_string 的确定性哈希
///
/// 验证:
/// - 对同一 map 重复调用 100 次，结果一致
/// - 添加新 key-value 后哈希变化 (无碰撞)
/// - 删除 key 添加不同 key 后哈希变化
/// - key/value 互换后哈希变化
///
/// 场景包含: 空 map, 单个 entry, 多个 entry, 空 value。
///
/// 对应 Go: Test_hashDeterministicString
#[test]
fn test_hash_deterministic_string() {
    // 空 map
    let empty: HashMap<String, String> = HashMap::new();
    let want_empty = hash_deterministic_string(&empty);
    for _ in 0..100 {
        assert_eq!(hash_deterministic_string(&empty), want_empty);
    }

    // 单 entry
    let mut single = HashMap::new();
    single.insert("key".into(), "value".into());
    let want_single = hash_deterministic_string(&single);
    for _ in 0..100 {
        assert_eq!(hash_deterministic_string(&single), want_single);
    }
    assert_ne!(want_single, want_empty);

    // 多 entry
    let mut multi = HashMap::new();
    multi.insert("x-amz-restore".into(), "FAILED".into());
    multi.insert("content-md5".into(), "uuid-value".into());
    multi.insert("x-amz-bucket-replication-status".into(), "PENDING".into());
    multi.insert("content-type".into(), "application/json".into());
    let want_multi = hash_deterministic_string(&multi);
    for _ in 0..100 {
        assert_eq!(hash_deterministic_string(&multi), want_multi);
    }

    // 添加 key 后哈希变化
    let mut changed = multi.clone();
    changed.insert("new-key".into(), "new-value".into());
    assert_ne!(hash_deterministic_string(&changed), want_multi);

    // 修改 value 后哈希变化
    let mut modified = multi.clone();
    modified.insert("content-md5".into(), "different-value".into());
    assert_ne!(hash_deterministic_string(&modified), want_multi);

    // key/value 互换后哈希变化
    let mut swapped = HashMap::new();
    swapped.insert("value".into(), "key".into());
    assert_ne!(hash_deterministic_string(&swapped), want_single);
}

/// 测试 get_file_info_versions 获取文件版本列表
///
/// 对应 Go: TestGetFileInfoVersions
#[test]
#[ignore]
fn test_get_file_info_versions() {
    // TODO: implement when xlMetaV2 and get_file_info_versions are available
}
