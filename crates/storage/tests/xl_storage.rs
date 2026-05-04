//! xlStorage 测试
//!
//! 对应 Go: cmd/xl-storage_test.go
//!
//! 测试本地磁盘存储层 (xlStorage) 的全部 IO 操作：
//! - 磁盘初始化与校验 (NewXLStorage, CheckPathLength, IsValidVolname)
//! - 卷管理 (MakeVol, DeleteVol, StatVol, ListVols)
//! - 文件读写 (ReadAll, ReadFile, AppendFile)
//! - 元数据 (ReadVersion, StatInfoFile, ReadMetadata)
//! - 文件操作 (Delete, RenameFile, DeleteVersion)
//! - 位衰减校验 (ReadFileWithVerify, VerifyFile)
//! - 磁盘格式变更监测
//! - 空目录检测

use storage::*;

/// 测试 check_path_length 路径长度检查逻辑
///
/// 场景:
/// - "." 和 "/" 应被拒绝 (errFileAccessDenied)
/// - ".." 应被拒绝 (errFileAccessDenied)
/// - 超长路径 (> 1024 字符) 应被拒绝 (errFileNameTooLong)
/// - 正常路径应通过
///
/// 对应 Go: TestCheckPathLength
#[test]
#[ignore]
fn test_check_path_length() {
    // TODO: implement when check_path_length() is available
    // let test_cases = vec![
    //     (".", Err(Error::FileAccessDenied)),
    //     ("/", Err(Error::FileAccessDenied)),
    //     ("..", Err(Error::FileAccessDenied)),
    //     (long_path_str, Err(Error::FileNameTooLong)),
    //     ("data/valid/path.txt", Ok(())),
    // ];
    // for (path, expected) in test_cases {
    //     assert_eq!(check_path_length(path), expected);
    // }
}

/// 测试 isValidVolname 卷名校验逻辑
///
/// 场景:
/// - 合法卷名: "lol", "testbucket", "this.works.too.1", "una nina" 等应通过
/// - 非法卷名: "" (空), "/" (斜杠分隔符), "a" (长度 < 3) 应失败
///
/// 对应 Go: TestIsValidVolname
#[test]
#[ignore]
fn test_is_valid_volname() {
    // TODO: implement when is_valid_volname() is available
    // let valid_names = vec!["lol", "testbucket", "this.works.too.1", "una nina"];
    // let invalid_names = vec!["", "/", "a", "ab"];
    // for name in valid_names {
    //     assert!(is_valid_volname(name), "Expected '{}' to be valid", name);
    // }
    // for name in invalid_names {
    //     assert!(!is_valid_volname(name), "Expected '{}' to be invalid", name);
    // }
}

/// 测试 xlStorage.get_disk_info() 磁盘信息获取
///
/// 场景:
/// - 正常磁盘路径应返回 DiskInfo 且无错误
/// - 不存在的磁盘路径应返回 errDiskNotFound
///
/// 对应 Go: TestXLStorageGetDiskInfo
#[test]
#[ignore]
fn test_xl_storage_get_disk_info() {
    // TODO: implement when xlStorage and get_disk_info() are available
    // let tmp = std::path::PathBuf::from(std::env::temp_dir()).join(uuid::Uuid::new_v4().to_string());
    // std::fs::create_dir_all(&tmp).unwrap();
    //
    // // Valid path
    // let info = get_disk_info(tmp.to_str().unwrap()).unwrap();
    // assert!(info.total > 0);
    //
    // // Non-existent path
    // let result = get_disk_info("/nonexistent-dir");
    // assert_eq!(result.unwrap_err(), Error::DiskNotFound);
}

