//! 磁盘信息获取测试
//!
//! 对应 Go: internal/disk/disk_test.go
//!
//! 测试 get_disk_info 函数，验证返回的磁盘信息包含有效的 FSType。

use storage::*;

/// 测试 get_info 获取磁盘信息
///
/// 场景:
/// - 对临时目录调用 get_info
/// - 验证 FSType 不是 "UNKNOWN"
///
/// 对应 Go: TestFree
#[test]
#[ignore]
fn test_free() {
    // TODO: implement when disk::get_info() is available
    // let tmp = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
    // std::fs::create_dir_all(&tmp).unwrap();
    //
    // let di = disk::get_info(tmp.to_str().unwrap(), true).unwrap();
    // assert_ne!(di.fs_type, "UNKNOWN", "Unexpected FSType {}", di.fs_type);
}
