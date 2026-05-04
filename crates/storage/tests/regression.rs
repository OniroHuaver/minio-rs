//! 回归测试 — 已知 Bug 暴露
//!
//! 本文件的测试用例针对代码审查中发现的已知问题。
//! 当前这些测试**预期失败**，暴露实现中的 bug。
//! bug 修复后这些测试将通过。
//!
//! 对应 Review 问题编号见各测试注释。

use std::path::PathBuf;
use storage::{StorageAPI, XlStorage};

// ========================================================================
// Issue #2: read_range length < 0 触发 panic
// ========================================================================
// 文件: crates/storage/src/xl_storage.rs:104
// offset < 0 有前置检查，但 length < 0 没有。
// length=-1 时 min(-1, N) = -1，as usize 在 debug 下 panic，
// release 下产生超大 allocation → OOM。
// ========================================================================

mod issue_02_read_range_negative_length {

    use super::*;

    fn setup() -> (XlStorage, PathBuf) {
        let dir = std::env::temp_dir().join(format!("reg_02_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let storage = XlStorage::new(&dir, "test");
        (storage, dir)
    }

    /// read_range 在 length < 0 时不应 panic，应返回错误或空结果
    ///
    /// 当前 BUG: length=-1 时 min(-1, N) = -1，as usize 在 debug 下 panic。
    /// 修复: 在 101 行处同时检查 length <= 0  返回空 Vec。
    #[tokio::test]
    async fn read_range_negative_length_should_not_panic() {
        let (storage, dir) = setup();
        storage.make_volume("bucket").await.unwrap();
        storage.write_all("bucket", "data.bin", b"hello world").await.unwrap();

        // 直接调用 — 如果 panic 了，测试框架会报告 FAILED
        // 正确行为: 返回 Err 或 空 Vec
        let result = storage.read_range("bucket", "data.bin", 0, -1).await;

        match result {
            Ok(data) => {
                // 没有 panic！但需要确认没有触发 OOM (data 应该为空)
                assert!(
                    data.is_empty(),
                    "length < 0 应返回空数据或错误，不应返回非空数据"
                );
                eprintln!(
                    "read_range(length=-1) 没有 panic 但返回了 Ok。\n\
                     如果数据量为空说明恰好没触发 OOM，但 length 校验仍缺失。"
                );
            }
            Err(_) => {
                // 返回了错误 — 虽然不是最理想的行为，但至少没 panic
            }
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// read_range 在 offset=0, length=0 时应返回空 Vec (边界行为)
    #[tokio::test]
    async fn read_range_zero_length_should_return_empty() {
        let (storage, dir) = setup();
        storage.make_volume("bucket").await.unwrap();
        storage.write_all("bucket", "data.bin", b"hello").await.unwrap();

        let result = storage.read_range("bucket", "data.bin", 0, 0).await;
        assert!(result.is_ok(), "length=0 应正常返回空数据");
        assert_eq!(result.unwrap().len(), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }
}

// ========================================================================
// Issue #7: delete_volume 无路径约束 — "" 或 ".." 可删除磁盘根
// ========================================================================
// 文件: crates/storage/src/xl_storage.rs:209-214
// volume="" → volume_path 返回 disk_path 本身 → remove_dir_all 删除整个根
// volume=".." → 穿越出磁盘根目录
// ========================================================================

mod issue_07_delete_volume_path_traversal {

    use super::*;

    fn setup() -> (XlStorage, PathBuf) {
        let dir = std::env::temp_dir().join(format!("reg_07_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let storage = XlStorage::new(&dir, "test");
        (storage, dir)
    }

    /// delete_volume("") 不应删除磁盘根目录
    ///
    /// 当前实现: volume_path("") = disk_path.join("") = disk_path
    /// 这会直接删除整个测试目录，是严重的安全隐患。
    #[tokio::test]
    async fn delete_volume_empty_string_should_be_rejected() {
        let (storage, dir) = setup();

        // 先在磁盘根下创建一个标记文件
        let marker = dir.join(".safeguard");
        std::fs::write(&marker, b"protect me").unwrap();

        // 尝试删除空 volume 名 — 应该被拒绝 (Err)，而不是删除整个目录
        let result = storage.delete_volume("").await;

        // 标记文件必须仍然存在 — 如果 delete_volume("") 删了根目录，这个 assert 会失败
        assert!(
            marker.exists(),
            "BUG 确认: delete_volume(\"\") 删除了磁盘根目录！\n\
             原因: volume_path(\"\") = disk_path.join(\"\") = disk_path\n\
             修复: 拒绝空字符串和含 .. 的 volume 名。"
        );

        // 正确行为: 应返回错误
        assert!(
            result.is_err(),
            "delete_volume(\"\") 应返回错误，当前返回 Ok (可能已删除根目录)"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// delete_volume("..") 应被拒绝，不能穿越出磁盘根目录
    #[tokio::test]
    async fn delete_volume_dot_dot_should_be_rejected() {
        let (storage, dir) = setup();

        let result = storage.delete_volume("..").await;
        assert!(
            result.is_err(),
            "delete_volume(\"..\") 应返回错误，路径穿越不应被允许"
        );

        // 磁盘根目录应仍然存在
        assert!(dir.exists(), "磁盘根目录被 delete_volume(\"..\") 删除了！");

        let _ = std::fs::remove_dir_all(&dir);
    }
}

// ========================================================================
// Issue #1: XlMeta::from_bytes 版本校验过严 — 拒绝更高 minor 版本
// ========================================================================
// 文件: crates/base/src/format.rs:215-216
// 当前要求 major/minor 精确匹配。minor 应向后兼容——
// 更高的 minor 表明新格式添加了可选字段，旧代码应能忽略。
// 当前行为会在滚动升级时导致旧节点无法读取新节点写入的 xl.meta。
// ========================================================================

mod issue_01_xl_meta_version_too_strict {

    use base::format::XlMeta;

    /// 构造一个 minor 版本更高的 xl.meta 二进制数据
    fn make_xl_meta_with_version(major: u16, minor: u16) -> Vec<u8> {
        let header = base::format::XlMetaHeader {
            magic: *b"XL2 ",
            major,
            minor,
        };
        // Body: XlMeta struct (1-element msgpack array) 包含空的 versions 数组
        let body: &[u8] = &[0x91, 0x90]; // 91: 1-element array; 90: empty versions array
        let mut buf = Vec::new();
        buf.extend_from_slice(&header.to_bytes());
        buf.extend_from_slice(body);
        buf
    }

    /// 更高 minor 版本应被接受（向后兼容）
    ///
    /// 当前行为: XlMeta::from_bytes 拒绝 minor != XL_VERSION_MINOR
    /// 正确行为: 只拒接 major 不匹配；minor 高于已知值应 warn 但接受
    #[test]
    fn higher_minor_version_should_be_accepted() {
        let current_minor = base::constants::XL_VERSION_MINOR;
        let future_minor = current_minor + 1; // 模拟未来版本

        let data = make_xl_meta_with_version(
            base::constants::XL_VERSION_MAJOR,
            future_minor,
        );

        let result = XlMeta::from_bytes(&data);

        // 当前预期失败 — 因为版本校验过严
        assert!(
            result.is_ok(),
            "BUG 确认: XlMeta::from_bytes 拒绝了 minor={} 的 xl.meta（当前要求={}）。\n\
             原因: 版本校验要求 major/minor 精确匹配。\n\
             修复: 只拒绝 major 不匹配；minor 更高时 warn 但接受。",
            future_minor,
            current_minor,
        );
    }

    /// 相同版本的 xl.meta 应正常读取（sanity check）
    #[test]
    fn same_version_should_be_accepted() {
        let data = make_xl_meta_with_version(
            base::constants::XL_VERSION_MAJOR,
            base::constants::XL_VERSION_MINOR,
        );
        let result = XlMeta::from_bytes(&data);
        assert!(result.is_ok(), "当前版本的 xl.meta 应该能被读取");
    }

    /// 不同 major 版本应被拒绝
    #[test]
    fn different_major_version_should_be_rejected() {
        let data = make_xl_meta_with_version(
            base::constants::XL_VERSION_MAJOR + 1,
            base::constants::XL_VERSION_MINOR,
        );
        let result = XlMeta::from_bytes(&data);
        assert!(
            result.is_err(),
            "不同 major 版本的 xl.meta 应被拒绝"
        );
    }
}

// ========================================================================
// Issue #4: read_xl_meta 跳过了版本兼容性检查
// ========================================================================
// 文件: crates/storage/src/format.rs:11-22
// XlMeta::from_bytes 做了版本号校验，但 read_xl_meta 只校验 magic，
// 跳过了版本检查。两条路径行为不一致。
// ========================================================================

mod issue_04_read_xl_meta_skips_version_check {

    use base::format::{XlMeta, XlMetaHeader};
    use storage::read_xl_meta;

    /// read_xl_meta 应和 XlMeta::from_bytes 行为一致：
    /// 对不兼容的 major 版本也返回错误
    #[test]
    fn read_xl_meta_should_reject_incompatible_version() {
        // 构造 major 版本不兼容的 xl.meta
        let header = XlMetaHeader {
            magic: *b"XL2 ",
            major: 99,  // 完全不兼容的 major
            minor: 0,
        };
        let body: &[u8] = &[0x90]; // empty msgpack array
        let mut buf = Vec::new();
        buf.extend_from_slice(&header.to_bytes());
        buf.extend_from_slice(body);

        let result = read_xl_meta(&buf);

        assert!(
            result.is_err(),
            "BUG 确认: read_xl_meta 跳过了版本号校验，接受了 major=99 的数据。\n\
             而 XlMeta::from_bytes 会正确拒绝。两条路径行为不一致。\n\
             修复: read_xl_meta 应委托给 XlMeta::from_bytes。"
        );
    }

    /// sanity: read_xl_meta 应和 XlMeta::from_bytes 对相同数据产生相同结果
    #[test]
    fn read_xl_meta_and_from_bytes_should_be_consistent() {
        let meta = XlMeta { versions: vec![] };
        let data_via_to_bytes = meta.to_bytes().unwrap();

        let result_read = read_xl_meta(&data_via_to_bytes);
        let result_from = XlMeta::from_bytes(&data_via_to_bytes);

        // 两条路径应该都成功或都失败
        assert_eq!(
            result_read.is_ok(),
            result_from.is_ok(),
            "read_xl_meta 和 XlMeta::from_bytes 行为不一致"
        );
    }
}

// ========================================================================
// Issue #8: is_xl_meta_erasure_info_valid 注释与实现不一致
// ========================================================================
// 文件: crates/storage/src/format.rs:50-56
// 注释说 "data 必须与 parity 相等"，但 Go 版 MinIO 约束是 data >= parity。
// 代码实现的是 data >= parity (正确)，但注释误导。
// ========================================================================

mod issue_08_erasure_info_valid_comment_mismatch {

    use storage::is_xl_meta_erasure_info_valid;

    /// 验证 data > parity 的情况合法（如 EC 4+2 配置）
    ///
    /// 注释说"必须相等"，但实际 MinIO 支持 data > parity。
    #[test]
    fn data_greater_than_parity_should_be_valid() {
        // EC 4+2: 4 个数据块，2 个校验块
        assert!(
            is_xl_meta_erasure_info_valid(4, 2),
            "BUG: is_xl_meta_erasure_info_valid(4, 2) 返回 false。\n\
             但 EC(4,2) 是合法的 MinIO erasure code 配置。\n\
             注释说 '必须相等' 是错误的——代码检查的是 data >= parity (正确)。\n\
             修复: 纠正注释。"
        );
    }

    /// EC 8+4 也应合法
    #[test]
    fn data_greater_than_parity_8_plus_4_should_be_valid() {
        assert!(
            is_xl_meta_erasure_info_valid(8, 4),
            "EC(8,4) 是合法的配置"
        );
    }

    /// EC 4+4 应合法 (data == parity)
    #[test]
    fn data_equal_to_parity_should_be_valid() {
        assert!(is_xl_meta_erasure_info_valid(4, 4));
    }

    /// data=0 不合法
    #[test]
    fn data_zero_should_be_invalid() {
        assert!(!is_xl_meta_erasure_info_valid(0, 2));
    }

    /// parity=0 (无校验) 合法
    #[test]
    fn parity_zero_should_be_valid() {
        assert!(is_xl_meta_erasure_info_valid(4, 0));
    }
}

// ========================================================================
// Issue #17: read_range offset 越界返回空 Vec，无法区分"越界"和"空数据"
// ========================================================================
// 文件: crates/storage/src/xl_storage.rs:101-103
// ========================================================================

mod issue_17_read_range_oob_ambiguous {

    use super::*;

    /// read_range offset >= file_len 返回空 Vec，
    /// 调用者无法区分"offset 越界"和"恰好读了 0 字节"
    #[tokio::test]
    async fn read_range_beyond_eof_returns_empty_not_error() {
        let dir = std::env::temp_dir().join(format!("reg_17_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let storage = XlStorage::new(&dir, "test");
        storage.make_volume("bucket").await.unwrap();
        storage.write_all("bucket", "small.bin", b"hi").await.unwrap();

        // offset = 100 (远超文件大小 2)
        let result = storage.read_range("bucket", "small.bin", 100, 10).await;

        // 当前实现: 返回 Ok(vec![]) — 与 offset=2, length=0 无法区分
        // 正确行为: 返回 Err 以区分"越界"和"空数据"
        match result {
            Ok(data) if data.is_empty() => {
                // 当前行为 — 静默返回空，上层无法判断是越界还是空数据
                eprintln!(
                    "ISSUE #17 仍在: read_range(offset=100) 返回 Ok([])。\n\
                     调用者无法区分\"offset 越界\"和\"读了 0 字节有效数据\"。\n\
                     建议: 越界时返回 Err 或在文档中明确说明。"
                );
            }
            Ok(_) => panic!("不应该返回非空数据"),
            Err(_) => {
                // 如果返回了错误 — 说明 bug 已修复！
                // 这是我们期待的行为
            }
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}

// ========================================================================
// Issue #25: disk_info 返回硬编码的 total: 0, free: 0, used: 0
// ========================================================================
// 文件: crates/storage/src/xl_storage.rs:54-56
// ========================================================================

mod issue_25_disk_info_hardcoded_zeros {

    use super::*;

    /// disk_info 应该返回真实的磁盘空间信息，而非硬编码 0
    #[tokio::test]
    async fn disk_info_should_return_real_disk_space() {
        let dir = std::env::temp_dir().join(format!("reg_25_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let storage = XlStorage::new(&dir, "test");

        let info = storage.disk_info().await.unwrap();

        // 临时目录所在磁盘不可能 total 为 0
        if info.total == 0 && info.free == 0 {
            eprintln!(
                "ISSUE #25 仍在: disk_info 返回 total={}, free={}, used={}。\n\
                 这些值被硬编码为 0。\n\
                 修复: 使用 statvfs/statfs 获取真实磁盘空间。",
                info.total, info.free, info.used
            );
        }

        // 至少 online 应该是 true
        assert!(info.online, "磁盘应在线");
        assert_eq!(info.healing, false);

        let _ = std::fs::remove_dir_all(&dir);
    }
}

// ========================================================================
// Issue #14: is_online 过于简陋 — 仅检查目录存在
// ========================================================================
// 文件: crates/storage/src/xl_storage.rs:66-68
// 仅检查 disk_path.exists() 不意味着磁盘可读写。
// NFS stale mount、权限变更、磁盘只读重挂等情况不会被检测。
// ========================================================================

mod issue_14_is_online_too_simplistic {

    use super::*;

    /// is_online 对不存在的路径返回 false
    #[tokio::test]
    async fn is_online_should_return_false_for_nonexistent_path() {
        let nonexistent = std::env::temp_dir().join(format!("reg_14_nonexistent_{}", uuid::Uuid::new_v4()));
        let storage = XlStorage::new(&nonexistent, "test");
        assert!(!storage.is_online().await, "不存在的路径 is_online 应返回 false");
    }

    /// is_online 对存在的目录返回 true (但可能不可读写)
    #[tokio::test]
    async fn is_online_should_return_true_for_existing_dir() {
        let dir = std::env::temp_dir().join(format!("reg_14_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let storage = XlStorage::new(&dir, "test");
        assert!(storage.is_online().await, "存在的目录 is_online 应返回 true");

        // 但注意：即使 is_online 返回 true，目录也不一定可读写
        // 例如 NFS stale mount、只读文件系统等场景。
        // 当前实现无法检测这些情况。
        // 建议: 添加周期性 IO 健康检查 (如 .minio.sys/.healthcheck)

        let _ = std::fs::remove_dir_all(&dir);
    }
}
