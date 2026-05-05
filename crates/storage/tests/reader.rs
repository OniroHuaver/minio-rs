//! Hash Reader tests
//!
//! Tests hash reader with MD5/SHA256 verification:
//! - Helper methods (Size, MD5Current, SHA256HexString, etc.)
//! - Checksum verification (correct/incorrect MD5, SHA256)
//! - Nested reader
//! - Truncated read
//! - Invalid arguments

use storage::*;

/// Tests HashReader helper methods
///
/// Verify:
/// - Size() returns 4
/// - ActualSize() returns 4
/// - MD5Current() returns correct MD5 hex
/// - SHA256HexString() returns correct SHA256 hex
/// - MD5 base64 encoding is correct
#[test]
#[ignore]
fn test_hash_reader_helper_methods() {
    // TODO: implement when hash::Reader is available
    // let data = b"abcd";
    // let md5_hex = "e2fc714c4727ee9395f324cd2e7f331f";
    // let sha256_hex = "88d4266fd4e6338d13b845fcf289579d209c897823b9217da3e161936f031589";
    //
    // let r = hash::Reader::new(data.as_slice(), 4, md5_hex, sha256_hex, 4).unwrap();
    // std::io::copy(&mut r, &mut std::io::sink()).unwrap();
    //
    // assert_eq!(hex::encode(r.md5_current()), md5_hex);
    // assert_eq!(r.sha256_hex_string(), sha256_hex);
    // assert_eq!(base64::Engine::encode(&base64::engine::general_purpose::STANDARD, r.md5_current()), "4vxxTEcn7pOV8yTNLn8zHw==");
    // assert_eq!(r.size(), 4);
    // assert_eq!(r.actual_size(), 4);
}

/// Tests HashReader checksum verification
///
/// Scenarios:
/// - No checksum -> success
/// - Wrong MD5 -> BadDigest
/// - Wrong SHA256 -> SHA256Mismatch
/// - Nested reader (inner checksum) -> merged verification
/// - Nested + wrong SHA256 -> SHA256Mismatch
/// - Nested + correct SHA256 -> success
/// - Nested + truncated + correct SHA256 -> ErrOverread
/// - Nested + wrong MD5 -> BadDigest
/// - Correct SHA256 + truncated -> ErrOverread
/// - Correct MD5 + nested -> success
/// - Correct MD5 + truncated -> ErrOverread
#[test]
#[ignore]
fn test_hash_reader_verification() {
    // TODO: implement when hash::Reader is available
    // let test_cases: Vec<(&str, &[u8], i64, i64, &str, &str, Option<Error>)> = vec![
    //     ("Success no checksum", b"abcd", 4, 4, "", "", None),
    //     ("Failure md5 mismatch", b"abcd", 4, 4, "d41d8cd98f00b204e9800998ecf8427f", "", Some(Error::BadDigest { .. })),
    //     ("Failure sha256 mismatch", b"abcd", 4, 4, "", "88d4266fd4e6338d13b845fcf289579d209c897823b9217da3e161936f031580", Some(Error::SHA256Mismatch { .. })),
    //     ("Nested reader merge", must_reader(b"abcd", 4, "", "", 4), 4, 4, "", "", None),
    //     // ... more cases
    // ];
    //
    // for (desc, src, size, actual_size, md5_hex, sha256_hex, expected_err) in test_cases {
    //     let r = hash::Reader::new(src, size, md5_hex, sha256_hex, actual_size);
    //     let result = std::io::copy(&mut r, &mut std::io::sink());
    //     match expected_err {
    //         None => assert!(result.is_ok(), "Test '{}': expected Ok, got {:?}", desc, result),
    //         Some(_) => assert!(result.is_err(), "Test '{}': expected Err, got Ok", desc),
    //     }
    // }
}

/// Tests HashReader invalid arguments
///
/// Scenarios:
/// - Invalid MD5 hex -> construction fails
/// - Invalid SHA256 hex -> construction fails
/// - Nested reader merge -> success
/// - Inner/outer SHA256 mismatch -> construction fails
/// - Correct SHA256 -> success
/// - Inner/outer MD5 mismatch -> construction fails
/// - Correct MD5 -> success
/// - No checksum -> success
/// - Nested + size mismatch -> construction fails
#[test]
#[ignore]
fn test_hash_reader_invalid_arguments() {
    // TODO: implement when hash::Reader is available
    // let test_cases: Vec<(&str, &[u8], i64, i64, &str, &str, bool)> = vec![
    //     ("Invalid md5", b"abcd", 4, 4, "invalid-md5", "", false),
    //     ("Invalid sha256", b"abcd", 4, 4, "", "invalid-sha256", false),
    //     ("Nested merge", must_reader(b"abcd", 4, "", "", 4), 4, 4, "", "", true),
    //     ("Mismatching sha256 nested", must_reader(b"abcd", 4, "", SHA256_CORRECT, 4), 4, 4, "", SHA256_WRONG, false),
    //     ("Correct sha256 nested", must_reader(b"abcd", 4, "", SHA256_CORRECT, 4), 4, 4, "", SHA256_CORRECT, true),
    //     ("Mismatching MD5 nested", must_reader(b"abcd", 4, MD5_CORRECT, "", 4), 4, 4, MD5_WRONG, "", false),
    //     ("Correct MD5 nested", must_reader(b"abcd", 4, MD5_CORRECT, "", 4), 4, 4, MD5_CORRECT, "", true),
    //     ("All ok", b"abcd", 4, 4, "", "", true),
    //     ("Nested size mismatch", must_reader(b"abcd-morestuff", 4, "", "", -1), 2, -1, "", "", false),
    // ];
    //
    // for (desc, src, size, actual_size, md5_hex, sha256_hex, success) in test_cases {
    //     let result = hash::Reader::new(src, size, md5_hex, sha256_hex, actual_size);
    //     if success {
    //         assert!(result.is_ok(), "Test '{}': expected success, got {:?}", desc, result);
    //     } else {
    //         assert!(result.is_err(), "Test '{}': expected error, got Ok", desc);
    //     }
    // }
}
