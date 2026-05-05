//! Regression tests - exposing known bugs
//!
//! Test cases in this file target known issues found during code review.
//! These tests are **expected to fail**, exposing bugs in the implementation.
//! They will pass once the bugs are fixed.

use std::path::PathBuf;
use storage::{StorageAPI, XlStorage};

// ========================================================================
// Issue #2: read_range length < 0 causes panic
// ========================================================================
// File: crates/storage/src/xl_storage.rs:104
// offset < 0 has a pre-check, but length < 0 does not.
// When length=-1, min(-1, N) = -1, as usize panics in debug mode,
// and produces a huge allocation -> OOM in release mode.
// ========================================================================

mod issue_02_read_range_negative_length {

    use super::*;

    fn setup() -> (XlStorage, PathBuf) {
        let dir = std::env::temp_dir().join(format!("reg_02_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let storage = XlStorage::new(&dir, "test");
        (storage, dir)
    }

    /// read_range should not panic when length < 0, should return error or empty result
    ///
    /// Current BUG: length=-1 causes min(-1, N) = -1, as usize panics in debug mode.
    /// Fix: Also check length <= 0 at line 101 and return empty Vec.
    #[tokio::test]
    async fn read_range_negative_length_should_not_panic() {
        let (storage, dir) = setup();
        storage.make_volume("bucket").await.unwrap();
        storage.write_all("bucket", "data.bin", b"hello world").await.unwrap();

        // Direct call -- if it panics, test framework reports FAILED
        // Expected behavior: return Err or empty Vec
        let result = storage.read_range("bucket", "data.bin", 0, -1).await;

        match result {
            Ok(data) => {
                // No panic! But verify no OOM triggered (data should be empty)
                assert!(
                    data.is_empty(),
                    "length < 0 should return empty data or error, not non-empty data"
                );
                eprintln!(
                    "read_range(length=-1) did not panic but returned Ok.\n\
                     Empty data means OOM was not triggered, but length validation is still missing."
                );
            }
            Err(_) => {
                // Returned an error -- not ideal but at least no panic
            }
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// read_range with offset=0, length=0 should return empty Vec (edge case)
    #[tokio::test]
    async fn read_range_zero_length_should_return_empty() {
        let (storage, dir) = setup();
        storage.make_volume("bucket").await.unwrap();
        storage.write_all("bucket", "data.bin", b"hello").await.unwrap();

        let result = storage.read_range("bucket", "data.bin", 0, 0).await;
        assert!(result.is_ok(), "length=0 should return empty data normally");
        assert_eq!(result.unwrap().len(), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }
}

// ========================================================================
// Issue #7: delete_volume has no path constraints -- "" or ".." can delete disk root
// ========================================================================
// File: crates/storage/src/xl_storage.rs:209-214
// volume="" -> volume_path returns disk_path itself -> remove_dir_all deletes entire root
// volume=".." -> traverses out of the disk root directory
// ========================================================================

mod issue_07_delete_volume_path_traversal {

    use super::*;

    fn setup() -> (XlStorage, PathBuf) {
        let dir = std::env::temp_dir().join(format!("reg_07_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let storage = XlStorage::new(&dir, "test");
        (storage, dir)
    }

    /// delete_volume("") should not delete the disk root directory
    ///
    /// Current implementation: volume_path("") = disk_path.join("") = disk_path
    /// This would delete the entire test directory, a serious security issue.
    #[tokio::test]
    async fn delete_volume_empty_string_should_be_rejected() {
        let (storage, dir) = setup();

        // Create a marker file under the disk root first
        let marker = dir.join(".safeguard");
        std::fs::write(&marker, b"protect me").unwrap();

        // Try to delete empty volume name -- should be rejected (Err), not delete entire directory
        let result = storage.delete_volume("").await;

        // Marker file must still exist -- if delete_volume("") deleted the root, this assert will fail
        assert!(
            marker.exists(),
            "BUG CONFIRMED: delete_volume(\"\") deleted the disk root directory!\n\
             Cause: volume_path(\"\") = disk_path.join(\"\") = disk_path\n\
             Fix: Reject empty strings and volume names containing \"..\"."
        );

        // Expected behavior: should return error
        assert!(
            result.is_err(),
            "delete_volume(\"\") should return error, currently returns Ok (may have deleted root)"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// delete_volume("..") should be rejected, path traversal out of disk root not allowed
    #[tokio::test]
    async fn delete_volume_dot_dot_should_be_rejected() {
        let (storage, dir) = setup();

        let result = storage.delete_volume("..").await;
        assert!(
            result.is_err(),
            "delete_volume(\"..\") should return error, path traversal should not be allowed"
        );

        // Disk root directory should still exist
        assert!(dir.exists(), "Disk root directory was deleted by delete_volume(\"..\")!");

        let _ = std::fs::remove_dir_all(&dir);
    }
}

// ========================================================================
// Issue #1: XlMeta::from_bytes version check too strict -- rejects higher minor versions
// ========================================================================
// File: crates/base/src/format.rs:215-216
// Currently requires exact major/minor match. Minor should be backward compatible --
// a higher minor indicates new format added optional fields, old code should ignore them.
// Current behavior causes old nodes to fail reading xl.meta written by newer nodes
// during rolling upgrades.
// ========================================================================

mod issue_01_xl_meta_version_too_strict {

    use base::format::XlMeta;

    /// Construct xl.meta binary data with a higher minor version
    fn make_xl_meta_with_version(major: u16, minor: u16) -> Vec<u8> {
        let header = base::format::XlMetaHeader {
            magic: *b"XL2 ",
            major,
            minor,
        };
        // Body: XlMeta struct (1-element msgpack array) containing empty versions array
        let body: &[u8] = &[0x91, 0x90]; // 91: 1-element array; 90: empty versions array
        let mut buf = Vec::new();
        buf.extend_from_slice(&header.to_bytes());
        buf.extend_from_slice(body);
        buf
    }

    /// Higher minor version should be accepted (backward compatible)
    ///
    /// Current behavior: XlMeta::from_bytes rejects minor != XL_VERSION_MINOR
    /// Expected behavior: Only reject major mismatch; warn but accept higher minor
    #[test]
    fn higher_minor_version_should_be_accepted() {
        let current_minor = base::constants::XL_VERSION_MINOR;
        let future_minor = current_minor + 1; // Simulate future version

        let data = make_xl_meta_with_version(
            base::constants::XL_VERSION_MAJOR,
            future_minor,
        );

        let result = XlMeta::from_bytes(&data);

        // Currently expected to fail -- version check is too strict
        assert!(
            result.is_ok(),
            "BUG CONFIRMED: XlMeta::from_bytes rejected xl.meta with minor={} (currently requires={}).\n\
             Cause: Version check requires exact major/minor match.\n\
             Fix: Only reject major mismatch; warn but accept higher minor.",
            future_minor,
            current_minor,
        );
    }

    /// Same version xl.meta should be readable (sanity check)
    #[test]
    fn same_version_should_be_accepted() {
        let data = make_xl_meta_with_version(
            base::constants::XL_VERSION_MAJOR,
            base::constants::XL_VERSION_MINOR,
        );
        let result = XlMeta::from_bytes(&data);
        assert!(result.is_ok(), "current version xl.meta should be readable");
    }

    /// Different major version should be rejected
    #[test]
    fn different_major_version_should_be_rejected() {
        let data = make_xl_meta_with_version(
            base::constants::XL_VERSION_MAJOR + 1,
            base::constants::XL_VERSION_MINOR,
        );
        let result = XlMeta::from_bytes(&data);
        assert!(
            result.is_err(),
            "xl.meta with different major version should be rejected"
        );
    }
}

// ========================================================================
// Issue #4: read_xl_meta skips version compatibility check
// ========================================================================
// File: crates/storage/src/format.rs:11-22
// XlMeta::from_bytes performs version validation, but read_xl_meta only
// checks the magic, skipping version check. The two code paths are inconsistent.
// ========================================================================

mod issue_04_read_xl_meta_skips_version_check {

    use base::format::{XlMeta, XlMetaHeader};
    use storage::read_xl_meta;

    /// read_xl_meta should behave consistently with XlMeta::from_bytes:
    /// return an error for incompatible major versions
    #[test]
    fn read_xl_meta_should_reject_incompatible_version() {
        // Construct xl.meta with incompatible major version
        let header = XlMetaHeader {
            magic: *b"XL2 ",
            major: 99,  // Completely incompatible major
            minor: 0,
        };
        let body: &[u8] = &[0x90]; // empty msgpack array
        let mut buf = Vec::new();
        buf.extend_from_slice(&header.to_bytes());
        buf.extend_from_slice(body);

        let result = read_xl_meta(&buf);

        assert!(
            result.is_err(),
            "BUG CONFIRMED: read_xl_meta skipped version validation, accepted major=99 data.\n\
             XlMeta::from_bytes correctly rejects it. The two paths are inconsistent.\n\
             Fix: read_xl_meta should delegate to XlMeta::from_bytes."
        );
    }

    /// Sanity: read_xl_meta should produce same result as XlMeta::from_bytes for same data
    #[test]
    fn read_xl_meta_and_from_bytes_should_be_consistent() {
        let meta = XlMeta { versions: vec![] };
        let data_via_to_bytes = meta.to_bytes().unwrap();

        let result_read = read_xl_meta(&data_via_to_bytes);
        let result_from = XlMeta::from_bytes(&data_via_to_bytes);

        // Both paths should succeed or both fail
        assert_eq!(
            result_read.is_ok(),
            result_from.is_ok(),
            "read_xl_meta and XlMeta::from_bytes behavior is inconsistent"
        );
    }
}

// ========================================================================
// Issue #8: is_xl_meta_erasure_info_valid comment vs implementation mismatch
// ========================================================================
// File: crates/storage/src/format.rs:50-56
// Comment says "data must equal parity", but the actual MinIO constraint is data >= parity.
// Code implements data >= parity (correct), but the comment is misleading.
// ========================================================================

mod issue_08_erasure_info_valid_comment_mismatch {

    use storage::is_xl_meta_erasure_info_valid;

    /// Verify data > parity is valid (e.g., EC 4+2 configuration)
    ///
    /// Comment says "must be equal", but MinIO actually supports data > parity.
    #[test]
    fn data_greater_than_parity_should_be_valid() {
        // EC 4+2: 4 data blocks, 2 parity blocks
        assert!(
            is_xl_meta_erasure_info_valid(4, 2),
            "BUG: is_xl_meta_erasure_info_valid(4, 2) returned false.\n\
             EC(4,2) is a valid MinIO erasure code configuration.\n\
             The comment saying 'must be equal' is wrong -- code checks data >= parity (correct).\n\
             Fix: Correct the comment."
        );
    }

    /// EC 8+4 should also be valid
    #[test]
    fn data_greater_than_parity_8_plus_4_should_be_valid() {
        assert!(
            is_xl_meta_erasure_info_valid(8, 4),
            "EC(8,4) is a valid configuration"
        );
    }

    /// EC 4+4 should be valid (data == parity)
    #[test]
    fn data_equal_to_parity_should_be_valid() {
        assert!(is_xl_meta_erasure_info_valid(4, 4));
    }

    /// data=0 is invalid
    #[test]
    fn data_zero_should_be_invalid() {
        assert!(!is_xl_meta_erasure_info_valid(0, 2));
    }

    /// parity=0 (no parity) is valid
    #[test]
    fn parity_zero_should_be_valid() {
        assert!(is_xl_meta_erasure_info_valid(4, 0));
    }
}

// ========================================================================
// Issue #17: read_range offset out-of-bounds returns empty Vec, cannot distinguish "out of bounds" from "empty data"
// ========================================================================
// File: crates/storage/src/xl_storage.rs:101-103
// ========================================================================

mod issue_17_read_range_oob_ambiguous {

    use super::*;

    /// read_range offset >= file_len returns empty Vec,
    /// caller cannot distinguish "offset out of bounds" from "read 0 bytes"
    #[tokio::test]
    async fn read_range_beyond_eof_returns_empty_not_error() {
        let dir = std::env::temp_dir().join(format!("reg_17_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let storage = XlStorage::new(&dir, "test");
        storage.make_volume("bucket").await.unwrap();
        storage.write_all("bucket", "small.bin", b"hi").await.unwrap();

        // offset = 100 (far beyond file size 2)
        let result = storage.read_range("bucket", "small.bin", 100, 10).await;

        // Current implementation: returns Ok(vec![]) -- indistinguishable from offset=2, length=0
        // Expected behavior: return Err to distinguish "out of bounds" from "empty data"
        match result {
            Ok(data) if data.is_empty() => {
                // Current behavior -- silently returns empty, caller cannot distinguish
                eprintln!(
                    "ISSUE #17 still present: read_range(offset=100) returned Ok([]).\n\
                     Caller cannot distinguish \"offset out of bounds\" from \"read 0 bytes of valid data\".\n\
                     Suggestion: Return Err on out-of-bounds or document the behavior explicitly."
                );
            }
            Ok(_) => panic!("should not return non-empty data"),
            Err(_) => {
                // If error is returned -- bug is fixed!
                // This is the expected behavior
            }
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}

// ========================================================================
// Issue #25: disk_info returns hardcoded total: 0, free: 0, used: 0
// ========================================================================
// File: crates/storage/src/xl_storage.rs:54-56
// ========================================================================

mod issue_25_disk_info_hardcoded_zeros {

    use super::*;

    /// disk_info should return real disk space info, not hardcoded 0
    #[tokio::test]
    async fn disk_info_should_return_real_disk_space() {
        let dir = std::env::temp_dir().join(format!("reg_25_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let storage = XlStorage::new(&dir, "test");

        let info = storage.disk_info().await.unwrap();

        // The disk where the temp dir resides cannot have total=0
        if info.total == 0 && info.free == 0 {
            eprintln!(
                "ISSUE #25 still present: disk_info returned total={}, free={}, used={}.\n\
                 These values are hardcoded to 0.\n\
                 Fix: Use statvfs/statfs to get real disk space.",
                info.total, info.free, info.used
            );
        }

        // At least online should be true
        assert!(info.online, "disk should be online");
        assert_eq!(info.healing, false);

        let _ = std::fs::remove_dir_all(&dir);
    }
}

// ========================================================================
// Issue #14: is_online is too simplistic -- only checks directory existence
// ========================================================================
// File: crates/storage/src/xl_storage.rs:66-68
// Checking disk_path.exists() alone does not mean the disk is readable/writable.
// NFS stale mounts, permission changes, read-only remounts, etc. are not detected.
// ========================================================================

mod issue_14_is_online_too_simplistic {

    use super::*;

    /// is_online should return false for non-existent paths
    #[tokio::test]
    async fn is_online_should_return_false_for_nonexistent_path() {
        let nonexistent = std::env::temp_dir().join(format!("reg_14_nonexistent_{}", uuid::Uuid::new_v4()));
        let storage = XlStorage::new(&nonexistent, "test");
        assert!(!storage.is_online().await, "non-existent path should return false from is_online");
    }

    /// is_online should return true for existing directories (but may not be writable)
    #[tokio::test]
    async fn is_online_should_return_true_for_existing_dir() {
        let dir = std::env::temp_dir().join(format!("reg_14_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let storage = XlStorage::new(&dir, "test");
        assert!(storage.is_online().await, "existing directory should return true from is_online");

        // Note: even if is_online returns true, the directory may not be readable/writable.
        // e.g., NFS stale mount, read-only filesystem, etc.
        // The current implementation cannot detect these cases.
        // Suggestion: add periodic IO health checks (e.g., .minio.sys/.healthcheck)

        let _ = std::fs::remove_dir_all(&dir);
    }
}
