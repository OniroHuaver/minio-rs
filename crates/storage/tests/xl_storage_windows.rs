//! xlStorage Windows 特定测试
//!
//! 对应 Go: cmd/xl-storage_windows_test.go
//!
//! 验证 Windows UNC 路径处理和 ENOTDIR 行为。
//! 这些测试仅在 Windows 平台有意义。

use storage::*;

/// 测试 UNC 路径格式下的各种路径是否正常工作
///
/// 场景:
/// - 正常路径 → AppendFile 应成功
/// - 各段长度 ≤ 255 的路径 → 应成功
/// - 含超长路径段 (> 255 字节) → 应失败
///
/// 对应 Go: TestUNCPaths
#[test]
#[ignore]
fn test_unc_paths() {
    // TODO: implement when xlStorage and AppendFile are available; Windows-only test
    // let tmp = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
    // let storage = new_local_xl_storage(tmp.to_str().unwrap()).unwrap();
    // storage.make_vol("voldir").await?;
    //
    // // Normal path
    // storage.append_file("voldir", "/abcdef", b"hello").await?;
    //
    // // Long path segment (>255 bytes in UTF-8)
    // let long_segment = "\u{754c}".repeat(280);
    // let result = storage.append_file("voldir", &format!("/{}", long_segment), b"hello").await;
    // assert!(result.is_err(), "Expected error for long path segment");
}

/// 测试非叶子路径为文件时的 ENOTDIR 处理
///
/// 场景:
/// - 创建文件 "/file"
/// - 尝试创建 "/file/obj1" (/file 是文件而非目录) → errFileAccessDenied
///
/// 对应 Go: TestUNCPathENOTDIR
#[test]
#[ignore]
fn test_unc_path_enotdir() {
    // TODO: implement when xlStorage and AppendFile are available; Windows-only test
    // let tmp = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
    // let storage = new_local_xl_storage(tmp.to_str().unwrap()).unwrap();
    // storage.make_vol("voldir").await?;
    // storage.append_file("voldir", "/file", b"hello").await?;
    //
    // // /file is a file, not a directory — should fail
    // let result = storage.append_file("voldir", "/file/obj1", b"hello").await;
    // assert_eq!(result.unwrap_err(), Error::FileAccessDenied);
}
