//! xlMetaV2 数据格式测试
//!
//! 对应 Go: cmd/xl-storage-format-v2_test.go
//!
//! 测试 xlMetaV2 的核心功能:
//! - xl.meta 数据读写、load/roundtrip
//! - Data 内联存储、查找、删除、替换、重命名
//! - xlMetaV2TrimData 裁剪
//! - UsesDataDir 判定
//! - 共享 DataDir 的版本删除
//! - mergeXLV2Versions 多盘版本合并
//! - 时间戳加载与签名转换
//! - LoadOrConvert 兼容性

use storage::*;

/// 测试 read_xl_meta_no_data 读取损坏的 xl.meta
///
/// 场景: 读取包含数据的 xl.meta 时如果缺少数据段应报错。
///
/// 对应 Go: TestReadXLMetaNoData
#[test]
#[ignore]
fn test_read_xl_meta_no_data() {
    // TODO: implement when read_xl_meta_no_data() is available
    // let corrupt_data = load_test_data("testdata/xl.meta-corrupt.gz");
    // let result = read_xl_meta_no_data(&corrupt_data[..], corrupt_data.len() as i64);
    // assert!(result.is_err(), "expected error but returned success");
}

/// 测试 xlMetaV2 数据格式: 添加版本、序列化、反序列化、
/// 数据查找/删除/替换/重命名/裁剪
///
/// 场景:
/// - 添加两个带内联数据的版本
/// - 序列化后反序列化验证数据完整性
/// - data.list() 返回 2 条
/// - data.find() 按 versionID 查询
/// - data.remove() 删除后 entries=1
/// - data.replace() 替换数据后 entries=2
/// - data.rename() 重命名 key
/// - xlMetaV2TrimData 裁剪数据后 data 长度为 0
/// - 损坏元数据可被检测
///
/// 对应 Go: TestXLV2FormatData
#[test]
#[ignore]
fn test_xl_v2_format_data() {
    // TODO: implement when xlMetaV2 is available
    // let mut xl = xlMetaV2::new();
    // let data = b"some object data";
    // let data2 = b"some other object data";
    //
    // let fi = FileInfo {
    //     volume: "volume".into(),
    //     name: "object-name".into(),
    //     version_id: "756100c6-b393-4981-928a-d49bbc164741".into(),
    //     data_dir: "bffea160-ca7f-465f-98bc-9b4f1c3ba1ef".into(),
    //     is_latest: true,
    //     ..Default::default()
    // };
    //
    // xl.add_version(&fi).unwrap();
    //
    // let mut fi2 = fi.clone();
    // fi2.version_id = uuid::Uuid::new_v4().to_string();
    // fi2.data_dir = uuid::Uuid::new_v4().to_string();
    // fi2.data = data2.to_vec();
    // xl.add_version(&fi2).unwrap();
    //
    // // Serialize and roundtrip
    // let serialized = xl.append_to(&mut vec![]).unwrap();
    // let mut xl2 = xlMetaV2::new();
    // xl2.load(&serialized).unwrap();
    //
    // // Should have 2 data entries
    // let list = xl2.data.list().unwrap();
    // assert_eq!(list.len(), 2);
    //
    // // Find data by version ID
    // assert_eq!(xl2.data.find(fi.version_id.as_bytes()), Some(data.as_ref()));
    //
    // // Remove entry
    // xl2.data.remove(fi2.version_id.as_bytes());
    // assert_eq!(xl2.data.find(fi2.version_id.as_bytes()), None);
    // assert_eq!(xl2.data.entries(), 1);
    //
    // // Re-add
    // xl2.data.replace(fi2.version_id.as_bytes(), fi2.data);
    // assert_eq!(xl2.data.entries(), 2);
    //
    // // Replace entry
    // xl2.data.replace(fi.version_id.as_bytes(), data2);
    // assert_eq!(xl2.data.find(fi.version_id.as_bytes()), Some(data2));
    //
    // // Rename key
    // assert!(xl2.data.rename(fi.version_id.as_bytes(), b"new-key"));
    // assert_eq!(xl2.data.find(b"new-key"), Some(data2));
    //
    // // Trim data
    // let trimmed = xl_meta_v2_trim_data(&serialized);
    // let mut xl3 = xlMetaV2::new();
    // xl3.load(&trimmed).unwrap();
    // assert_eq!(xl3.data.len(), 0);
    //
    // // Corrupted metadata should be detected
    // let mut corrupted = trimmed.clone();
    // corrupted[corrupted.len() - 10] += 10;
    // assert!(xl3.load(&corrupted).is_err());
}

