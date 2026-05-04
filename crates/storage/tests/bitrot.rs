//! 位衰减 (Bitrot) 读写器测试
//!
//! 对应 Go: cmd/bitrot_test.go
//!
//! 测试新 BitrotWriter/BitrotReader 组合，验证使用不同位衰减算法时
//! 写入和读取数据的一致性。

use storage::*;

/// 测试所有 BitrotAlgorithm 的读写器组合
///
/// 场景:
/// - 使用 new_bitrot_writer 写入 35 字节数据 (分多次写入)
/// - 写入完成后使用 new_bitrot_reader 读取
/// - 在偏移 0, 10, 20, 30 处分别读取 10/5 字节
/// - 验证数据一致性
///
/// 对应 Go: TestAllBitrotAlgorithms
#[test]
#[ignore]
fn test_all_bitrot_algorithms() {
    // TODO: implement when BitrotWriter/BitrotReader are available
    // for algo in ALL_BITROT_ALGORITHMS {
    //     let tmp = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
    //     let disk = new_local_xl_storage(tmp.to_str().unwrap()).unwrap();
    //     disk.make_vol("testvol").await?;
    //
    //     let writer = new_bitrot_writer(&disk, "", "testvol", "testfile", 35, algo, 10);
    //     writer.write(b"aaaaaaaaaa").await?;
    //     writer.write(b"aaaaaaaaaa").await?;
    //     writer.write(b"aaaaaaaaaa").await?;
    //     writer.write(b"aaaaa").await?;
    //     writer.close().await?;
    //
    //     let sum = bitrot_writer_sum(&writer);
    //     let reader = new_bitrot_reader(&disk, None, "testvol", "testfile", 35, algo, sum, 10);
    //
    //     let mut buf = vec![0u8; 10];
    //     reader.read_at(&mut buf, 0).await?;
    //     reader.read_at(&mut buf, 10).await?;
    //     reader.read_at(&mut buf, 20).await?;
    //     reader.read_at(&mut buf[..5], 30).await?;
    //     reader.close().await?;
    // }
}
