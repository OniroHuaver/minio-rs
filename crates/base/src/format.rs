//! xl.meta (XL Storage Format V2) 格式定义
//!
//! 对应 Go: cmd/xl-storage-format-v2.go
//!
//! ## 二进制格式
//!
//! ```text
//! ┌─────────────────────────────────────────────────┐
//! │ Header (8 bytes)                                 │
//! │  "XL2 " (4B magic) + major(2B) + minor(2B)      │
//! ├─────────────────────────────────────────────────┤
//! │ Body: MessagePack Array of Version Entries        │
//! │  ┌─────────────────────────────────────────┐    │
//! │  │ Entry Type 1: Object (VersionData)       │    │
//! │  │ Entry Type 2: Delete (DeleteMarker)      │    │
//! │  │ Entry Type 3: Legacy (V1 占位)           │    │
//! │  └─────────────────────────────────────────┘    │
//! └─────────────────────────────────────────────────┘
//! ```

use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::constants;
use crate::error::{MinioError, MinioResult};

/// xl.meta 版本条目类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum VersionType {
    /// 版本实体 (包含完整对象数据)
    Object = 1,
    /// 删除标记
    Delete = 2,
    /// V1 格式占位
    Legacy = 3,
}

/// xl.meta 文件头
#[derive(Debug, Clone)]
pub struct XlMetaHeader {
    pub magic: [u8; 4], // "XL2 "
    pub major: u16,
    pub minor: u16,
}

impl Default for XlMetaHeader {
    fn default() -> Self {
        Self {
            magic: *constants::XL_HEADER_MAGIC,
            major: constants::XL_VERSION_MAJOR,
            minor: constants::XL_VERSION_MINOR,
        }
    }
}

impl XlMetaHeader {
    pub const SIZE: usize = 8;

    pub fn to_bytes(&self) -> [u8; 8] {
        let mut buf = [0u8; 8];
        buf[0..4].copy_from_slice(&self.magic);
        buf[4..6].copy_from_slice(&self.major.to_be_bytes());
        buf[6..8].copy_from_slice(&self.minor.to_be_bytes());
        buf
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < Self::SIZE {
            return Err("header too short".into());
        }
        let magic = [bytes[0], bytes[1], bytes[2], bytes[3]];
        if &magic != constants::XL_HEADER_MAGIC {
            return Err(format!("bad magic: {:?}", magic));
        }
        Ok(Self {
            magic,
            major: u16::from_be_bytes([bytes[4], bytes[5]]),
            minor: u16::from_be_bytes([bytes[6], bytes[7]]),
        })
    }
}

/// xl.meta 内的版本条目 (MessagePack 序列化)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XlMetaVersionHeader {
    pub version_id: String,
    pub mod_time: i64,           // Unix 时间戳 (纳秒)
    pub signature: Vec<u8>,
    pub r#type: u8,              // VersionType
    pub flags: u8,
    /// 对象总大小 (为 0 时需要遍历 Parts 累加)
    #[serde(default)]
    pub size: i64,
    pub erasure_algorithm: u8,
    pub erasure_m: u16,
    pub erasure_n: u16,
    pub erasure_block_size: i64,
    pub erasure_dist: Vec<u8>,
    pub parts: Vec<ObjectPart>,
    pub meta_sys: Vec<(String, Vec<u8>)>,   // 系统元数据
    pub meta_user: Vec<(String, Vec<u8>)>,  // 用户元数据
}

impl XlMetaVersionHeader {
    /// 创建新的版本 header，使用默认 EC 块大小
    pub fn new(version_id: String) -> Self {
        Self {
            version_id,
            mod_time: 0,
            signature: Vec::new(),
            r#type: VersionType::Object as u8,
            flags: 0,
            size: 0,
            erasure_algorithm: 0,
            erasure_m: 0,
            erasure_n: 0,
            erasure_block_size: constants::DEFAULT_BLOCK_SIZE,
            erasure_dist: Vec::new(),
            parts: Vec::new(),
            meta_sys: Vec::new(),
            meta_user: Vec::new(),
        }
    }