/// 测试 is_dir_empty 空目录检测
///
/// 场景:
/// - 不存在的目录应返回 false
/// - 文件 (非目录) 应返回 false
/// - 真正的空目录应返回 true
///
/// 对应 Go: TestXLStorageIsDirEmpty
#[test]
#[ignore]
fn test_xl_storage_is_dir_empty() {
    // TODO: implement when is_dir_empty() is available
    // let tmp = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
    // std::fs::create_dir_all(&tmp).unwrap();
    //
    // // Non-existent dir
    // let dir1 = tmp.join("non-existent");
    // assert!(!is_dir_empty(dir1.to_str().unwrap(), true));
    //
    // // File (not a directory)
    // let dir2 = tmp.join("file");
    // std::fs::write(&dir2, b"hello").unwrap();
    // assert!(!is_dir_empty(dir2.to_str().unwrap(), true));
    //
    // // Empty dir
    // let dir3 = tmp.join("empty");
    // std::fs::create_dir_all(&dir3).unwrap();
    // assert!(is_dir_empty(dir3.to_str().unwrap(), true));
}

/// 测试 ReadVersion 读取旧版 xl.json 格式 (v1 legacy)
///
/// 场景:
/// - 写入 xl.json 格式的元数据文件
/// - 使用 ReadVersion 读取应成功
/// - 返回的 FileInfo 应标记 XLV1 = true
///
/// 对应 Go: TestXLStorageReadVersionLegacy
#[test]
#[ignore]
fn test_xl_storage_read_version_legacy() {
    // TODO: implement when xlStorage and ReadVersion are available
    // let legacy_json = r#"{"version":"1.0.1","format":"xl",...}"#;
    // let (storage, _path) = new_xl_storage_test_setup()?;
    // storage.make_vol("exists-legacy").await?;
    // storage.append_file("exists-legacy", "as-file/xl.json", legacy_json.as_bytes()).await?;
    // let fi = storage.read_version("", "exists-legacy", "as-file", "", Default::default()).await?;
    // assert!(fi.xl_v1, "Expected legacy xl.json to be interpreted as v1");
}

/// 测试 xlStorage.ReadVersion 版本元数据读取
///
/// 场景:
/// - 不存在的 volume → errVolumeNotFound
/// - 不存在的文件 → errFileNotFound
/// - 路径是目录/前缀 → errFileNotFound
/// - 有效路径 → 成功读取
/// - 非法 volume 名 → errVolumeNotFound
///
/// 对应 Go: TestXLStorageReadVersion
#[test]
#[ignore]
fn test_xl_storage_read_version() {
    // TODO: implement when xlStorage and ReadVersion are available
    // let (storage, _path) = new_xl_storage_test_setup()?;
    // storage.make_vol("exists").await?;
    // storage.append_file("exists", "as-file/xl.meta", xl_meta_bytes).await?;
    //
    // let cases = vec![
    //     ("i-dont-exist", "", Err(Error::VolumeNotFound)),
    //     ("exists", "as-file-not-found", Err(Error::FileNotFound)),
    //     ("exists", "as-directory", Err(Error::FileNotFound)),
    //     ("exists", "as-file-parent/as-file", Err(Error::FileNotFound)),
    //     ("exists", "as-file", Ok(())),
    //     ("ab", "as-file", Err(Error::VolumeNotFound)),
    // ];
    // for (vol, path, expected) in cases {
    //     let result = storage.read_version("", vol, path, "", Default::default()).await;
    //     assert_eq!(result.map(|_| ()), expected.map(|_| ()));
    // }
}

/// 测试 xlStorage.ReadAll 读取文件全部内容
///
/// 场景:
/// - 不存在的 volume → errVolumeNotFound
/// - 不存在的文件 → errFileNotFound
/// - 路径是目录 → errFileNotFound
/// - 路径是非叶子节点 → errFileNotFound
/// - 有效文件 → 返回完整数据
/// - 非法 volume 名 → errVolumeNotFound
///
/// 对应 Go: TestXLStorageReadAll
#[test]
#[ignore]
fn test_xl_storage_read_all() {
    // TODO: implement when xlStorage and ReadAll are available
    // let (storage, _path) = new_xl_storage_test_setup()?;
    // storage.make_vol("exists").await?;
    // storage.append_file("exists", "as-file", b"Hello, World").await?;
    //
    // let cases = vec![
    //     ("i-dont-exist", "", Err(Error::VolumeNotFound)),
    //     ("exists", "as-file-not-found", Err(Error::FileNotFound)),
    //     ("exists", "as-directory", Err(Error::FileNotFound)),
    //     ("exists", "as-file-parent/as-file", Err(Error::FileNotFound)),
    //     ("exists", "as-file", Ok(b"Hello, World".to_vec())),
    //     ("ab", "as-file", Err(Error::VolumeNotFound)),
    // ];
    // for (vol, path, expected) in cases {
    //     let result = storage.read_all(vol, path).await;
    //     match (&result, &expected) {
    //         (Ok(data), Ok(exp)) => assert_eq!(data, exp),
    //         (Err(e), Err(_)) => {} // error types match
    //         _ => panic!("Mismatch: got {:?}, expected {:?}", result, expected),
    //     }
    // }
}

