//! Disk info retrieval tests
//!
//! Tests the get_disk_info function, verifying the returned disk
//! info contains a valid FSType.

use storage::*;

/// Tests get_info for disk info retrieval
///
/// Scenarios:
/// - Call get_info on a temp directory
/// - Verify FSType is not "UNKNOWN"
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
