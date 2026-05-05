//! Copy Part Range tests

/// Verifies parseCopyPartRangeSpec function.
///
/// Tests copy part range parsing:
/// - Success cases: "bytes=2-5", "bytes=0000-0006" etc.
/// - Invalid formats: "bytes=8", "bytes=5-2" (reversed range)
/// - Invalid formats: missing "bytes=" prefix, contains spaces etc.
/// - Out of object size: "bytes=10-10" (object_size=10)
#[test]
#[ignore]
// TODO: implement when copy part range parsing is available
fn test_parse_copy_part_range_spec() {
    // let object_size = 10i64;
    //
    // // Success cases
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
    // // Invalid formats (should fail to parse)
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
    // // Out of object size (parsed successfully but validation fails)
    // let out_of_range_cases = vec!["bytes=10-10", "bytes=20-30"];
    // for range_str in out_of_range_cases {
    //     let rs = parse_copy_part_range_spec(range_str).unwrap();
    //     let result = check_copy_part_range_with_size(&rs, object_size);
    //     assert_eq!(result, Err(err_invalid_range_source));
    // }
}
