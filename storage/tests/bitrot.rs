//! Bitrot reader/writer tests
//!
//! Tests the BitrotWriter/BitrotReader combination, verifying data
//! consistency when writing and reading with different bitrot algorithms.


/// Tests all BitrotAlgorithm reader/writer combinations
///
/// Scenarios:
/// - Write 35 bytes via new_bitrot_writer (multiple writes)
/// - Read back via new_bitrot_reader after writing
/// - Read 10/5 bytes at offsets 0, 10, 20, 30
/// - Verify data consistency
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