/// 测试 NewXLStorage xlStorage 初始化
///
/// 场景:
/// - 空路径 → errInvalidArgument
/// - 临时目录不存在 → 自动创建成功
/// - 路径是文件而非目录 → errDiskNotDir
///
/// 对应 Go: TestNewXLStorage
#[test]
#[ignore]
fn test_new_xl_storage() {
    // TODO: implement when new_local_xl_storage() is available
    // // Empty path
    // let result = new_local_xl_storage("");
    // assert_eq!(result.unwrap_err(), Error::InvalidArgument);
    //
    // // Temp dir (auto-created)
    // let tmp = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
    // let result = new_local_xl_storage(tmp.to_str().unwrap());
    // assert!(result.is_ok());
    //
    // // Path is a file
    // let tmp_file = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
    // std::fs::write(&tmp_file, b"").unwrap();
    // let result = new_local_xl_storage(tmp_file.to_str().unwrap());
    // assert_eq!(result.unwrap_err(), Error::DiskNotDir);
}

/// 测试 xlStorage.MakeVol 卷创建
///
/// 场景:
/// - 正常创建应成功
/// - 路径已存在文件 → errVolumeExists
/// - 目录已存在 → errVolumeExists
/// - 非法 volume 名 → errInvalidArgument
/// - 权限不足 → errDiskAccessDenied (Unix)
///
/// 对应 Go: TestXLStorageMakeVol
#[test]
#[ignore]
fn test_xl_storage_make_vol() {
    // TODO: implement when xlStorage and MakeVol are available
    // let (storage, path) = new_xl_storage_test_setup()?;
    //
    // // Create file at volume path
    // std::fs::write(std::path::Path::new(&path).join("vol-as-file"), b"").unwrap();
    // std::fs::create_dir_all(std::path::Path::new(&path).join("existing-vol")).unwrap();
    //
    // let cases = vec![
    //     ("success-vol", Ok(())),
    //     ("vol-as-file", Err(Error::VolumeExists)),
    //     ("existing-vol", Err(Error::VolumeExists)),
    //     ("ab", Err(Error::InvalidArgument)),
    // ];
    // for (vol, expected) in cases {
    //     let result = storage.make_vol(vol).await;
    //     assert_eq!(result, expected);
    // }
}

/// 测试 xlStorage.DeleteVol 卷删除
///
/// 场景:
/// - 空卷 → 删除成功
/// - 不存在的卷 → errVolumeNotFound
/// - 非空卷 → errVolumeNotEmpty
/// - 非法卷名 → errVolumeNotFound
/// - 权限不足 → errDiskAccessDenied
/// - 磁盘已删除 → errDiskNotFound
///
/// 对应 Go: TestXLStorageDeleteVol
#[test]
#[ignore]
fn test_xl_storage_delete_vol() {
    // TODO: implement when xlStorage and DeleteVol are available
    // let (storage, path) = new_xl_storage_test_setup()?;
    // storage.make_vol("success-vol").await?;
    // // Create non-empty vol
    // let nonempty = std::path::Path::new(&path).join("nonempty-vol");
    // std::fs::create_dir_all(&nonempty).unwrap();
    // std::fs::write(nonempty.join("test-file"), b"").unwrap();
    //
    // let cases = vec![
    //     ("success-vol", Ok(())),
    //     ("nonexistent-vol", Err(Error::VolumeNotFound)),
    //     ("nonempty-vol", Err(Error::VolumeNotEmpty)),
    //     ("ab", Err(Error::VolumeNotFound)),
    // ];
    // for (vol, expected) in cases {
    //     let result = storage.delete_vol(vol, false).await;
    //     assert_eq!(result, expected);
    // }
}

