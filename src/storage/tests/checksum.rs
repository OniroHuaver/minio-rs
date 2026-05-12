//! Checksum tests
//!
//! Tests ChecksumType header serialization/deserialization,
//! Checksum struct serialization/deserialization, and multipart checksum.


/// Tests Checksum add/get HTTP header
///
/// Verifies header serialization and deserialization for various
/// ChecksumType values (CRC32, CRC32C, CRC64NVME, SHA1, SHA256)
/// in composite and full-object modes.
///
/// Scenarios:
/// - CRC32 composite -> success
/// - CRC32 full-object -> success
/// - CRC32C composite -> success
/// - CRC32C full-object -> success
/// - CRC64NVME (always full-object) -> success
/// - SHA1 composite -> success
/// - SHA256 composite -> success
/// - SHA1 full-object -> ChecksumInvalid
/// - SHA256 full-object -> ChecksumInvalid
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

/// Tests Checksum serialize/deserialize
///
/// Scenarios:
/// - Create CRC32 Checksum from data
/// - Serialize via AppendTo
/// - Deserialize via ChecksumFromBytes
/// - Verify Matches and Equal
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

/// Tests Multipart Checksum serialize/deserialize
///
/// Scenarios:
/// - Split data into 3 parts, compute CRC32C checksum for each
/// - Combine part checksums
/// - Create final checksum with multipart flag
/// - Serialize then deserialize
/// - Verify each part checksum is correct
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
