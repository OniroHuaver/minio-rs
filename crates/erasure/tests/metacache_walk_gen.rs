//! Auto-generated msgpack serialization tests for WalkDirOptions type.
//!
//! 对应 Go: `cmd/metacache-walk_gen_test.go` (msgp 生成)
//!
//! 测试 WalkDirOptions 的 msgpack 序列化/反序列化。

use erasure::*;

/// 测试 WalkDirOptions 的 Marshal/Unmarshal 往返。
///
/// Go 源: `TestMarshalUnmarshalWalkDirOptions`
#[test]
#[ignore]
fn test_marshal_unmarshal_walk_dir_options() {
    // TODO: implement when WalkDirOptions with msgp serialization is available
    /*
    let v = WalkDirOptions::default();
    let bytes = v.marshal_msg(None).expect("marshal");
    let (v2, remaining) = WalkDirOptions::unmarshal_msg(&bytes).expect("unmarshal");
    assert!(remaining.is_empty(), "bytes left after unmarshal");
    */
}

/// 测试 WalkDirOptions 的 Encode/Decode 往返。
#[test]
#[ignore]
fn test_encode_decode_walk_dir_options() {
    // TODO: implement when msgp encode/decode is available
}