/// 测试 xlStorage.StatVol 卷状态查询
///
/// 场景:
/// - 存在的卷 → 返回 VolInfo
/// - 不存在的卷 → errVolumeNotFound
/// - 非法卷名 → errVolumeNotFound
/// - 磁盘已删除 → errDiskNotFound
///
/// 对应 Go: TestXLStorageStatVol
#[test]
#[ignore]
fn test_xl_storage_stat_vol() {
    // TODO: implement when xlStorage and StatVol are available
    // let (storage, _path) = new_xl_storage_test_setup()?;
    // storage.make_vol("success-vol").await?;
    //
    // let cases = vec![
    //     ("success-vol", Ok(())),
    //     ("nonexistent-vol", Err(Error::VolumeNotFound)),
    //     ("ab", Err(Error::VolumeNotFound)),
    // ];
    // for (vol, expected) in cases {
    //     let result = storage.stat_vol(vol).await;
    //     match expected {
    //         Ok(()) => assert!(result.is_ok()),
    //         Err(e) => assert_eq!(result.unwrap_err(), e),
    //     }
    // }
}

/// 测试 xlStorage.ListVols 卷列表
///
/// 场景:
/// - 空列表时只返回 minioMetaBucket
/// - 创建卷后列表应包含新卷
/// - 磁盘删除后 → errDiskNotFound
///
/// 对应 Go: TestXLStorageListVols
#[test]
#[ignore]
fn test_xl_storage_list_vols() {
    // TODO: implement when xlStorage and ListVols are available
    // let (storage, path) = new_xl_storage_test_setup()?;
    //
    // // Initially should have .minio.sys only
    // let vols = storage.list_vols().await?;
    // assert_eq!(vols.len(), 1, "Expected 1 volume (minioMetaBucket)");
    //
    // // After creating a volume
    // storage.make_vol("success-vol").await?;
    // let vols = storage.list_vols().await?;
    // assert_eq!(vols.len(), 2);
    // assert!(vols.iter().any(|v| v.name == "success-vol"));
    //
    // // Removed disk
    // std::fs::remove_dir_all(&path).unwrap();
    // let result = storage.list_vols().await;
    // assert_eq!(result.unwrap_err(), Error::DiskNotFound);
}

/// 测试 xlStorage.ListDir 目录列表
///
/// 场景:
/// - 多层次目录结构列表
/// - 不存在的路径 → errFileNotFound
/// - 非法卷名 → errVolumeNotFound
/// - 不存在的卷 → errVolumeNotFound
///
/// 对应 Go: TestXLStorageListDir (the ListDir portion)
#[test]
#[ignore]
fn test_xl_storage_list_dir() {
    // TODO: implement when xlStorage and ListDir are available
    // let (storage, _path) = new_xl_storage_test_setup()?;
    // storage.make_vol("success-vol").await?;
    // storage.append_file("success-vol", "abc/def/ghi/success-file", b"Hello").await?;
    // storage.append_file("success-vol", "abc/xyz/ghi/success-file", b"Hello").await?;
    //
    // let cases = vec![
    //     (("success-vol", "abc"), Ok(vec!["def/", "xyz/"])),
    //     (("success-vol", "abc/def"), Ok(vec!["ghi/"])),
    //     (("success-vol", "abc/def/ghi"), Ok(vec!["success-file"])),
    //     (("success-vol", "abcdef"), Err(Error::FileNotFound)),
    //     (("ab", "success-file"), Err(Error::VolumeNotFound)),
    //     (("non-existent-vol", "success-file"), Err(Error::VolumeNotFound)),
    // ];
    // for ((vol, path), expected) in cases {
    //     let result = storage.list_dir("", vol, path, -1).await;
    //     match expected {
    //         Ok(entries) => {
    //             let got = result.unwrap();
    //             for e in entries { assert!(got.contains(&e)); }
    //         }
    //         Err(e) => assert_eq!(result.unwrap_err(), e),
    //     }
    // }
}

