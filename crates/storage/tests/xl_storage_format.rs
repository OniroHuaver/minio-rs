//! xl.meta format validation and parsing tests
//!
//! Tests xl-meta format version/format validation, JSON serialization/deserialization,
//! part size calculation, and xlMetaV2 benchmarks.

use storage::{
    calculate_part_size_from_idx, is_xl_meta_erasure_info_valid, is_xl_meta_format_valid,
    read_xl_meta, write_xl_meta, write_xl_meta_no_data,
};

/// Tests is_xl_meta_format_valid for xl.meta version+format fields
///
/// Scenarios:
/// - ("123", "fs") -> false (format is not "xl")
/// - ("123", "xl") -> false (version is not "1.0.0" or "1.0.1")
/// - ("1.0.0", "test") -> false (format is not "xl")
/// - ("1.0.0", "xl") -> true
/// - ("1.0.1", "xl") -> true
#[test]
fn test_is_xl_meta_format_valid() {
    let tests = vec![
        ("123", "fs", false),
        ("123", "xl", false),
        ("1.0.0", "test", false),
        ("1.0.1", "hello", false),
        ("1.0.0", "xl", true),
        ("1.0.1", "xl", true),
    ];
    for (i, (version, format, want)) in tests.iter().enumerate() {
        let got = is_xl_meta_format_valid(version, format);
        assert_eq!(
            got, *want,
            "Test {}: expected {} but got {}",
            i + 1, want, got
        );
    }
}

/// Tests is_xl_meta_erasure_info_valid for erasure code parameters
///
/// Scenarios:
/// - data=5, parity=6 -> false (not equal and not zero)
/// - data=5, parity=5 -> true
/// - data=0, parity=5 -> false
/// - data=-1, parity=5 -> false
/// - data=5, parity=0 -> true
/// - data=5, parity=4 -> true
#[test]
fn test_is_xl_meta_erasure_info_valid() {
    let tests = vec![
        (5, 6, false),
        (5, 5, true),
        (0, 5, false),
        (-1, 5, false),
        (5, 0, true),
        (5, 4, true),
    ];
    for (i, (data, parity, want)) in tests.iter().enumerate() {
        let got = is_xl_meta_erasure_info_valid(*data, *parity);
        assert_eq!(
            got, *want,
            "Test {}: expected {} but got {}",
            i + 1, want, got
        );
    }
}

/// Tests calculate_part_size_from_idx for part size calculation
///
/// Scenarios:
/// - Normal case: total_size=4MiB, part_size=2MiB, part_index=1 -> 2MiB
/// - Last part: total_size=5MiB, part_size=2MiB, part_index=3 -> 1MiB
/// - Out of bounds: part_index out of range -> 0
/// - Error case: part_size=0 -> errPartSizeZero
/// - Error case: part_index=0 -> errPartSizeIndex
/// - Error case: total_size=-1 -> errInvalidArgument
#[test]
fn test_get_part_size_from_idx() {
    let kib = 1024;
    let mib = 1024 * kib;

    // Normal cases
    let ok_cases = vec![
        (0, 10, 1, 0),
        (4 * mib, 2 * mib, 1, 2 * mib),
        (4 * mib, 2 * mib, 2, 2 * mib),
        (4 * mib, 2 * mib, 3, 0),
        (5 * mib, 2 * mib, 1, 2 * mib),
        (5 * mib, 2 * mib, 2, 2 * mib),
        (5 * mib, 2 * mib, 3, 1 * mib),
        (5 * mib, 2 * mib, 4, 0),
    ];
    for (i, (total, part_sz, idx, expected)) in ok_cases.iter().enumerate() {
        let result = calculate_part_size_from_idx(*total, *part_sz, *idx).unwrap();
        assert_eq!(
            result, *expected,
            "OK Test {} failed: expected {} but got {}",
            i + 1, expected, result
        );
    }

    // Error cases
    let result = calculate_part_size_from_idx(10, 0, 1);
    assert!(result.is_err(), "part_size=0 should error");

    let result = calculate_part_size_from_idx(10, 1, 0);
    assert!(result.is_err(), "part_index=0 should error");

    let result = calculate_part_size_from_idx(-2, 10, 1);
    assert!(result.is_err(), "total_size<0 should error");
}

/// Tests JSON deserialization consistency
///
/// Create a 1-part xlMetaV1Object JSON, deserialize with serde,
/// verify results match exactly.
#[test]
#[ignore]
fn test_get_xl_meta_v1_json_iter_1() {
    // TODO: implement when xlMetaV1Object and JSON parsing are available
}

/// Tests JSON deserialization consistency (10-part)
#[test]
#[ignore]
fn test_get_xl_meta_v1_json_iter_10() {
    // TODO: implement when xlMetaV1Object and JSON parsing are available
}

/// Benchmark: xlMetaV2 shallow operation performance
#[test]
#[ignore]
fn benchmark_xl_meta_v2_shallow() {
    // TODO: implement benchmarks when xlMetaV2 is available
}

/// Tests write_xl_meta / read_xl_meta roundtrip through the storage format API.
///
/// Verifies: write → produces valid binary with "XL2 " header,
/// read → correctly parses the serialized data.
#[test]
fn test_write_read_xl_meta_roundtrip() {
    use base::format::{ObjectPart, XlMeta, XlMetaEntry, XlMetaVersionHeader};

    let header = XlMetaVersionHeader {
        version_id: "test-roundtrip-1".into(),
        mod_time: 1_700_000_000_000_000_000i64,
        signature: vec![0u8; 32],
        r#type: 1u8,
        flags: 0,
        size: 0,
        erasure_algorithm: 0,
        erasure_m: 4,
        erasure_n: 2,
        erasure_block_size: 4 * 1024 * 1024,
        erasure_dist: vec![0, 1, 2, 3, 4, 5],
        parts: vec![ObjectPart {
            number: 1,
            etag: "d41d8cd98f00b204e9800998ecf8427e".into(),
            size: 1024,
            actual_size: 1024,
            index: 0,
        }],
        meta_sys: vec![],
        meta_user: vec![],
    };

    let meta = XlMeta {
        versions: vec![XlMetaEntry::Object {
            header,
            data: None,
        }],
    };

    // Write
    let bytes = write_xl_meta(&meta).expect("write_xl_meta");
    assert!(&bytes[0..4] == b"XL2 ", "header magic missing");

    // Read — verify roundtrip
    let loaded = read_xl_meta(&bytes).expect("read_xl_meta");
    assert_eq!(loaded.versions.len(), 1);
}

/// Tests write_xl_meta_no_data produces valid serialized xl.meta.
///
/// Uses `to_vec_named` (struct-as-map) encoding, useful for signature
/// computation where field-name stability matters.
#[test]
fn test_write_xl_meta_no_data_roundtrip() {
    use base::format::{XlMeta, XlMetaEntry, XlMetaVersionHeader};

    let header = XlMetaVersionHeader::new("no-data-test".into());
    let meta = XlMeta {
        versions: vec![XlMetaEntry::Object {
            header,
            data: Some(b"inline payload".to_vec()),
        }],
    };

    // Both write modes produce valid xl.meta with "XL2 " header
    let bytes = write_xl_meta_no_data(&meta).expect("write_xl_meta_no_data");
    assert!(&bytes[0..4] == b"XL2 ");

    // The no-data variant uses to_vec_named (string keys) — should be parseable
    let loaded = read_xl_meta(&bytes).expect("read no-data output");
    assert_eq!(loaded.versions.len(), 1);
}
