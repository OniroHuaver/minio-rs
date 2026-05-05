//! Storage REST common types MessagePack serialization tests
//!
//! Tests MsgPack roundtrip for nsScannerOptions type.

use storage::*;

/// Tests nsScannerOptions MsgPack roundtrip
#[test]
#[ignore]
fn test_marshal_unmarshal_ns_scanner_options() {
    // TODO: implement when nsScannerOptions has ser/de
    // let v = nsScannerOptions::default();
    // let encoded = rmp_serde::to_vec(&v).unwrap();
    // let decoded: nsScannerOptions = rmp_serde::from_slice(&encoded).unwrap();
    // assert_eq!(v, decoded);
}

/// Tests nsScannerOptions Encode/Decode roundtrip
#[test]
#[ignore]
fn test_encode_decode_ns_scanner_options() {
    // TODO: implement when nsScannerOptions has ser/de
    // let v = nsScannerOptions::default();
    // let mut buf = Vec::new();
    // rmp_serde::encode::write(&mut buf, &v).unwrap();
    // let decoded: nsScannerOptions = rmp_serde::decode::from_read(&buf[..]).unwrap();
    // assert_eq!(v, decoded);
}