    /// 计算确定性 SHA256 签名 (跨磁盘一致性校验)
    ///
    /// 签名涵盖: VersionID, ModTime, Type, Flags,
    /// ErasureAlgorithm, ErasureM/N, ErasureBlockSize, ErasureDist, Parts
    /// 不涵盖: MetaSys, MetaUser, Data (内联数据)
    pub fn compute_signature(&self) -> MinioResult<Vec<u8>> {
        let sig_data = SignatureData {
            version_id: &self.version_id,
            mod_time: self.mod_time,
            r#type: self.r#type,
            flags: self.flags,
            erasure_algorithm: self.erasure_algorithm,
            erasure_m: self.erasure_m,
            erasure_n: self.erasure_n,
            erasure_block_size: self.erasure_block_size,
            erasure_dist: &self.erasure_dist,
            parts: &self.parts,
        };
        let canonical = rmp_serde::to_vec(&sig_data)
            .map_err(|e| MinioError::MessagePack(e.to_string()))?;
        let mut hasher = Sha256::new();
        hasher.update(&canonical);
        Ok(hasher.finalize().to_vec())
    }
}

/// 签名计算专用结构 (仅包含确定性字段)
#[derive(Serialize)]
struct SignatureData<'a> {
    #[serde(rename = "VersionID")]
    version_id: &'a str,
    #[serde(rename = "ModTime")]
    mod_time: i64,
    #[serde(rename = "Type")]
    r#type: u8,
    #[serde(rename = "Flags")]
    flags: u8,
    #[serde(rename = "ErasureAlgorithm")]
    erasure_algorithm: u8,
    #[serde(rename = "ErasureM")]
    erasure_m: u16,
    #[serde(rename = "ErasureN")]
    erasure_n: u16,
    #[serde(rename = "ErasureBlockSize")]
    erasure_block_size: i64,
    #[serde(rename = "ErasureDist")]
    erasure_dist: &'a [u8],
    #[serde(rename = "Parts")]
    parts: &'a [ObjectPart],
}

/// 对象分片信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectPart {
    pub number: u32,
    pub etag: String,
    pub size: i64,
    pub actual_size: i64,
    pub index: i32,
}

/// 完整 xl.meta 文件内容 (版本条目列表)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XlMeta {
    pub versions: Vec<XlMetaEntry>,
}

/// xl.meta body 最大允许大小 (64 MiB)，防止恶意/损坏输入导致 OOM
pub const MAX_XL_META_SIZE: usize = 64 * 1024 * 1024;

impl XlMeta {
    /// 序列化为完整 xl.meta 二进制格式：8 字节 header + MessagePack body
    pub fn to_bytes(&self) -> MinioResult<Vec<u8>> {
        let header = XlMetaHeader::default();
        let mut buf = Vec::with_capacity(4096);
        buf.extend_from_slice(&header.to_bytes());
        let body =
            rmp_serde::to_vec(self).map_err(|e| MinioError::MessagePack(e.to_string()))?;
        buf.extend_from_slice(&body);
        Ok(buf)
    }

    /// 从 xl.meta 二进制数据反序列化
    pub fn from_bytes(bytes: &[u8]) -> MinioResult<Self> {
        if bytes.len() < XlMetaHeader::SIZE {
            return Err(MinioError::XlMetaFormat(
                "数据太短，不足 header 长度".into(),
            ));
        }
        let header = XlMetaHeader::from_bytes(&bytes[..8])
            .map_err(|e| MinioError::XlMetaFormat(e))?;
        if header.major != constants::XL_VERSION_MAJOR {
            return Err(MinioError::XlMetaFormat(format!(
                "不支持的主版本: {}.{} (当前: {}.{})",
                header.major,
                header.minor,
                constants::XL_VERSION_MAJOR,
                constants::XL_VERSION_MINOR
            )));
        }
        // minor 版本向后兼容：高于已知版本可安全读取，新字段反序列化时被忽略
        let body = &bytes[8..];
        if body.len() > MAX_XL_META_SIZE {
            return Err(MinioError::XlMetaFormat(format!(
                "xl.meta body 过大: {} bytes (上限: {})",
                body.len(),
                MAX_XL_META_SIZE
            )));
        }
        rmp_serde::from_slice(body).map_err(|e| MinioError::MessagePack(e.to_string()))
    }

