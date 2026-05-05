//! Storage datatypes benchmark tests
//!
//! Benchmarks MessagePack vs GOB encoding/decoding for VolInfo,
//! DiskInfo, FileInfo, and other types.


/// Benchmark: VolInfo MsgPack decode
#[test]
#[ignore]
fn benchmark_decode_vol_info_msgp() {
    // TODO: implement benchmark when VolInfo is available
    // let v = VolInfo { name: "uuid".into(), created: Utc::now() };
    // let mut buf = Vec::new();
    // rmp_serde::encode::write(&mut buf, &v).unwrap();
    // // Benchmark decode speed
}

/// Benchmark: DiskInfo MsgPack decode
#[test]
#[ignore]
fn benchmark_decode_disk_info_msgp() {
    // TODO: implement benchmark when DiskInfo is available
    // let v = DiskInfo {
    //     total: 1000, free: 1000, used: 1000,
    //     fs_type: "xfs".into(), root_disk: true,
    //     healing: true, endpoint: "http://localhost:9001/tmp/drive1".into(),
    //     mount_path: "/tmp/drive1".into(), id: "uuid".into(),
    //     ..Default::default()
    // };
    // let mut buf = Vec::new();
    // rmp_serde::encode::write(&mut buf, &v).unwrap();
    // // Benchmark decode speed
}

/// Benchmark: DiskInfo GOB decode
#[test]
#[ignore]
fn benchmark_decode_disk_info_gob() {
    // TODO: implement benchmark using bincode or similar as GOB alternative
}

/// Benchmark: DiskInfo MsgPack encode
#[test]
#[ignore]
fn benchmark_encode_disk_info_msgp() {
    // TODO: implement benchmark when DiskInfo is available
}

/// Benchmark: DiskInfo GOB encode
#[test]
#[ignore]
fn benchmark_encode_disk_info_gob() {
    // TODO: implement benchmark using bincode or similar as GOB alternative
}

/// Benchmark: FileInfo MsgPack decode
#[test]
#[ignore]
fn benchmark_decode_file_info_msgp() {
    // TODO: implement benchmark when FileInfo is available
}

/// Benchmark: FileInfo GOB decode
#[test]
#[ignore]
fn benchmark_decode_file_info_gob() {
    // TODO: implement benchmark using bincode or similar as GOB alternative
}

/// Benchmark: FileInfo MsgPack encode
#[test]
#[ignore]
fn benchmark_encode_file_info_msgp() {
    // TODO: implement benchmark when FileInfo is available
}

/// Benchmark: FileInfo GOB encode
#[test]
#[ignore]
fn benchmark_encode_file_info_gob() {
    // TODO: implement benchmark using bincode or similar as GOB alternative
}
