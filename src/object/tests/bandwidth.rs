//! Bandwidth monitoring tests
//!
//! Tests for replication bandwidth monitoring measurement, throttling, and report generation.

/// Verifies the Monitor.GetReport function.
///
/// Tests bandwidth monitoring report generation:
/// 1. Create two measurement scenarios (ZeroToOne, OneToTwo)
/// 2. Create bucketThrottle and Monitor
/// 3. Verify exponential moving average and bandwidth report correctness
#[test]
#[ignore]
// TODO: implement when bandwidth monitor types are available
fn test_monitor_get_report() {
    // let start = Instant::now();
    //
    // // Scenario 1: ZeroToOne — from 0 to 1MiB/s
    // let m0 = BucketMeasurement::new(start);
    // m0.increment_bytes(0);
    //
    // let throttle = BucketThrottle { node_bandwidth_per_sec: 1024 * 1024 };
    // let mut th = HashMap::new();
    // th.insert(BucketOptions { name: "bucket".into(), replication_arn: "arn".into() }, throttle);
    //
    // let mut monitor = Monitor {
    //     buckets_measurement: map!{
    //         BucketOptions { name: "bucket".into(), replication_arn: "arn".into() } => m0,
    //     },
    //     buckets_throttle: th,
    //     node_count: 1,
    // };
    //
    // // First report
    // let report = monitor.get_report(SelectBuckets::All);
    // // Verify report contains correct limit and bandwidth
    //
    // // Second update
    // monitor.buckets_measurement.get_mut(&opts).unwrap().increment_bytes(1024 * 1024);
    // let report2 = monitor.get_report(SelectBuckets::All);
    // // Verify exponential moving average calculation
}

/// Verifies bandwidth monitoring serialization/deserialization.
#[test]
#[ignore]
// TODO: implement when bandwidth monitor types are available
fn test_monitor_serde() {
    // let report = BucketBandwidthReport {
    //     bucket_stats: map!{
    //         BucketOptions { name: "bucket".into(), replication_arn: "arn".into() } => Details {
    //             limit_in_bytes_per_second: 1024 * 1024,
    //             current_bandwidth_in_bytes_per_second: 500_000.0,
    //         },
    //     },
    // };
    // let json = serde_json::to_string(&report).unwrap();
    // let deserialized: BucketBandwidthReport = serde_json::from_str(&json).unwrap();
    // assert_eq!(report, deserialized);
}
