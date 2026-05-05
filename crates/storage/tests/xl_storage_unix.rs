//! xlStorage Unix-specific tests
//!
//! Verifies correct umask on directory and file creation on Unix systems.
//! Only applicable to Linux, macOS, FreeBSD, and other Unix-like systems.

use storage::*;

/// Tests MakeVol creates directories with correct umask
///
/// Scenarios:
/// - MakeVol creates directory with 0777 permissions
/// - Actual permissions should be 0777 & ^umask
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

/// Tests AppendFile creates files with correct umask
///
/// Scenarios:
/// - AppendFile creates file with 0666 permissions
/// - Actual permissions should be 0666 & ^umask
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
