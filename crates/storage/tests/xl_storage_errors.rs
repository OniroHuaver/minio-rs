//! 系统错误判断函数测试
//!
//! 对应 Go: cmd/xl-storage-errors_test.go
//!
//! 测试 is_sys_err_too_long, is_sys_err_not_dir,
//! is_sys_err_not_empty, is_sys_err_path_not_found 等函数。

use storage::*;

/// 测试系统错误类型判断函数
///
/// 验证:
/// - ENAMETOOLONG → is_sys_err_too_long 返回 true
/// - ENOTDIR → is_sys_err_not_dir 返回 true
/// - ENOTEMPTY → is_sys_err_not_empty 返回 true (Unix)
/// - Windows 特定错误码 → is_sys_err_not_empty 和 is_sys_err_path_not_found
///
/// 对应 Go: TestSysErrors
#[test]
#[ignore]
fn test_sys_errors() {
    // TODO: implement when error helper functions are available
    // // ENAMETOOLONG
    // let path_err = std::io::Error::from(std::io::ErrorKind::InvalidInput);
    // // In real test: os.PathError{Err: syscall.ENAMETOOLONG}
    // assert!(is_sys_err_too_long(&path_err));
    //
    // // ENOTDIR
    // assert!(is_sys_err_not_dir(&path_err));
    //
    // // ENOTEMPTY
    // assert!(is_sys_err_not_empty(&path_err));
    //
    // // Windows specific: is_sys_err_path_not_found
    // // assert!(is_sys_err_path_not_found(&path_err));
}