/// 测试 xlStorage.Delete 文件删除
///
/// 场景:
/// - 正常删除 → 成功
/// - 删除已删除文件 → 成功 (幂等)
/// - 卷名段 > 255 字符 → errVolumeNotFound
/// - 不存在的卷 → errVolumeNotFound
/// - 路径段 > 255 字符 → errFileNameTooLong
/// - 权限不足 → errFileAccessDenied
/// - 磁盘已删除 → errDiskNotFound
///
/// 对应 Go: TestXLStorageDeleteFile
#[test]
#[ignore]
fn test_xl_storage_delete_file() {
    // TODO: implement when xlStorage and Delete are available
    // let (storage, path) = new_xl_storage_test_setup()?;
    // storage.make_vol("success-vol").await?;
    // storage.append_file("success-vol", "success-file", b"Hello").await?;
    //
    // let cases = vec![
    //     (("success-vol", "success-file"), Ok(())),
    //     (("success-vol", "success-file"), Ok(())), // already deleted
    //     (("my", "success-file"), Err(Error::VolumeNotFound)),
    //     (("non-existent-vol", "success-file"), Err(Error::VolumeNotFound)),
    //     (("success-vol", &long_path), Err(Error::FileNameTooLong)),
    // ];
    // for ((vol, file), expected) in cases {
    //     let result = storage.delete(vol, file, DeleteOptions { recursive: false, immediate: false }).await;
    //     assert_eq!(result, expected);
    // }
}

/// 测试 xlStorage.ReadFile 范围读取
///
/// 场景:
/// - 偏移 0, 缓冲区合适 → 读取正确内容
/// - 层次路径 → 读取正确
/// - 读取目录 → errIsNotRegular
/// - 路径段 > 255 → errFileNameTooLong
/// - 路径总长 > 1024 → errFileNameTooLong
/// - 缓冲区大于数据 → io::UnexpectedEOF
/// - 偏移读取 → 正确内容
/// - 超出范围 → io::EOF
/// - 空卷名 → errVolumeNotFound
/// - 空文件名 → errIsNotRegular
/// - 负偏移 → 返回错误
///
/// 对应 Go: TestXLStorageReadFile
#[test]
#[ignore]
fn test_xl_storage_read_file() {
    // TODO: implement when xlStorage and ReadFile are available
    // let (storage, path) = new_xl_storage_test_setup()?;
    // storage.make_vol("success-vol").await?;
    // // Create directory to test is-not-regular
    // std::fs::create_dir_all(std::path::Path::new(&path).join("success-vol/object-as-dir")).unwrap();
    //
    // let test_data = b"hello, world";
    // // Create test files
    // storage.append_file("success-vol", "myobject", test_data).await?;
    // storage.append_file("success-vol", "path/to/my/object", test_data).await?;
    //
    // let v = new_bitrot_verifier(Algorithm::SHA256, sha256_hash(test_data));
    //
    // // Negative offset test
    // let result = storage.read_file("success-vol", "myobject", -1, &mut vec![0u8; 5], &v).await;
    // assert!(result.is_err());
    //
    // // Various read cases...
    // // Case: offset 0, buffer 5 → "hello"
    // let mut buf = vec![0u8; 5];
    // let n = storage.read_file("success-vol", "myobject", 0, &mut buf, &v).await.unwrap();
    // assert_eq!(n, 5);
    // assert_eq!(&buf, b"hello");
}

