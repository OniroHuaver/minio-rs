//! Storage REST common types MessagePack 序列化测试
//!
//! 对应 Go: cmd/storage-rest-common_gen_test.go
//!
//! 测试 nsScannerOptions 类型的 MsgPack roundtrip。

use storage::*;

/// 测试 nsScannerOptions MsgPack roundtrip
#[test]
#[ignore]
fn test_marshal_unmarshal_ns_scanner_options() {
    // TODO: implement when nsScannerOptions has ser/de
    // let v = nsScannerOptions::default();
    // let encoded = rmp_serde::to_vec(&v).unwrap();
    // let decoded: nsScannerOptions = rmp_serde::from_slice(&encoded).unwrap();
    // assert_eq!(v, decoded);
}

/// 测试 nsScannerOptions Encode/Decode roundtrip
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
