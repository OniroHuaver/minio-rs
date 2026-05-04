//! 磁盘 I/O 状态统计测试
//!
//! 对应 Go: internal/disk/stat_test.go
//!
//! 测试从 /sys/block/<dev>/stat 格式的磁盘 I/O 状态解析 (read_drive_stats)。

use storage::*;

/// 测试 read_drive_stats 解析磁盘 I/O 状态字符串
///
/// 场景:
/// - 完整 18 字段统计 → 正确解析所有字段
/// - 15 字段统计 (无 discard 列) → 正确解析 (Discard 字段为 0)
/// - 11 字段统计 (无 discard/flush 列) → 正确解析剩余字段
/// - 字段不足 → 解析失败
///
/// 对应 Go: TestReadDriveStats
#[test]
#[ignore]
fn test_read_drive_stats() {
    // TODO: implement when read_drive_stats() is available
    // let test_cases = vec![
    //     // (stat_string, expected_iostats, expect_err)
    //     (
    //         "1432553   420084 66247626  2398227  7077314  8720147 157049224  7469810        0  7580552  9869354    46037        0 41695120     1315        0        0",
    //         IOStats {
    //             read_ios: 1432553, read_merges: 420084, read_sectors: 66247626, read_ticks: 2398227,
    //             write_ios: 7077314, write_merges: 8720147, write_sectors: 157049224, write_ticks: 7469810,
    //             current_ios: 0, total_ticks: 7580552, req_ticks: 9869354,
    //             discard_ios: 46037, discard_merges: 0, discard_sectors: 41695120, discard_ticks: 1315,
    //             flush_ios: 0, flush_ticks: 0,
    //         },
    //         false,
    //     ),
    //     (
    //         "1432553   420084 66247626  2398227  7077314  8720147 157049224  7469810        0  7580552  9869354",
    //         IOStats {
    //             read_ios: 1432553, read_merges: 420084, read_sectors: 66247626, read_ticks: 2398227,
    //             write_ios: 7077314, write_merges: 8720147, write_sectors: 157049224, write_ticks: 7469810,
    //             current_ios: 0, total_ticks: 7580552, req_ticks: 9869354,
    //             discard_ios: 0, discard_merges: 0, discard_sectors: 0, discard_ticks: 0,
    //             flush_ios: 0, flush_ticks: 0,
    //         },
    //         false,
    //     ),
    //     (
    //         "1432553   420084 66247626  2398227",
    //         IOStats::default(),
    //         true,
    //     ),
    // ];
    //
    // for (input, expected, expect_err) in test_cases {
    //     let tmpfile = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
    //     std::fs::write(&tmpfile, input).unwrap();
    //
    //     let result = read_drive_stats(tmpfile.to_str().unwrap());
    //     if expect_err {
    //         assert!(result.is_err(), "expected error for input: {}", input);
    //     } else {
    //         let iostats = result.unwrap();
    //         assert_eq!(iostats, expected, "IOStats mismatch for input: {}", input);
    //     }
    // }
}