/// 测试 xlStorage.ReadFile 带位衰减校验
///
/// 使用不同 BitrotAlgorithm (SHA256, BLAKE2b512) 在不同偏移和长度下
/// 校验数据完整性。验证：
/// - 正确哈希 → 读取成功
/// - 错误哈希 → errFileCorrupt
///
/// 对应 Go: TestXLStorageReadFileWithVerify
#[test]
#[ignore]
fn test_xl_storage_read_file_with_verify() {
    // TODO: implement when xlStorage, ReadFile, and bitrot verification are available
    // let (storage, _path) = new_xl_storage_test_setup()?;
    // storage.make_vol("test-vol").await?;
    //
    // let data = random_vec(8 * 1024);
    // storage.append_file("test-vol", "myobject", &data).await?;
    //
    // let test_cases = vec![
    //     // (offset, length, algorithm, exp_error)
    //     (0, 100, Algorithm::SHA256, None),
    //     (25, 74, Algorithm::SHA256, None),
    //     (100, 0, Algorithm::SHA256, None),
    //     (1, 120, Algorithm::SHA256, Some(Error::FileCorrupt)),
    //     (0, 100, Algorithm::BLAKE2b512, Some(Error::FileCorrupt)),
    // ];
    //
    // for (offset, length, algo, exp_err) in test_cases {
    //     let h = algo.new_hasher();
    //     h.update(&data);
    //     if exp_err.is_some() {
    //         h.update(&[0]); // corrupt hash
    //     }
    //     let hash = h.finalize();
    //     let v = new_bitrot_verifier(algo, &hash);
    //
    //     let mut buf = vec![0u8; length as usize];
    //     let result = storage.read_file("test-vol", "myobject", offset, &mut buf, &v).await;
    //     match exp_err {
    //         None => assert!(result.is_ok()),
    //         Some(e) => assert_eq!(result.unwrap_err(), e),
    //     }
    // }
}

/// 测试磁盘格式变更 (format.json 中 diskID 改变) 导致操作失败
///
/// 场景:
/// - 创建磁盘后修改 format.json 中的 "this" 字段
/// - 再次 MakeVol 应失败 (errVolumeExists, 因为 diskID 不匹配)
///
/// 对应 Go: TestXLStorageFormatFileChange
#[test]
#[ignore]
fn test_xl_storage_format_file_change() {
    // TODO: implement when xlStorage disk ID checking is available
    // let (storage, path) = new_xl_storage_test_setup()?;
    // storage.make_vol("fail-vol").await?;
    //
    // // Change format.json to have a different disk ID
    // let format_path = format!("{}/.minio.sys/format.json", path);
    // let modified_format = r#"{"version":"1","format":"xl","id":"...","xl":{"version":"3","this":"randomid","sets":[...],"distributionAlgo":"CRCMOD"}}"#;
    // std::fs::write(&format_path, modified_format).unwrap();
    //
    // let result = storage.make_vol("fail-vol").await;
    // assert_eq!(result.unwrap_err(), Error::VolumeExists);
}

/// 测试 xlStorage.AppendFile 追加写入
///
/// 场景:
/// - 新建文件 → 成功
/// - 层次路径 → 成功
/// - 追加到已有文件 → 成功
/// - 写入目录路径 → errIsNotRegular
/// - 路径冲突 (文件 vs 目录) → errFileAccessDenied
/// - 路径段 > 255 → errFileNameTooLong
/// - 路径总长 > 1024 → errFileNameTooLong
/// - 权限不足 → errFileAccessDenied
/// - 非法卷名 → errVolumeNotFound
///
/// 对应 Go: TestXLStorageAppendFile
#[test]
#[ignore]
fn test_xl_storage_append_file() {
    // TODO: implement when xlStorage and AppendFile are available
    // let (storage, path) = new_xl_storage_test_setup()?;
    // storage.make_vol("success-vol").await?;
    // std::fs::create_dir_all(std::path::Path::new(&path).join("success-vol/object-as-dir")).unwrap();
    //
    // let cases = vec![
    //     ("myobject", Ok(())),
    //     ("path/to/my/object", Ok(())),
    //     ("myobject", Ok(())), // append
    //     ("object-as-dir", Err(Error::IsNotRegular)),
    //     ("myobject/testobject", Err(Error::FileAccessDenied)),
    //     (&long_segment_path, Err(Error::FileNameTooLong)),
    // ];
    // for (path, expected) in cases {
    //     let result = storage.append_file("success-vol", path, b"hello, world").await;
    //     assert_eq!(result, expected);
    // }
}