/// 测试 xlMetaV2Object.UsesDataDir 判断版本是否使用数据目录
///
/// 场景:
/// - Transitioned 版本 (有 metaTierStatus=COMPLETE) → uses=false
/// - Transitioned + 正在恢复 (AmzRestore ongoing) → uses=false
/// - Transitioned + 已恢复 (AmzRestore completed, 未过期) → uses=true
/// - Transitioned + 恢复已过期 → uses=false
/// - 无 ILM 的普通版本 → uses=true
///
/// 对应 Go: TestUsesDataDir
#[test]
#[ignore]
fn test_uses_data_dir() {
    // TODO: implement when xlMetaV2Object.UsesDataDir is available
    // let v_id = uuid::Uuid::new_v4();
    // let data_dir = uuid::Uuid::new_v4();
    //
    // // Transitioned
    // let transitioned: HashMap<String, Vec<u8>> = HashMap::from([
    //     ("x-minio-internal-transition-status".into(), b"COMPLETE".to_vec()),
    // ]);
    //
    // // ... test cases similar to Go test
    //
    // // Not transitioned → uses data dir
    // let meta = xlMetaV2Object {
    //     version_id: v_id,
    //     data_dir: data_dir,
    //     ..Default::default()
    // };
    // assert!(meta.uses_data_dir());
}

/// 测试 DeleteVersion 正确处理共享 DataDir
///
/// 场景:
/// - 内联数据版本不计入共享
/// - Transitioned 版本不计入共享
/// - 恢复中的 transitioned 版本不计入共享
/// - 已恢复版本计入共享
/// - 普通磁盘版本计入共享
/// - 删除版本时返回正确的 dataDir (无其他共享者时)
///
/// 对应 Go: TestDeleteVersionWithSharedDataDir
#[test]
#[ignore]
fn test_delete_version_with_shared_data_dir() {
    // TODO: implement when xlMetaV2.DeleteVersion with shared data dir is available
    // let mut xl = xlMetaV2::new();
    // let data = b"some object data";
    // let data2 = b"some other object data";
    //
    // let fi = FileInfo {
    //     volume: "volume".into(),
    //     name: "object-name".into(),
    //     ..Default::default()
    // };
    //
    // // Create versions with varying data dir sharing
    // // ... test cases
    // // Verify SharedDataDirCount returns correct share count
    // // Verify DeleteVersion returns expected data dir
}

/// 测试 xlMetaV2Shallow.Load 加载和索引一致性
///
/// 从 testdata/xl.meta-v1.2.zst 加载数据，验证:
/// - 855 个版本正确加载
/// - sort_by_mod_time 正确排序
/// - header 与 meta 一致
/// - Roundtrip (load → append_to → load) 一致
/// - compressed index 兼容性
///
/// 对应 Go: Test_xlMetaV2Shallow_Load
#[test]
#[ignore]
fn test_xl_meta_v2_shallow_load() {
    // TODO: implement when xlMetaV2Shallow/xlMetaV2 is available
    // let data = load_compressed_test_data("testdata/xl.meta-v1.2.zst");
    //
    // // Legacy load
    // let mut xl = xlMetaV2::new();
    // xl.load(&data).unwrap();
    // assert_eq!(xl.versions.len(), 855);
    //
    // xl.sort_by_mod_time();
    // assert!(xl.versions.windows(2).all(|w| w[0].header.mod_time >= w[1].header.mod_time));
    //
    // // Roundtrip
    // let data2 = xl.append_to(&mut vec![]).unwrap();
    // let mut xl2 = xlMetaV2::new();
    // xl2.load(&data2).unwrap();
    // assert_eq!(xl2.versions.len(), 855);
}

/// 测试时间戳加载和签名转换
///
/// 验证旧格式 ReplicationTimestamp 和 ReplicaTimestamp 的
/// 时区转换 (从 +08:00 到 Z) 和签名升级。
///
/// 对应 Go: Test_xlMetaV2Shallow_LoadTimeStamp
#[test]
#[ignore]
fn test_xl_meta_v2_shallow_load_timestamp() {
    // TODO: implement when xlMetaV2 timestamp conversion is available
    // let data = get_raw_timestamp_test_data();
    // let mut xl = xlMetaV2::new();
    // xl.load(&data).unwrap();
    //
    // let v0 = &xl.versions[0];
    // // Signature should be converted
    // let want_sig = [0x1e, 0x5f, 0xba, 0x4a];
    // assert_eq!(v0.header.signature, want_sig);
    //
    // // Timestamps should be in UTC
    // let want_ts = "2022-10-27T07:40:53.195813291Z";
    // let got = get_meta_sys_value(&v, RESERVED_METADATA_PREFIX_LOWER, REPLICATION_TIMESTAMP);
    // assert_eq!(got, want_ts);
}

