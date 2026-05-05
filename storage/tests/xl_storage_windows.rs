//! xlStorage Windows-specific tests
//!
//! Verifies Windows UNC path handling and ENOTDIR behavior.
//! These tests are only meaningful on Windows platforms.


/// Tests UNC path format handling
///
/// Scenarios:
/// - Normal path -> AppendFile should succeed
/// - Path with segments <= 255 bytes -> should succeed
/// - Path with overly long segment (> 255 bytes) -> should fail
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

/// Tests ENOTDIR handling when non-leaf path is a file
///
/// Scenarios:
/// - Create file "/file"
/// - Try to create "/file/obj1" (/file is a file, not a directory) -> errFileAccessDenied
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
