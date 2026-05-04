//! xl.meta 格式读写工具
//!
//! 提供 xl.meta 二进制文件的读取、写入、格式校验等基础功能。
//!
//! 对应 Go: cmd/xl-storage-format-utils.go

use base::error::{MinioError, MinioResult};
use base::format::{XlMeta, XlMetaHeader};

/// 读取 xl.meta 文件 (8 字节 Header + MessagePack Body)
///
/// 委托给 `XlMeta::from_bytes`，确保与所有调用路径走同一套解析逻辑
/// （含 header 版本校验和 body 大小限制）。
pub fn read_xl_meta(buf: &[u8]) -> MinioResult<XlMeta> {
    XlMeta::from_bytes(buf)
}

/// 写入 xl.meta 二进制 (Header + MessagePack Body)
pub fn write_xl_meta(meta: &XlMeta) -> MinioResult<Vec<u8>> {
    write_xl_meta_inner(meta, false)
}

/// 写入 xl.meta 二进制 (不含 Data 内联字段，用于签名计算等场景)
pub fn write_xl_meta_no_data(meta: &XlMeta) -> MinioResult<Vec<u8>> {
    write_xl_meta_inner(meta, true)
}

/// 统一的 xl.meta 序列化内部实现
fn write_xl_meta_inner(meta: &XlMeta, named: bool) -> MinioResult<Vec<u8>> {
    let mut buf = Vec::with_capacity(4096);
    buf.extend_from_slice(&XlMetaHeader::default().to_bytes());
    let body = if named {
        rmp_serde::to_vec_named(meta).map_err(|e| MinioError::MessagePack(e.to_string()))?
    } else {
        rmp_serde::to_vec(meta).map_err(|e| MinioError::MessagePack(e.to_string()))?
    };
    buf.extend_from_slice(&body);
    Ok(buf)
}

/// 校验 xl.meta 的 version + format 字段是否合法
///
/// format 必须为 "xl"，version 必须为 "1.0.0" 或 "1.0.1"。
pub fn is_xl_meta_format_valid(version: &str, format: &str) -> bool {
    format == "xl" && (version == "1.0.0" || version == "1.0.1")
}

/// 校验擦除码参数 (data/parity block 数量)
///
/// data 必须 > 0，parity ≥ 0，且 data ≥ parity（RS 容错理论要求数据块不少于校验块）。
pub fn is_xl_meta_erasure_info_valid(data: i64, parity: i64) -> bool {
    data > 0 && parity >= 0 && data >= parity
}

/// 根据 part 索引计算该 part 的实际大小
///
/// 最后一个 part 可能不满 part_size。
pub fn calculate_part_size_from_idx(
    total_size: i64,
    part_size: i64,
    part_index: i32,
) -> MinioResult<i64> {
    if total_size < 0 {
        return Err(MinioError::Internal(
            "invalid argument: total_size < 0".into(),
        ));
    }
    if part_size <= 0 {
        return Err(MinioError::Internal("part size is zero".into()));
    }
    if part_index < 1 {
        return Err(MinioError::Internal(
            "part size index is invalid".into(),
        ));
    }

    let total_parts = (total_size + part_size - 1) / part_size;
    let idx = part_index as i64;
    if idx > total_parts {
        return Ok(0);
    }

    if idx == total_parts {
        let remainder = total_size % part_size;
        if remainder == 0 {
            Ok(part_size)
        } else {
            Ok(remainder)
        }
    } else {
        Ok(part_size)
    }
}