/// 测试 mergeXLV2Versions 多盘版本合并
///
/// 从 testdata/xl-meta-consist.zip 加载多个磁盘的版本列表,
/// 在不同 quorum 下测试合并结果包含有效版本。
///
/// 多项子测试:
/// - non-strict mode
/// - strict mode
/// - 签名变异
/// - modtime 变异
/// - flags 变异
/// - versionID 变异
///
/// 对应 Go: Test_mergeXLV2Versions
#[test]
#[ignore]
fn test_merge_xl_v2_versions() {
    // TODO: implement when merge_xl_v2_versions() is available
    // let data = load_test_zip("testdata/xl-meta-consist.zip");
    // let mut vers: Vec<Vec<xlMetaV2ShallowVersion>> = Vec::new();
    // for buf in data {
    //     let mut xl = xlMetaV2::new();
    //     xl.load_or_convert(&buf).unwrap();
    //     vers.push(xl.versions);
    // }
    //
    // // Test merge for different quorum values
    // for i in 0..vers.len() {
    //     let merged = merge_xl_v2_versions(i, false, 0, &vers).unwrap();
    //     assert!(!merged.is_empty(), "Did not get any results");
    //     for ver in &merged {
    //         assert_ne!(ver.header.type_, invalid_version_type, "Invalid result");
    //     }
    // }
}

/// 测试 mergeXLV2Versions 特定场景: 删除标记与对象的合并
///
/// 场景:
/// - 对象仅出现在 1 个磁盘 → 不满足 quorum → 不包含
/// - 对象出现在 2+ 磁盘 → 满足 quorum → 包含
/// - 删除标记出现在 1 个磁盘 → 不满足 quorum → 不包含
/// - 删除标记出现在 2+ 磁盘 → 满足 quorum → 包含
/// - 16-stripe 场景验证
///
/// 对应 Go: Test_mergeXLV2Versions2
#[test]
#[ignore]
fn test_merge_xl_v2_versions2() {
    // TODO: implement when merge_xl_v2_versions() is available
    // // Construct vDelMarker and vObj as shallow versions
    // // Test different disk layouts
    // let test_cases = vec![
    //     ("obj-on-one", input_1_disk_has_object, 2, vec![v_del_marker]),
    //     ("obj-on-two", input_2_disks_have_object, 2, vec![v_del_marker, v_obj]),
    //     ("del-on-one", input_1_disk_has_del, 2, vec![v_obj]),
    //     ("del-on-two", input_2_disks_have_del, 2, vec![v_del_marker, v_obj]),
    // ];
    //
    // for (name, input, quorum, want) in test_cases {
    //     for seed in 0..50 {
    //         let mut rng = rand::thread_rng();
    //         // Shuffle input order
    //         let got = merge_xl_v2_versions(quorum, true, 0, &input);
    //         assert_eq!(got, want, "Test '{}' seed {} failed", name, seed);
    //     }
    // }
}

/// 测试 mergeEntryChannels 通道合并
///
/// 从 testdata/xl-meta-merge.zip 加载多个 metaCacheEntry，
/// 打乱顺序后通过 channel 合并，验证结果包含 3 个版本且排序正确。
///
/// 对应 Go: Test_mergeEntryChannels
#[test]
#[ignore]
fn test_merge_entry_channels() {
    // TODO: implement when merge_entry_channels() is available
    // let data = load_test_zip("testdata/xl-meta-merge.zip");
    // let mut entries = Vec::new();
    // for buf in data {
    //     let trimmed = xl_meta_v2_trim_data(&buf);
    //     entries.push(meta_cache_entry {
    //         name: "a".into(),
    //         metadata: trimmed,
    //     });
    // }
    //
    // for seed in 0..100 {
    //     let mut rng = rand::thread_rng();
    //     entries.shuffle(&mut rng);
    //     // ... create channels and merge
    //     let result = merge_entry_channels(channels, out, 1).await?;
    //     assert_eq!(result.versions.len(), 3);
    //     // Verify sorted
    // }
}

/// 测试 XMinIOHealingSkip - healing 标记不被保留在 ToFileInfo 中
///
/// 场景:
/// - 在 FileInfo 上设置 Healing 标记
/// - 添加版本到 xlMetaV2
/// - 调用 ToFileInfo 后 Healing 应为 false
///
/// 对应 Go: TestXMinIOHealingSkip
#[test]
#[ignore]
fn test_x_min_io_healing_skip() {
    // TODO: implement when xlMetaV2.ToFileInfo is available
    // let mut xl = xlMetaV2::new();
    // let mut fi = FileInfo {
    //     volume: "volume".into(),
    //     name: "object-name".into(),
    //     version_id: "756100c6-b393-4981-928a-d49bbc164741".into(),
    //     is_latest: true,
    //     size: 1024,
    //     ..Default::default()
    // };
    // fi.set_healing();
    // xl.add_version(&fi).unwrap();
    //
    // let fi_out = xl.to_file_info("volume", "object-name", &fi.version_id, false, true).unwrap();
    // assert!(!fi_out.healing(), "Expected fi.healing() to be false");
}
