//! Disk I/O stats tests
//!
//! Tests parsing of /sys/block/<dev>/stat format disk I/O stats (read_drive_stats).

use storage::*;

/// Tests read_drive_stats parsing disk I/O stats string
///
/// Scenarios:
/// - Full 18-field stats -> correctly parse all fields
/// - 15-field stats (no discard) -> correctly parse (Discard fields = 0)
/// - 11-field stats (no discard/flush) -> correctly parse remaining fields
/// - Insufficient fields -> parse failure
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
