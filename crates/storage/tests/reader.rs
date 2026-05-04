//! Hash Reader 测试
//!
//! 对应 Go: internal/hash/reader_test.go
//!
//! 测试带 MD5/SHA256 校验的 hash reader (Reader):
//! - 辅助方法 (Size, MD5Current, SHA256HexString 等)
//! - 校验和验证 (正确/错误 MD5, SHA256)
//! - 嵌套 reader
//! - 截断读取
//! - 非法参数

use storage::*;

/// 测试 HashReader 的辅助方法
///
/// 验证:
/// - Size() 返回 4
/// - ActualSize() 返回 4
/// - MD5Current() 返回正确 MD5 hex
/// - SHA256HexString() 返回正确 SHA256 hex
/// - MD5 base64 编码正确
///
/// 对应 Go: TestHashReaderHelperMethods
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

/// 测试 HashReader 校验和验证
///
/// 场景:
/// - 无校验和 → 成功
/// - 错误 MD5 → BadDigest
/// - 错误 SHA256 → SHA256Mismatch
/// - 嵌套 reader (内部有校验) → 合并校验
/// - 嵌套 + 错误 SHA256 → SHA256Mismatch
/// - 嵌套 + 正确 SHA256 → 成功
/// - 嵌套 + 截断 + 正确 SHA256 → ErrOverread
/// - 嵌套 + 错误 MD5 → BadDigest
/// - 正确 SHA256 + 截断 → ErrOverread
/// - 正确 MD5 + 嵌套 → 成功
/// - 正确 MD5 + 截断 → ErrOverread
///
/// 对应 Go: TestHashReaderVerification
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

/// 测试 HashReader 非法参数
///
/// 场景:
/// - 非法 MD5 hex → 构造失败
/// - 非法 SHA256 hex → 构造失败
/// - 嵌套 reader 合并 → 成功
/// - 内层与外层 SHA256 不匹配 → 构造失败
/// - 正确 SHA256 → 成功
/// - 内层与外层 MD5 不匹配 → 构造失败
/// - 正确 MD5 → 成功
/// - 无校验 → 成功
/// - 嵌套 + size 不匹配 → 构造失败
///
/// 对应 Go: TestHashReaderInvalidArguments
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
