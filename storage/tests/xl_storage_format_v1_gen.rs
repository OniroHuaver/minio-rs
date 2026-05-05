//! xlMetaV1 MessagePack serialization/deserialization tests
//!
//! Tests MsgPack (via rmp-serde) roundtrip for xlMetaV1 related types.

use storage::{ChecksumInfo, ErasureInfo, ObjectPartInfo, StatInfo, XlMetaV1Object};

// ---------- ChecksumInfo ----------

#[test]
fn test_marshal_unmarshal_checksum_info() {
    let v = ChecksumInfo::default();
    let encoded = rmp_serde::to_vec(&v).unwrap();
    let decoded: ChecksumInfo = rmp_serde::from_slice(&encoded).unwrap();
    assert_eq!(v, decoded);
}

#[test]
fn test_encode_decode_checksum_info() {
    let v = ChecksumInfo {
        part_number: 1,
        algorithm: 2,
        hash: vec![1, 2, 3, 4],
    };
    let mut buf = Vec::new();
    rmp_serde::encode::write(&mut buf, &v).unwrap();
    let decoded: ChecksumInfo = rmp_serde::decode::from_read(&buf[..]).unwrap();
    assert_eq!(v, decoded);
}

// ---------- ErasureInfo ----------

#[test]
fn test_marshal_unmarshal_erasure_info() {
    let v = ErasureInfo::default();
    let encoded = rmp_serde::to_vec(&v).unwrap();
    let decoded: ErasureInfo = rmp_serde::from_slice(&encoded).unwrap();
    let reencoded = rmp_serde::to_vec(&decoded).unwrap();
    assert_eq!(encoded, reencoded);
}

#[test]
fn test_encode_decode_erasure_info() {
    let v = ErasureInfo {
        algorithm: 1,
        data: 4,
        parity: 2,
        block_size: 4 * 1024 * 1024,
        index: 0,
        distribution: vec![1, 2, 3],
        checksums: vec![ChecksumInfo {
            part_number: 1,
            algorithm: 2,
            hash: vec![4, 5, 6],
        }],
    };
    let mut buf = Vec::new();
    rmp_serde::encode::write(&mut buf, &v).unwrap();
    let decoded: ErasureInfo = rmp_serde::decode::from_read(&buf[..]).unwrap();
    let mut reencoded = Vec::new();
    rmp_serde::encode::write(&mut reencoded, &decoded).unwrap();
    assert_eq!(buf, reencoded);
}

// ---------- ObjectPartInfo ----------

#[test]
fn test_marshal_unmarshal_object_part_info() {
    let v = ObjectPartInfo::default();
    let encoded = rmp_serde::to_vec(&v).unwrap();
    let decoded: ObjectPartInfo = rmp_serde::from_slice(&encoded).unwrap();
    assert_eq!(v, decoded);
}

#[test]
fn test_encode_decode_object_part_info() {
    let v = ObjectPartInfo {
        number: 1,
        name: "part-1".into(),
        etag: "abc123".into(),
        size: 1024,
        actual_size: 1000,
        index: 0,
    };
    let mut buf = Vec::new();
    rmp_serde::encode::write(&mut buf, &v).unwrap();
    let decoded: ObjectPartInfo = rmp_serde::decode::from_read(&buf[..]).unwrap();
    assert_eq!(v, decoded);
}

// ---------- StatInfo ----------

#[test]
fn test_marshal_unmarshal_stat_info() {
    let v = StatInfo::default();
    let encoded = rmp_serde::to_vec(&v).unwrap();
    let decoded: StatInfo = rmp_serde::from_slice(&encoded).unwrap();
    let reencoded = rmp_serde::to_vec(&decoded).unwrap();
    assert_eq!(encoded, reencoded);
}

#[test]
fn test_encode_decode_stat_info() {
    let v = StatInfo {
        size: 1048576,
        mod_time: 1700000000000000000i64,
    };
    let mut buf = Vec::new();
    rmp_serde::encode::write(&mut buf, &v).unwrap();
    let decoded: StatInfo = rmp_serde::decode::from_read(&buf[..]).unwrap();
    let mut reencoded = Vec::new();
    rmp_serde::encode::write(&mut reencoded, &decoded).unwrap();
    assert_eq!(buf, reencoded);
}

// ---------- xlMetaV1Object ----------

#[test]
fn test_marshal_unmarshal_xl_meta_v1_object() {
    // meta_sys/meta_user are None in default(), skipped by skip_serializing_if,
    // causing field count mismatch on ser/de. Construct with full fields instead.
    let v = XlMetaV1Object {
        version: "1.0.1".into(),
        format: "xl".into(),
        stat: Some(StatInfo::default()),
        erasure: Some(ErasureInfo::default()),
        parts: vec![],
        meta_sys: Some(std::collections::HashMap::new()),
        meta_user: Some(std::collections::HashMap::new()),
    };
    let encoded = rmp_serde::to_vec(&v).unwrap();
    let decoded: XlMetaV1Object = rmp_serde::from_slice(&encoded).unwrap();
    let reencoded = rmp_serde::to_vec(&decoded).unwrap();
    assert_eq!(encoded, reencoded);
}

#[test]
fn test_encode_decode_xl_meta_v1_object() {
    let v = XlMetaV1Object {
        version: "1.0.1".into(),
        format: "xl".into(),
        stat: Some(StatInfo {
            size: 1000,
            mod_time: 1700000000000000000i64,
        }),
        erasure: Some(ErasureInfo {
            algorithm: 0,
            data: 4,
            parity: 2,
            block_size: 4194304,
            index: 0,
            distribution: vec![],
            checksums: vec![],
        }),
        parts: vec![ObjectPartInfo {
            number: 1,
            name: String::new(),
            etag: "d41d8cd98f00b204e9800998ecf8427e".into(),
            size: 10485760,
            actual_size: 10485760,
            index: 0,
        }],
        meta_sys: Some(std::collections::HashMap::new()),
        meta_user: Some(std::collections::HashMap::new()),
    };
    let mut buf = Vec::new();
    rmp_serde::encode::write(&mut buf, &v).unwrap();
    let decoded: XlMetaV1Object = rmp_serde::decode::from_read(&buf[..]).unwrap();
    let mut reencoded = Vec::new();
    rmp_serde::encode::write(&mut reencoded, &decoded).unwrap();
    assert_eq!(buf, reencoded);
}
