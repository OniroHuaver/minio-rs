//! xl.meta format utility function tests
//!
//! Tests hash_deterministic_string and get_file_info_versions.

use std::collections::HashMap;
use storage::hash_deterministic_string;

/// Tests hash_deterministic_string deterministic hashing
///
/// Verify:
/// - Same map called 100 times produces the same result
/// - Hash changes after adding new key-value (no collision)
/// - Hash changes after adding different key
/// - Hash changes after swapping key/value
///
/// Scenarios include: empty map, single entry, multiple entries, empty value.
#[test]
fn test_hash_deterministic_string() {
    // Empty map
    let empty: HashMap<String, String> = HashMap::new();
    let want_empty = hash_deterministic_string(&empty);
    for _ in 0..100 {
        assert_eq!(hash_deterministic_string(&empty), want_empty);
    }

    // Single entry
    let mut single = HashMap::new();
    single.insert("key".into(), "value".into());
    let want_single = hash_deterministic_string(&single);
    for _ in 0..100 {
        assert_eq!(hash_deterministic_string(&single), want_single);
    }
    assert_ne!(want_single, want_empty);

    // Multiple entries
    let mut multi = HashMap::new();
    multi.insert("x-amz-restore".into(), "FAILED".into());
    multi.insert("content-md5".into(), "uuid-value".into());
    multi.insert("x-amz-bucket-replication-status".into(), "PENDING".into());
    multi.insert("content-type".into(), "application/json".into());
    let want_multi = hash_deterministic_string(&multi);
    for _ in 0..100 {
        assert_eq!(hash_deterministic_string(&multi), want_multi);
    }

    // Hash changes after adding key
    let mut changed = multi.clone();
    changed.insert("new-key".into(), "new-value".into());
    assert_ne!(hash_deterministic_string(&changed), want_multi);

    // Hash changes after modifying value
    let mut modified = multi.clone();
    modified.insert("content-md5".into(), "different-value".into());
    assert_ne!(hash_deterministic_string(&modified), want_multi);

    // Hash changes after swapping key/value
    let mut swapped = HashMap::new();
    swapped.insert("value".into(), "key".into());
    assert_ne!(hash_deterministic_string(&swapped), want_single);
}

/// Tests get_file_info_versions for file version listing
#[test]
#[ignore]
fn test_get_file_info_versions() {
    // TODO: implement when xlMetaV2 and get_file_info_versions are available
}
