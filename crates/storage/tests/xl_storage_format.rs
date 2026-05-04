//! xl.meta 格式校验与解析测试
//!
//! 对应 Go: cmd/xl-storage-format_test.go
//!
//! 测试 xl-meta format 的版本/格式校验、JSON 序列化/反序列化、
//! part size 计算以及 xlMetaV2 的基准测试。

use base::error::MinioError;
use storage::{
    calculate_part_size_from_idx, is_xl_meta_erasure_info_valid, is_xl_meta_format_valid,
};

/// 测试 is_xl_meta_format_valid 校验 xl.meta 的 version+format 字段
///
/// 场景:
/// - ("123", "fs") → false (format 不是 "xl")
/// - ("123", "xl") → false (version 不是 "1.0.0" 或 "1.0.1")
/// - ("1.0.0", "test") → false (format 不是 "xl")
/// - ("1.0.0", "xl") → true
/// - ("1.0.1", "xl") → true
///
/// 对应 Go: TestIsXLMetaFormatValid
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

/// 测试 is_xl_meta_erasure_info_valid 校验擦除码参数
///
/// 场景:
/// - data=5, parity=6 → false (不相等且不为零)
/// - data=5, parity=5 → true
/// - data=0, parity=5 → false
/// - data=-1, parity=5 → false
/// - data=5, parity=0 → true
/// - data=5, parity=4 → true
///
/// 对应 Go: TestIsXLMetaErasureInfoValid
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

/// 测试 calculate_part_size_from_idx 根据 part 索引计算 part 大小
///
/// 场景:
/// - 正常 case: total_size=4MiB, part_size=2MiB, part_index=1 → 2MiB
/// - 最后 part: total_size=5MiB, part_size=2MiB, part_index=3 → 1MiB
/// - 越界索引: part_index 超出范围 → 0
/// - 错误 case: part_size=0 → errPartSizeZero
/// - 错误 case: part_index=0 → errPartSizeIndex
/// - 错误 case: total_size=-1 → errInvalidArgument
///
/// 对应 Go: TestGetPartSizeFromIdx
#[test]
fn test_get_part_size_from_idx() {
    let kib = 1024;
    let mib = 1024 * kib;

    // 正常 case
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

    // 错误 case
    let result = calculate_part_size_from_idx(10, 0, 1);
    assert!(result.is_err(), "part_size=0 should error");

    let result = calculate_part_size_from_idx(10, 1, 0);
    assert!(result.is_err(), "part_index=0 should error");

    let result = calculate_part_size_from_idx(-2, 10, 1);
    assert!(result.is_err(), "total_size<0 should error");
}

/// 测试 JSON 反序列化 (标准库 vs jsoniter) 一致性
///
/// 创建 1-part xlMetaV1Object JSON，分别用 serde 反序列化，
/// 验证结果完全一致。
///
/// 对应 Go: TestGetXLMetaV1Jsoniter1
#[test]
#[ignore]
fn test_get_xl_meta_v1_json_iter_1() {
    // TODO: implement when xlMetaV1Object and JSON parsing are available
}

/// 测试 JSON 反序列化一致性 (10-part)
///
/// 对应 Go: TestGetXLMetaV1Jsoniter10
#[test]
#[ignore]
fn test_get_xl_meta_v1_json_iter_10() {
    // TODO: implement when xlMetaV1Object and JSON parsing are available
}

/// 基准测试: xlMetaV2 浅层操作性能
///
/// 对应 Go: BenchmarkXlMetaV2Shallow
#[test]
#[ignore]
fn benchmark_xl_meta_v2_shallow() {
    // TODO: implement benchmarks when xlMetaV2 is available
}
