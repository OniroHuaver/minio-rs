//! xlMetaV2 data format tests
//!
//! Tests xlMetaV2 core functionality:
//! - xl.meta data read/write, load/roundtrip
//! - Data inline storage, lookup, delete, replace, rename
//! - xlMetaV2TrimData trimming
//! - UsesDataDir determination
//! - Shared DataDir version deletion
//! - mergeXLV2Versions multi-disk version merge
//! - Timestamp loading and signature conversion
//! - LoadOrConvert compatibility


/// Tests read_xl_meta_no_data for corrupt xl.meta
///
/// Scenarios: Reading a data-containing xl.meta without data segment should error.
#[test]
#[ignore]
fn test_read_xl_meta_no_data() {
    // TODO: implement when read_xl_meta_no_data() is available
    // let corrupt_data = load_test_data("testdata/xl.meta-corrupt.gz");
    // let result = read_xl_meta_no_data(&corrupt_data[..], corrupt_data.len() as i64);
    // assert!(result.is_err(), "expected error but returned success");
}

/// Tests xlMetaV2 data format: add version, serialize, deserialize,
/// data lookup/delete/replace/rename/trim
///
/// Scenarios:
/// - Add two versions with inline data
/// - Serialize then deserialize to verify data integrity
/// - data.list() returns 2 entries
/// - data.find() queries by versionID
/// - data.remove() deletes => entries=1
/// - data.replace() replaces data => entries=2
/// - data.rename() renames key
/// - xlMetaV2TrimData trims => data length = 0
/// - Corrupt metadata can be detected
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

/// Tests xlMetaV2Object.UsesDataDir for data directory usage
///
/// Scenarios:
/// - Transitioned version (metaTierStatus=COMPLETE) => uses=false
/// - Transitioned + restoring (AmzRestore ongoing) => uses=false
/// - Transitioned + restored (AmzRestore completed, not expired) => uses=true
/// - Transitioned + restore expired => uses=false
/// - Normal version without ILM => uses=true
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

/// Tests DeleteVersion handling of shared DataDir
///
/// Scenarios:
/// - Inline data versions do not count toward sharing
/// - Transitioned versions do not count toward sharing
/// - Restoring transitioned versions do not count toward sharing
/// - Restored versions count toward sharing
/// - Normal disk versions count toward sharing
/// - Returns correct dataDir on delete (when no other sharers)
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

/// Tests xlMetaV2Shallow.Load loading and index consistency
///
/// Load data from testdata/xl.meta-v1.2.zst, verify:
/// - 855 versions loaded correctly
/// - sort_by_mod_time sorts correctly
/// - header and meta are consistent
/// - Roundtrip (load -> append_to -> load) consistency
/// - Compressed index compatibility
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

/// Tests timestamp loading and signature conversion
///
/// Verify timezone conversion (from +08:00 to Z) and signature
/// upgrade for old format ReplicationTimestamp and ReplicaTimestamp.
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

/// Tests mergeXLV2Versions multi-disk version merge
///
/// Load version lists from multiple disks via testdata/xl-meta-consist.zip,
/// test merge results contain valid versions under different quorum values.
///
/// Subtests:
/// - non-strict mode
/// - strict mode
/// - signature variation
/// - modtime variation
/// - flags variation
/// - versionID variation
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

/// Tests mergeXLV2Versions specific scenarios: delete marker and object merge
///
/// Scenarios:
/// - Object appears on only 1 disk -> no quorum -> not included
/// - Object appears on 2+ disks -> quorum met -> included
/// - Delete marker appears on only 1 disk -> no quorum -> not included
/// - Delete marker appears on 2+ disks -> quorum met -> included
/// - 16-stripe scenario verification
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

/// Tests mergeEntryChannels channel merge
///
/// Load multiple metaCacheEntry from testdata/xl-meta-merge.zip,
/// shuffle then merge via channels, verify result contains 3 versions and is sorted correctly.
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

/// Tests XMinIOHealingSkip - healing flag is not retained in ToFileInfo
///
/// Scenarios:
/// - Set Healing flag on FileInfo
/// - Add version to xlMetaV2
/// - After ToFileInfo, Healing should be false
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