    /// 从文件路径读取 xl.meta
    ///
    /// **注意**: 使用同步 I/O。在 async 上下文中应改用 `spawn_blocking`，
    /// 或直接调用 `tokio::fs::read` + `XlMeta::from_bytes`。
    /// 此方法主要用于测试和同步场景。
    pub fn read_from_file(path: impl AsRef<Path>) -> MinioResult<Self> {
        let data = std::fs::read(path.as_ref())?;
        Self::from_bytes(&data)
    }

    /// 写入 xl.meta 到文件 (先写临时文件再原子 rename)
    ///
    /// **注意**: 使用同步 I/O + 随机临时文件名避免冲突。
    /// 在 async 上下文中应改用 `spawn_blocking` 包装。
    pub fn write_to_file(&self, path: impl AsRef<Path>) -> MinioResult<()> {
        let data = self.to_bytes()?;
        let path = path.as_ref();
        let dir = path.parent().unwrap_or_else(|| Path::new("."));
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        let tmp_suffix = format!(".xl.meta.{:x}.tmp", (std::process::id() as u64).wrapping_mul(ts));
        let tmp_path = dir.join(tmp_suffix);
        std::fs::write(&tmp_path, &data)?;
        std::fs::rename(&tmp_path, path).map_err(|e| {
            // 清理临时文件，失败不影响主错误
            if let Err(cleanup_err) = std::fs::remove_file(&tmp_path) {
                tracing::warn!(
                    tmp_path = %tmp_path.display(),
                    error = %cleanup_err,
                    "无法清理写入失败的临时文件"
                );
            }
            MinioError::DiskIO(e)
        })?;
        Ok(())
    }
}

