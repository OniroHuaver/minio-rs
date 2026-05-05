//! xlMetaV2 MessagePack serialization/deserialization tests
//!
//! Tests MsgPack (via rmp-serde) roundtrip for xlMetaV2 related types.

use crate::storage::{
    XlMetaDataDirDecoder, XlMetaV2DeleteMarker, XlMetaV2Object, XlMetaV2Version,
    XlMetaV2VersionHeader,
};

// ========== XlMetaDataDirDecoder ==========

#[test]
fn test_marshal_unmarshal_xl_meta_data_dir_decoder() {
    let v = XlMetaDataDirDecoder::default();
    let encoded = rmp_serde::to_vec(&v).unwrap();
    let decoded: XlMetaDataDirDecoder = rmp_serde::from_slice(&encoded).unwrap();
    let reencoded = rmp_serde::to_vec(&decoded).unwrap();
    assert_eq!(encoded, reencoded);
}

#[test]
fn test_encode_decode_xl_meta_data_dir_decoder() {
    let v = XlMetaDataDirDecoder {
        data: b"test-data-dir".to_vec(),
    };
    let mut buf = Vec::new();
    rmp_serde::encode::write(&mut buf, &v).unwrap();
    let decoded: XlMetaDataDirDecoder = rmp_serde::decode::from_read(&buf[..]).unwrap();
    let mut reencoded = Vec::new();
    rmp_serde::encode::write(&mut reencoded, &decoded).unwrap();
    assert_eq!(buf, reencoded);
}

// ========== XlMetaV2DeleteMarker ==========

#[test]
fn test_marshal_unmarshal_xl_meta_v2_delete_marker() {
    let v = XlMetaV2DeleteMarker::default();
    let encoded = rmp_serde::to_vec(&v).unwrap();
    let decoded: XlMetaV2DeleteMarker = rmp_serde::from_slice(&encoded).unwrap();
    let reencoded = rmp_serde::to_vec(&decoded).unwrap();
    assert_eq!(encoded, reencoded);
}

#[test]
fn test_encode_decode_xl_meta_v2_delete_marker() {
    let v = XlMetaV2DeleteMarker {
        version_id: b"abc-def-123".to_vec(),
        mod_time: 1700000000000000000i64,
        signature: vec![0u8; 32],
        r#type: 2,
        flags: 0,
    };
    let mut buf = Vec::new();
    rmp_serde::encode::write(&mut buf, &v).unwrap();
    let decoded: XlMetaV2DeleteMarker = rmp_serde::decode::from_read(&buf[..]).unwrap();
    let mut reencoded = Vec::new();
    rmp_serde::encode::write(&mut reencoded, &decoded).unwrap();
    assert_eq!(buf, reencoded);
}

// ========== XlMetaV2Object ==========

#[test]
fn test_marshal_unmarshal_xl_meta_v2_object() {
    let v = XlMetaV2Object::default();
    let encoded = rmp_serde::to_vec(&v).unwrap();
    let decoded: XlMetaV2Object = rmp_serde::from_slice(&encoded).unwrap();
    let reencoded = rmp_serde::to_vec(&decoded).unwrap();
    assert_eq!(encoded, reencoded);
}

#[test]
fn test_encode_decode_xl_meta_v2_object() {
    let v = XlMetaV2Object {
        version_id: b"v1-v2-v3".to_vec(),
        data_dir: b"dd-001".to_vec(),
        mod_time: 1700000000000000000i64,
        signature: vec![0u8; 32],
        r#type: 1,
        flags: 0,
    };
    let mut buf = Vec::new();
    rmp_serde::encode::write(&mut buf, &v).unwrap();
    let decoded: XlMetaV2Object = rmp_serde::decode::from_read(&buf[..]).unwrap();
    let mut reencoded = Vec::new();
    rmp_serde::encode::write(&mut reencoded, &decoded).unwrap();
    assert_eq!(buf, reencoded);
}

// ========== XlMetaV2Version ==========

#[test]
fn test_marshal_unmarshal_xl_meta_v2_version() {
    let v = XlMetaV2Version::default();
    let encoded = rmp_serde::to_vec(&v).unwrap();
    let decoded: XlMetaV2Version = rmp_serde::from_slice(&encoded).unwrap();
    let reencoded = rmp_serde::to_vec(&decoded).unwrap();
    assert_eq!(encoded, reencoded);
}

#[test]
fn test_encode_decode_xl_meta_v2_version() {
    let v = XlMetaV2Version {
        header: XlMetaV2VersionHeader {
            version_id: b"ver-001".to_vec(),
            mod_time: 1700000000000000000i64,
            signature: vec![0u8; 32],
            r#type: 1,
            flags: 0,
        },
        object: Some(XlMetaV2Object {
            version_id: b"ver-001".to_vec(),
            data_dir: b"dd-001".to_vec(),
            mod_time: 1700000000000000000i64,
            signature: vec![0u8; 32],
            r#type: 1,
            flags: 0,
        }),
        delete_marker: None,
    };
    let mut buf = Vec::new();
    rmp_serde::encode::write(&mut buf, &v).unwrap();
    let decoded: XlMetaV2Version = rmp_serde::decode::from_read(&buf[..]).unwrap();
    let mut reencoded = Vec::new();
    rmp_serde::encode::write(&mut reencoded, &decoded).unwrap();
    assert_eq!(buf, reencoded);
}

// ========== XlMetaV2VersionHeader ==========

#[test]
fn test_marshal_unmarshal_xl_meta_v2_version_header() {
    let v = XlMetaV2VersionHeader::default();
    let encoded = rmp_serde::to_vec(&v).unwrap();
    let decoded: XlMetaV2VersionHeader = rmp_serde::from_slice(&encoded).unwrap();
    let reencoded = rmp_serde::to_vec(&decoded).unwrap();
    assert_eq!(encoded, reencoded);
}

#[test]
fn test_encode_decode_xl_meta_v2_version_header() {
    let v = XlMetaV2VersionHeader {
        version_id: b"hdr-001".to_vec(),
        mod_time: 1700000000000000000i64,
        signature: vec![0u8; 32],
        r#type: 1,
        flags: 0,
    };
    let mut buf = Vec::new();
    rmp_serde::encode::write(&mut buf, &v).unwrap();
    let decoded: XlMetaV2VersionHeader = rmp_serde::decode::from_read(&buf[..]).unwrap();
    let mut reencoded = Vec::new();
    rmp_serde::encode::write(&mut reencoded, &decoded).unwrap();
    assert_eq!(buf, reencoded);
}
