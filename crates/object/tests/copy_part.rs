//! Copy Part Range 测试
//!
//! 对应 Go: `cmd/copy-part-range_test.go`

/// 验证 parseCopyPartRangeSpec 函数。
///
/// Go: `TestParseCopyPartRangeSpec`
/// 测试复制 part 范围解析:
/// - 成功用例: "bytes=2-5"、"bytes=0000-0006" 等
/// - 无效格式: "bytes=8"、"bytes=5-2" 等(范围反序)
/// - 无效格式: 缺失 "bytes=" 前缀、含空格等
/// - 超出对象大小: "bytes=10-10"(对象大小=10)
#[test]
#[ignore]
// TODO: implement when copy part range parsing is available
fn test_parse_copy_part_range_spec() {
    // let object_size = 10i64;
    //
    // // 成功用例
    // let success_cases = vec![
    //     ("bytes=2-5", 2, 5),
    //     ("bytes=2-9", 2, 9),
    //     ("bytes=2-2", 2, 2),
    //     ("bytes=0000-0006", 0, 6),
    // ];
    // for (range_str, expected_start, expected_end) in success_cases {
    //     let rs = parse_copy_part_range_spec(range_str).unwrap();
    //     let (start, length) = rs.get_offset_length(object_size).unwrap();
    //     assert_eq!(start, expected_start);
    //     assert_eq!(start + length - 1, expected_end);
    // }
    //
    // // 无效格式(应解析失败)
    // let invalid_cases = vec![
    //     "bytes=8", "bytes=5-2", "bytes=+2-5", "bytes=2-+5",
    //     "bytes=2--5", "bytes=-", "2-5", "bytes = 2-5",
    //     "bytes=2 - 5", "bytes=0-0,-1", "bytes=2-5 ", "bytes=-1", "bytes=1-",
    // ];
    // for range_str in invalid_cases {
    //     let result = parse_copy_part_range_spec(range_str);
    //     assert!(result.is_err(), "expected error for range: {range_str}");
    // }
    //
    // // 超出对象大小(解析成功但验证失败)
    // let out_of_range_cases = vec!["bytes=10-10", "bytes=20-30"];
    // for range_str in out_of_range_cases {
    //     let rs = parse_copy_part_range_spec(range_str).unwrap();
    //     let result = check_copy_part_range_with_size(&rs, object_size);
    //     assert_eq!(result, Err(err_invalid_range_source));
    // }
}