/// 版本条目 (枚举变体)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum XlMetaEntry {
    #[serde(rename = "1")]
    Object {
        header: XlMetaVersionHeader,
        /// 小文件 (<128KiB) 的数据内联存储于此
        #[serde(with = "serde_bytes")]
        data: Option<Vec<u8>>,
    },
    #[serde(rename = "2")]
    Delete {
        version_id: String,
        mod_time: i64,
        signature: Vec<u8>,
        flags: u8,
    },
    #[serde(rename = "3")]
    Legacy,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_meta() -> XlMeta {
        let header = XlMetaVersionHeader {
            version_id: "test-version-1".into(),
            mod_time: 1_700_000_000_000_000_000i64,
            signature: Vec::new(),
            r#type: VersionType::Object as u8,
            flags: 0,
            size: 0,
            erasure_algorithm: 0,
            erasure_m: 4,
            erasure_n: 2,
            erasure_block_size: constants::DEFAULT_BLOCK_SIZE,
            erasure_dist: vec![0, 1, 2, 3, 4, 5],
            parts: vec![
                ObjectPart {
                    number: 1,
                    etag: "abc123".into(),
                    size: 1048576,
                    actual_size: 1048576,
                    index: 0,
                },
                ObjectPart {
                    number: 2,
                    etag: "def456".into(),
                    size: 524288,
                    actual_size: 524288,
                    index: 1,
                },
            ],
            meta_sys: vec![("content-type".into(), b"text/plain".to_vec())],
            meta_user: vec![("x-amz-meta-key".into(), b"value".to_vec())],
        };
        XlMeta {
            versions: vec![
                XlMetaEntry::Object {
                    header,
                    data: None,
                },
                XlMetaEntry::Delete {
                    version_id: "deleted-version".into(),
                    mod_time: 1_700_000_000_000_000_000i64,
                    signature: vec![0u8; 32],
                    flags: 0,
                },
            ],
        }
    }

    #[test]
    fn test_xl_meta_roundtrip_bytes() {
        let original = make_test_meta();
        let bytes = original.to_bytes().expect("序列化失败");
        let decoded = XlMeta::from_bytes(&bytes).expect("反序列化失败");

        assert_eq!(decoded.versions.len(), 2);
        match &decoded.versions[0] {
            XlMetaEntry::Object { header, data } => {
                assert_eq!(header.version_id, "test-version-1");
                assert_eq!(header.erasure_m, 4);
                assert_eq!(header.erasure_n, 2);
                assert_eq!(header.parts.len(), 2);
                assert!(data.is_none());
            }
            _ => panic!("expected Object entry"),
        }
    }

    #[test]
    fn test_xl_meta_header_roundtrip() {
        let header = XlMetaHeader::default();
        let bytes = header.to_bytes();
        assert_eq!(&bytes[0..4], b"XL2 ");
        assert_eq!(u16::from_be_bytes([bytes[4], bytes[5]]), constants::XL_VERSION_MAJOR);
        assert_eq!(u16::from_be_bytes([bytes[6], bytes[7]]), constants::XL_VERSION_MINOR);

        let parsed = XlMetaHeader::from_bytes(&bytes).expect("header 解析失败");
        assert_eq!(parsed.major, header.major);
        assert_eq!(parsed.minor, header.minor);
    }

    #[test]
    fn test_xl_meta_from_bytes_bad_magic() {
        let mut bytes = vec![0u8; 8];
        bytes[0..4].copy_from_slice(b"BAD!");
        let result = XlMeta::from_bytes(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_xl_meta_from_bytes_too_short() {
        let result = XlMeta::from_bytes(&[0u8; 3]);
        assert!(result.is_err());
    }

    #[test]
    fn test_xl_meta_file_write_read_roundtrip() {
        let original = make_test_meta();
        let tmp = std::env::temp_dir().join(format!("test_xl_meta_{}.meta", std::process::id()));
        original.write_to_file(&tmp).expect("写入文件失败");
        let loaded = XlMeta::read_from_file(&tmp).expect("读取文件失败");
        let _ = std::fs::remove_file(&tmp);

        assert_eq!(loaded.versions.len(), original.versions.len());
    }

    #[test]
    fn test_compute_signature_deterministic() {
        let mut header = XlMetaVersionHeader::new("sig-test".into());
        header.mod_time = 1000;
        header.erasure_m = 3;
        header.erasure_n = 3;
        header.parts.push(ObjectPart {
            number: 1,
            etag: "test-etag".into(),
            size: 100,
            actual_size: 100,
            index: 0,
        });

        let sig1 = header.compute_signature().expect("签名计算失败");
        let sig2 = header.compute_signature().expect("签名计算失败");
        assert_eq!(sig1, sig2);
        assert_eq!(sig1.len(), 32); // SHA256 = 32 bytes

        // 修改一个字段后签名应变化
        header.mod_time = 2000;
        let sig3 = header.compute_signature().expect("签名计算失败");
        assert_ne!(sig1, sig3);
    }

    #[test]
    fn test_compute_signature_ignores_meta() {
        let mut h1 = XlMetaVersionHeader::new("v1".into());
        h1.meta_sys = vec![("a".into(), b"1".to_vec())];
        h1.meta_user = vec![("x".into(), b"y".to_vec())];

        let mut h2 = XlMetaVersionHeader::new("v1".into());
        h2.meta_sys = vec![("b".into(), b"2".to_vec())];
        h2.meta_user = vec![("z".into(), b"w".to_vec())];

        assert_eq!(
            h1.compute_signature().expect("签名计算失败"),
            h2.compute_signature().expect("签名计算失败")
        );
    }
}