/// 测试 xlStorage.RenameFile 文件重命名/移动
///
/// 场景:
/// - 同一卷内重命名 → 成功
/// - 目录重命名 → 成功
/// - 覆盖目标文件 → 成功
/// - 源文件不存在 → errFileNotFound
/// - 类型不匹配 (文件↔目录) → errFileAccessDenied
/// - 目标目录已存在 → errFileAccessDenied
/// - 源卷不存在 → errVolumeNotFound
/// - 目标卷不存在 → errVolumeNotFound
/// - 非法卷名 → errVolumeNotFound
/// - 目标父路径是文件 → errFileAccessDenied
/// - 路径段 > 255 → errFileNameTooLong
///
/// 对应 Go: TestXLStorageRenameFile
#[test]
#[ignore]
fn test_xl_storage_rename_file() {
    // TODO: implement when xlStorage and RenameFile are available
    // let (storage, _path) = new_xl_storage_test_setup()?;
    // storage.make_vol("src-vol").await?;
    // storage.make_vol("dest-vol").await?;
    // storage.append_file("src-vol", "file1", b"Hello, world").await?;
    // storage.append_file("src-vol", "file2", b"Hello, world").await?;
    // storage.append_file("src-vol", "path/to/file1", b"Hello, world").await?;
    //
    // let cases = vec![
    //     (("src-vol", "file1", "dest-vol", "file-one"), Ok(())),
    //     (("src-vol", "path/", "dest-vol", "new-path/"), Ok(())),
    //     (("src-vol", "file2", "dest-vol", "file-one"), Ok(())), // overwrite
    //     (("src-vol", "non-existent", "dest-vol", "x"), Err(Error::FileNotFound)),
    //     (("src-vol", "path/", "dest-vol", "file-one"), Err(Error::FileAccessDenied)), // dir → file
    //     (("src-vol", "file4", "dest-vol", "new-path/"), Err(Error::FileAccessDenied)), // file → dir
    //     (("src-vol-non-existent", "file4", "dest-vol", "new-path/"), Err(Error::VolumeNotFound)),
    //     (("src-vol", "file4", "dest-vol-non-existent", "new-path/"), Err(Error::VolumeNotFound)),
    //     (("ab", "file4", "dest-vol", "new-path/"), Err(Error::VolumeNotFound)),
    // ];
    // for ((src_vol, src_path, dst_vol, dst_path), expected) in cases {
    //     let result = storage.rename_file(src_vol, src_path, dst_vol, dst_path).await;
    //     assert_eq!(result, expected);
    // }
}

/// 测试 xlStorage.DeleteVersion 版本删除与批量删除
///
/// 场景:
/// - 删除单个版本 → 成功
/// - 批量删除 (含已删除版本) → 成功
/// - 全部删除后 ReadVersion → errFileNotFound
///
/// 对应 Go: TestXLStorageDeleteVersion
#[test]
#[ignore]
fn test_xl_storage_delete_version() {
    // TODO: implement when xlStorage, DeleteVersion, DeleteVersions, WriteMetadata are available
    // let (storage, _path) = new_xl_storage_test_setup()?;
    // storage.make_vol("myvol-vol").await?;
    //
    // let mut versions = Vec::new();
    // for _ in 0..50 {
    //     let version_id = uuid::Uuid::new_v4().to_string();
    //     let fi = FileInfo {
    //         name: "my-object",
    //         volume: "myvol-vol",
    //         version_id: &version_id,
    //         mod_time: Utc::now(),
    //         size: 10000,
    //         erasure: ErasureInfo { data_blocks: 4, parity_blocks: 4, .. },
    //         ..Default::default()
    //     };
    //     storage.write_metadata("", "myvol-vol", "my-object", &fi).await?;
    //     versions.push(version_id);
    // }
    //
    // // Delete version 0
    // storage.delete_version("myvol-vol", "my-object", &versions[0], false).await?;
    //
    // // Bulk delete 10 versions
    // storage.delete_versions("myvol-vol", &versions[..10]).await?;
    //
    // // Delete all
    // storage.delete_versions("myvol-vol", &versions).await?;
    //
    // // Verify object is gone
    // let result = storage.read_version("", "myvol-vol", "my-object", "", Default::default()).await;
    // assert_eq!(result.unwrap_err(), Error::FileNotFound);
}

