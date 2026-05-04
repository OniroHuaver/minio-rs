//! 校验和 (Checksum) 测试
//!
//! 对应 Go: internal/hash/checksum_test.go
//!
//! 测试 ChecksumType 的 header 序列化/反序列化、
//! Checksum 结构体的序列化/反序列化以及 multipart checksum。

use storage::*;

/// 测试 Checksum 添加/获取 HTTP header
///
/// 验证各种 ChecksumType (CRC32, CRC32C, CRC64NVME, SHA1, SHA256)
/// 在 composite 和 full-object 模式下的 header 序列化和反序列化。
///
/// 场景:
/// - CRC32 composite → 成功
/// - CRC32 full-object → 成功
/// - CRC32C composite → 成功
/// - CRC32C full-object → 成功
/// - CRC64NVME (always full-object) → 成功
/// - SHA1 composite → 成功
/// - SHA256 composite → 成功
/// - SHA1 full-object → ChecksumInvalid
/// - SHA256 full-object → ChecksumInvalid
///
/// 对应 Go: TestChecksumAddToHeader
#[test]
#[ignore]
fn test_checksum_add_to_header() {
    // TODO: implement when ChecksumType, Checksum, AddChecksumHeader, GetContentChecksum are available
    // let my_data = b"this-is-a-checksum-data-test";
    //
    // let test_cases: Vec<(&str, ChecksumType, bool, bool)> = vec![
    //     ("CRC32-composite", ChecksumType::CRC32, false, false),
    //     ("CRC32C-composite", ChecksumType::CRC32C, false, false),
    //     ("CRC64NVME", ChecksumType::CRC64NVME, false, false),
    //     ("SHA1-composite", ChecksumType::SHA1, false, false),
    //     ("SHA256-composite", ChecksumType::SHA256, false, false),
    //     ("SHA1-full-object", ChecksumType::SHA1, true, true),
    //     ("SHA256-full-object", ChecksumType::SHA256, true, true),
    // ];
    //
    // for (name, ctype, full_obj, want_err) in test_cases {
    //     if full_obj && (ctype.is(ChecksumType::SHA1) || ctype.is(ChecksumType::SHA256)) {
    //         let typ = ChecksumType::new(ctype.to_string(), "full-object");
    //         assert!(typ.is(ChecksumType::INVALID), "Expected ChecksumInvalid for {}", name);
    //         continue;
    //     }
    //
    //     let mut chksm = Checksum::from_data(ctype, my_data).unwrap();
    //     if full_obj { chksm.set_full_object(); }
    //     if ctype.is(ChecksumType::CRC64NVME) { chksm.set_full_object(); }
    //
    //     let mut headers = HeaderMap::new();
    //     add_checksum_header(&mut headers, &chksm.as_map()).unwrap();
    //     let got = get_content_checksum(&headers).unwrap();
    //
    //     assert!(chksm.equal(&got), "Checksum mismatch for {}", name);
    //     assert_eq!(got.typ, chksm.typ, "Type mismatch for {}", name);
    // }
}

/// 测试 Checksum 序列化/反序列化
///
/// 场景:
/// - 从数据创建 CRC32 Checksum
/// - 调用 AppendTo 序列化
/// - 调用 ChecksumFromBytes 反序列化
/// - 验证 Matches 和 Equal
///
/// 对应 Go: TestChecksumSerializeDeserialize
#[test]
#[ignore]
fn test_checksum_serialize_deserialize() {
    // TODO: implement when Checksum serialization is available
    // let my_data = b"this-is-a-checksum-data-test";
    // let chksm = Checksum::from_data(ChecksumType::CRC32, my_data).unwrap();
    //
    // let serialized = chksm.append_to(&mut vec![], None).unwrap();
    // let chksm_out = Checksum::from_bytes(&serialized).unwrap();
    //
    // assert!(chksm_out.matches(my_data, 0).is_ok(), "Checksum mismatch");
    // assert!(chksm_out.equal(&chksm), "Checksum structural mismatch");
}

/// 测试 Multipart Checksum 序列化/反序列化
///
/// 场景:
/// - 将数据分为 3 部分, 分别计算 CRC32C 校验和
/// - 组合各部分校验和
/// - 创建包含 multipart flag 的最终 checksum
/// - 序列化后反序列化
/// - 验证各部分校验和正确
///
/// 对应 Go: TestChecksumSerializeDeserializeMultiPart
#[test]
#[ignore]
fn test_checksum_serialize_deserialize_multipart() {
    // TODO: implement when Checksum multipart serialization is available
    // let dummy = b"The quick brown fox jumps over the lazy dog. \
    //     Pack my box with five dozen brown eggs. \
    //     Have another go it will all make sense in the end!";
    //
    // let part_size = dummy.len() / 3;
    // let parts = vec![&dummy[..part_size], &dummy[part_size..2*part_size], &dummy[2*part_size..]];
    //
    // let ctype = ChecksumType::CRC32C;
    // let part_checksums: Vec<Checksum> = parts.iter().map(|p| Checksum::from_data(ctype, p).unwrap()).collect();
    //
    // let mut combined = vec![];
    // for cs in &part_checksums {
    //     combined.extend_from_slice(&cs.raw);
    // }
    //
    // let final_type = ctype | ChecksumType::MULTIPART | ChecksumType::INCLUDES_MULTIPART;
    // let mut final_chksm = Checksum::from_data(final_type, &combined).unwrap();
    // final_chksm.want_parts = 3;
    //
    // let serialized = final_chksm.append_to(&mut vec![], &combined).unwrap();
    // let chksm_out = Checksum::from_bytes(&serialized).unwrap();
    //
    // assert!(chksm_out.equal(&final_chksm), "Checksum mismatch");
    //
    // let read_parts = ReadPartCheckSums::new(&serialized, ctype);
    // for (i, expected) in part_checksums.iter().enumerate() {
    //     assert_eq!(read_parts.get(i, ctype), expected.encoded, "Part {} mismatch", i+1);
    // }
}