/// 确定性哈希 (用于 xl.meta 签名计算)
///
/// 对 key-value map 做稳定排序后 SHA256 哈希。
/// 对应 Go: cmd/xl-storage-format-utils.go hashDeterministicString
pub fn hash_deterministic_string(meta: &std::collections::HashMap<String, String>) -> String {
    use sha2::{Digest, Sha256};
    let mut keys: Vec<&String> = meta.keys().collect();
    keys.sort();

    let mut hasher = Sha256::new();
    for key in keys {
        hasher.update(key.as_bytes());
        hasher.update(meta[key].as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

/// xl.meta V1 对象（用于兼容旧版 xl.json 格式的 JSON 序列化）
///
/// 对应 Go: cmd/xl-storage-format-v1.go xlMetaV1Object
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct XlMetaV1Object {
    pub version: String,
    pub format: String,
    pub stat: Option<StatInfo>,
    pub erasure: Option<ErasureInfo>,
    #[serde(default)]
    pub parts: Vec<ObjectPartInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta_sys: Option<std::collections::HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta_user: Option<std::collections::HashMap<String, String>>,
}

/// V1 文件状态信息
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StatInfo {
    pub size: i64,
    pub mod_time: i64,
}

/// V1 擦除码信息
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ErasureInfo {
    pub algorithm: u8,
    pub data: i64,
    pub parity: i64,
    pub block_size: i64,
    pub index: i32,
    pub distribution: Vec<u8>,
    pub checksums: Vec<ChecksumInfo>,
}

/// V1 校验和信息
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChecksumInfo {
    pub part_number: u32,
    pub algorithm: u8,
    pub hash: Vec<u8>,
}

/// V1 对象分片信息
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ObjectPartInfo {
    pub number: u32,
    #[serde(default)]
    pub name: String,
    pub etag: String,
    pub size: i64,
    pub actual_size: i64,
    pub index: i32,
}

/// V2 版本头 (可序列化版本，用于签名计算)
///
/// 对应 Go: cmd/xl-storage-format-v2.go xlMetaV2VersionHeader
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct XlMetaV2VersionHeader {
    pub version_id: Vec<u8>,
    pub mod_time: i64,
    pub signature: Vec<u8>,
    pub r#type: u8,
    pub flags: u8,
}

/// V2 对象条目
///
/// 对应 Go: cmd/xl-storage-format-v2.go xlMetaV2Object
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct XlMetaV2Object {
    pub version_id: Vec<u8>,
    pub data_dir: Vec<u8>,
    pub mod_time: i64,
    pub signature: Vec<u8>,
    pub r#type: u8,
    pub flags: u8,
}

/// V2 删除标记
///
/// 对应 Go: cmd/xl-storage-format-v2.go xlMetaV2DeleteMarker
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct XlMetaV2DeleteMarker {
    pub version_id: Vec<u8>,
    pub mod_time: i64,
    pub signature: Vec<u8>,
    pub r#type: u8,
    pub flags: u8,
}

/// V2 版本条目
///
/// 对应 Go: cmd/xl-storage-format-v2.go xlMetaV2Version
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct XlMetaV2Version {
    pub header: XlMetaV2VersionHeader,
    pub object: Option<XlMetaV2Object>,
    pub delete_marker: Option<XlMetaV2DeleteMarker>,
}

/// V2 DataDir 解码器 (用于解析内联 DataDir 索引)
///
/// 对应 Go: cmd/xl-storage-format-v2.go xlMetaDataDirDecoder
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct XlMetaDataDirDecoder {
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>,
}

// ========== 默认值构造 ==========

impl Default for XlMetaV1Object {
    fn default() -> Self {
        Self {
            version: "1.0.1".into(),
            format: "xl".into(),
            stat: None,
            erasure: None,
            parts: vec![],
            meta_sys: None,
            meta_user: None,
        }
    }
}

impl Default for StatInfo {
    fn default() -> Self {
        Self {
            size: 0,
            mod_time: 0,
        }
    }
}

impl Default for ErasureInfo {
    fn default() -> Self {
        Self {
            algorithm: 0,
            data: 0,
            parity: 0,
            block_size: 4 * 1024 * 1024,
            index: 0,
            distribution: vec![],
            checksums: vec![],
        }
    }
}

impl Default for ChecksumInfo {
    fn default() -> Self {
        Self {
            part_number: 0,
            algorithm: 0,
            hash: vec![],
        }
    }
}

impl Default for ObjectPartInfo {
    fn default() -> Self {
        Self {
            number: 0,
            name: String::new(),
            etag: String::new(),
            size: 0,
            actual_size: 0,
            index: 0,
        }
    }
}

impl Default for XlMetaV2VersionHeader {
    fn default() -> Self {
        Self {
            version_id: vec![],
            mod_time: 0,
            signature: vec![],
            r#type: 1,
            flags: 0,
        }
    }
}

impl Default for XlMetaV2Object {
    fn default() -> Self {
        Self {
            version_id: vec![],
            data_dir: vec![],
            mod_time: 0,
            signature: vec![],
            r#type: 1,
            flags: 0,
        }
    }
}

impl Default for XlMetaV2DeleteMarker {
    fn default() -> Self {
        Self {
            version_id: vec![],
            mod_time: 0,
            signature: vec![],
            r#type: 2,
            flags: 0,
        }
    }
}

impl Default for XlMetaV2Version {
    fn default() -> Self {
        Self {
            header: XlMetaV2VersionHeader::default(),
            object: None,
            delete_marker: None,
        }
    }
}

impl Default for XlMetaDataDirDecoder {
    fn default() -> Self {
        Self { data: vec![] }
    }
}
