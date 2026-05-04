//! IO 工具函数测试
//!
//! 对应 Go: internal/ioutil/ioutil_test.go
//!
//! 测试 DeadlineWorker, DeadlineWriter, WriteOnClose, AppendFile,
//! SkipReader, SameFile, CopyAligned 等工具。

use storage::*;

/// 测试 DeadlineWorker 超时工作器
///
/// 场景:
/// - 耗时 600ms 的工作在 500ms 超时 → context::DeadlineExceeded
/// - 耗时 450ms 的工作在 500ms 超时 → 成功
///
/// 对应 Go: TestDeadlineWorker
#[test]
#[ignore]
fn test_deadline_worker() {
    // TODO: implement when DeadlineWorker is available
    // let worker = DeadlineWorker::new(Duration::from_millis(500));
    //
    // // Should timeout
    // let result = worker.run(|| {
    //     std::thread::sleep(Duration::from_millis(600));
    //     Ok(())
    // });
    // assert_eq!(result.unwrap_err(), std::io::ErrorKind::TimedOut);
    //
    // // Should succeed
    // let result = worker.run(|| {
    //     std::thread::sleep(Duration::from_millis(450));
    //     Ok(())
    // });
    // assert!(result.is_ok());
}

/// 测试 DeadlineWriter 超时写入器
///
/// 场景:
/// - 写入耗时 500ms 的目标, 450ms 超时 → DeadlineExceeded
/// - 写入耗时 100ms 的目标, 600ms 超时 → 成功, 写入 4 字节
///
/// 对应 Go: TestDeadlineWriter
#[test]
#[ignore]
fn test_deadline_writer() {
    // TODO: implement when DeadlineWriter is available
    // let w = DeadlineWriter::new(SleepWriter::new(Duration::from_millis(500)), Duration::from_millis(450));
    // let result = w.write(b"1");
    // assert_eq!(result.unwrap_err(), std::io::ErrorKind::TimedOut);
    //
    // let w = DeadlineWriter::new(SleepWriter::new(Duration::from_millis(100)), Duration::from_millis(600));
    // let n = w.write(b"abcd").unwrap();
    // assert_eq!(n, 4);
}

/// 测试 WriteOnClose 写入时关闭标记
///
/// 场景:
/// - 新建 WriteOnClose → HasWritten = false
/// - 写入后 → HasWritten = true
/// - Close 后 → HasWritten = true
///
/// 对应 Go: TestCloseOnWriter
#[test]
#[ignore]
fn test_close_on_writer() {
    // TODO: implement when WriteOnClose is available
    // let mut writer = WriteOnClose::new(std::io::sink());
    // assert!(!writer.has_written());
    //
    // writer.write(&[]).unwrap();
    // assert!(writer.has_written());
    //
    // let mut writer = WriteOnClose::new(std::io::sink());
    // writer.close().unwrap();
    // assert!(writer.has_written());
}

/// 测试 AppendFile 文件追加
///
/// 场景:
/// - 将文件 b 的内容追加到文件 a
/// - 验证 a 的内容为两个文件的拼接
///
/// 对应 Go: TestAppendFile
#[test]
#[ignore]
fn test_append_file() {
    // TODO: implement when AppendFile (file-level, not StorageAPI) is available
    // let tmp = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
    // std::fs::create_dir_all(&tmp).unwrap();
    //
    // let name1 = tmp.join("file1");
    // std::fs::write(&name1, b"aaaaaaaaaa").unwrap();
    //
    // let name2 = tmp.join("file2");
    // std::fs::write(&name2, b"bbbbbbbbbb").unwrap();
    //
    // AppendFile::append(&name1, &name2, false).unwrap();
    // let content = std::fs::read_to_string(&name1).unwrap();
    // assert_eq!(content, "aaaaaaaaaabbbbbbbbbb");
}

/// 测试 SkipReader 跳过读取
///
/// 场景:
/// - 空数据, skip=0 → ""
/// - 空数据, skip=1 → ""
/// - "abc", skip=0 → "abc"
/// - "abc", skip=1 → "bc"
/// - "abc", skip=2 → "c"
/// - "abc", skip=3 → ""
/// - "abc", skip=4 → ""
///
/// 对应 Go: TestSkipReader
#[test]
#[ignore]
fn test_skip_reader() {
    // TODO: implement when SkipReader is available
    // let cases = vec![
    //     (b"", 0, b""),
    //     (b"", 1, b""),
    //     (b"abc", 0, b"abc"),
    //     (b"abc", 1, b"bc"),
    //     (b"abc", 2, b"c"),
    //     (b"abc", 3, b""),
    //     (b"abc", 4, b""),
    // ];
    // for (i, (data, skip, expected)) in cases.iter().enumerate() {
    //     let reader = SkipReader::new(std::io::Cursor::new(data), *skip);
    //     let result = reader.read_to_end().unwrap();
    //     assert_eq!(&result, expected, "Case {} failed", i);
    // }
}

/// 测试 SameFile 比较两个文件是否相同
///
/// 场景:
/// - 同一个文件的两个 stat → 相同
/// - 修改文件后 stat → 不相同
///
/// 对应 Go: TestSameFile
#[test]
#[ignore]
fn test_same_file() {
    // TODO: implement when SameFile() is available
    // let tmp = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
    // std::fs::write(&tmp, b"").unwrap();
    //
    // let fi1 = std::fs::metadata(&tmp).unwrap();
    // let fi2 = std::fs::metadata(&tmp).unwrap();
    // assert!(same_file(&fi1, &fi2));
    //
    // std::fs::write(&tmp, b"aaa").unwrap();
    // let fi2_modified = std::fs::metadata(&tmp).unwrap();
    // assert!(!same_file(&fi1, &fi2_modified));
}

/// 测试 CopyAligned 对齐拷贝
///
/// 场景:
/// - 源数据 5 字节, 读取 5 字节 → 成功写入 5 字节
/// - 源数据完整, 读取全部 → 写入全部
///
/// 对应 Go: TestCopyAligned
#[test]
#[ignore]
fn test_copy_aligned() {
    // TODO: implement when CopyAligned() is available
    // let tmp = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
    // let f = std::fs::File::create(&tmp).unwrap();
    //
    // let r = std::io::Cursor::new(b"hello world");
    //
    // // Use O_DIRECT aligned buffer
    // let buf = ODirectPoolSmall::get().unwrap();
    //
    // // Write first 5 bytes
    // let written = copy_aligned(&f, r.take(5), buf, r.len(), &f).unwrap();
    // assert_eq!(written, 5);
    //
    // // Write all
    // r.seek(std::io::SeekFrom::Start(0)).unwrap();
    // let written = copy_aligned(&f, r, buf, r.len(), &f).unwrap();
    // assert_eq!(written, r.len());
}