/// 测试 xlStorage.StatInfoFile 统计文件信息
///
/// 场景:
/// - 有效文件 → 返回 StatInfo
/// - 不存在 → errPathNotFound
/// - 路径是目录 → errPathNotFound
/// - 不存在的卷 → errVolumeNotFound
///
/// 对应 Go: TestXLStorageStatInfoFile
#[test]
#[ignore]
fn test_xl_storage_stat_info_file() {
    // TODO: implement when xlStorage and StatInfoFile are available
    // let (storage, _path) = new_xl_storage_test_setup()?;
    // storage.make_vol("success-vol").await?;
    // storage.append_file("success-vol", "success-file/xl.meta", b"Hello").await?;
    // storage.append_file("success-vol", "path/to/success-file/xl.meta", b"Hello").await?;
    //
    // let cases = vec![
    //     (("success-vol", "success-file"), Ok(())),
    //     (("success-vol", "path/to/success-file"), Ok(())),
    //     (("success-vol", "nonexistent-file"), Err(Error::PathNotFound)),
    //     (("success-vol", "path"), Err(Error::PathNotFound)),
    //     (("non-existent-vol", "success-file"), Err(Error::VolumeNotFound)),
    // ];
    // for ((vol, path), expected) in cases {
    //     let result = storage.stat_info_file(vol, &format!("{}/xl.meta", path), false).await;
    //     match expected {
    //         Ok(()) => assert!(result.is_ok()),
    //         Err(e) => assert_eq!(result.unwrap_err(), e),
    //     }
    // }
}

/// 测试 xlStorage.VerifyFile 全文件位衰减校验
///
/// 场景:
/// - 1) Whole-file bitrot check on proper file → 成功
/// - 2) Whole-file bitrot check on corrupted file → 失败
/// - 3) Streaming bitrot check on proper file → 成功
/// - 4) Streaming bitrot check on corrupted file → 失败
///
/// 对应 Go: TestXLStorageVerifyFile
#[test]
#[ignore]
fn test_xl_storage_verify_file() {
    // TODO: implement when xlStorage, VerifyFile, bitrot_verify are available
    // let (storage, path) = new_xl_storage_test_setup()?;
    // storage.make_vol("testvol").await?;
    //
    // let size: i64 = 4 * 1024 * 1024 + 100 * 1024;
    // let data = random_vec(size as usize);
    // let algo = Algorithm::HighwayHash256;
    // let hash = algo.hash(&data);
    //
    // // 1) Whole-file: write + verify good data
    // storage.write_all("testvol", "testfile", &data).await?;
    // storage.bitrot_verify(&format!("{}/testvol/testfile", path), size, algo, &hash, 0).await?;
    //
    // // 2) Corrupt file
    // storage.append_file("testvol", "testfile", b"a").await?;
    // let result = storage.bitrot_verify(&format!("{}/testvol/testfile", path), size, algo, &hash, 0).await;
    // assert!(result.is_err()); // wrong size
    //
    // storage.delete("testvol", "testfile", DeleteOptions { recursive: false, immediate: false }).await?;
    //
    // // 3) Streaming verify on proper file
    // storage.write_all("testvol", "testfile", &data).await?;
    // storage.bitrot_verify(&format!("{}/testvol/testfile", path), size, algo, &[], 1024*1024).await?;
    //
    // // 4) Corrupt streaming
    // // overwrite first 256 bytes
    // // ... then verify should fail
}

/// 测试 xlStorage.readMetadata 读取元数据时的路径校验
///
/// 场景:
/// - 超长对象名 (单段 > 255 字符) → errFileNameTooLong
///
/// 对应 Go: TestXLStorageReadMetadata
#[test]
#[ignore]
fn test_xl_storage_read_metadata() {
    // TODO: implement when xlStorage and read_metadata are available
    // let long_object = "A".repeat(256);
    // let tmp = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
    // let storage = new_local_xl_storage(tmp.to_str().unwrap()).unwrap();
    // storage.make_vol("test-vol").await?;
    //
    // let result = storage.read_metadata(&format!("{}/test-vol/{}", tmp.display(), long_object)).await;
    // assert_eq!(result.unwrap_err(), Error::FileNameTooLong);
}
