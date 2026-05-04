//! xlStorage Unix 特定测试
//!
//! 对应 Go: cmd/xl-storage_unix_test.go
//!
//! 验证 Unix 系统上目录和文件创建时 umask 是否正确。
//! 仅适用于 linux/darwin/freebsd 等 Unix 系系统。

use storage::*;

/// 测试 MakeVol 创建的目录使用正确的 umask
///
/// 场景:
/// - MakeVol 使用 0777 权限创建目录
/// - 实际权限应为 0777 & ^umask
///
/// 对应 Go: TestIsValidUmaskVol
#[test]
#[ignore]
fn test_is_valid_umask_vol() {
    // TODO: implement when xlStorage and MakeVol are available; Unix-only test
    // let tmp = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
    // let disk = new_local_xl_storage(tmp.to_str().unwrap()).unwrap();
    // disk.make_vol("is-this-valid").await?;
    //
    // let metadata = std::fs::metadata(tmp.join("is-this-valid")).unwrap();
    // let current_umask = 0o777 - (metadata.permissions().mode() & 0o777);
    // let expected_umask = get_umask();
    // assert_eq!(current_umask, expected_umask,
    //     "umask check failed expected {:o}, got {:o}", expected_umask, current_umask);
}

/// 测试 AppendFile 创建的文件使用正确的 umask
///
/// 场景:
/// - AppendFile 使用 0666 权限创建文件
/// - 实际权限应为 0666 & ^umask
///
/// 对应 Go: TestIsValidUmaskFile
#[test]
#[ignore]
fn test_is_valid_umask_file() {
    // TODO: implement when xlStorage, MakeVol, AppendFile, StatInfoFile are available; Unix-only test
    // let tmp = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
    // let disk = new_local_xl_storage(tmp.to_str().unwrap()).unwrap();
    // disk.make_vol("is-this-valid").await?;
    // disk.append_file("is-this-valid", "hello-world.txt/xl.meta", b"Hello World").await?;
    //
    // let result = disk.stat_info_file("is-this-valid", "hello-world.txt/xl.meta", false).await;
    // assert!(result.is_ok(), "Stat failed: {:?}", result);
}
