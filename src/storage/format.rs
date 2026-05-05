//! xl.meta format read/write utilities
//!
//! Provides basic functionality for reading, writing, and validating xl.meta binary files.

use crate::base::error::{MinioError, MinioResult};
use crate::base::format::{XlMeta, XlMetaHeader};

/// Read an xl.meta file (8-byte Header + MessagePack Body)
///
/// Delegates to `XlMeta::from_bytes` to ensure consistent parsing logic
/// across all call paths (including header version validation and body size limits).
pub fn read_xl_meta(buf: &[u8]) -> MinioResult<XlMeta> {
    XlMeta::from_bytes(buf)
}

/// Write xl.meta binary (Header + MessagePack Body)
pub fn write_xl_meta(meta: &XlMeta) -> MinioResult<Vec<u8>> {
    write_xl_meta_inner(meta, false)
}

/// Write xl.meta binary without inline Data field (for signature calculation, etc.)
pub fn write_xl_meta_no_data(meta: &XlMeta) -> MinioResult<Vec<u8>> {
    write_xl_meta_inner(meta, true)
}

/// Internal unified xl.meta serialization implementation
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

/// Validate xl.meta version and format fields
///
/// format must be "xl", version must be "1.0.0" or "1.0.1".
pub fn is_xl_meta_format_valid(version: &str, format: &str) -> bool {
    format == "xl" && (version == "1.0.0" || version == "1.0.1")
}

/// Validate erasure coding parameters (data/parity block counts)
///
/// data must be > 0, parity >= 0, and data >= parity
/// (RS erasure coding requires at least as many data blocks as parity blocks).
pub fn is_xl_meta_erasure_info_valid(data: i64, parity: i64) -> bool {
    data > 0 && parity >= 0 && data >= parity
}

/// Calculate the actual size of a part given its index
///
/// The last part may be smaller than part_size.
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

/// Deterministic hash (used for xl.meta signature calculation)
///
/// Performs SHA256 hashing on a key-value map after stable sorting.
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

/// xl.meta V1 object (for JSON serialization compatible with legacy xl.json format)
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

/// V1 file stat information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StatInfo {
    pub size: i64,
    pub mod_time: i64,
}

/// V1 erasure code information
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

/// V1 checksum information
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChecksumInfo {
    pub part_number: u32,
    pub algorithm: u8,
    pub hash: Vec<u8>,
}

/// V1 object part information
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

/// V2 version header (serializable, used for signature calculation)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct XlMetaV2VersionHeader {
    pub version_id: Vec<u8>,
    pub mod_time: i64,
    pub signature: Vec<u8>,
    pub r#type: u8,
    pub flags: u8,
}

/// V2 object entry
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct XlMetaV2Object {
    pub version_id: Vec<u8>,
    pub data_dir: Vec<u8>,
    pub mod_time: i64,
    pub signature: Vec<u8>,
    pub r#type: u8,
    pub flags: u8,
}

/// V2 delete marker
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct XlMetaV2DeleteMarker {
    pub version_id: Vec<u8>,
    pub mod_time: i64,
    pub signature: Vec<u8>,
    pub r#type: u8,
    pub flags: u8,
}

/// V2 version entry
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct XlMetaV2Version {
    pub header: XlMetaV2VersionHeader,
    pub object: Option<XlMetaV2Object>,
    pub delete_marker: Option<XlMetaV2DeleteMarker>,
}

/// V2 DataDir decoder (for parsing inline DataDir index)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct XlMetaDataDirDecoder {
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>,
}

// ========== Default implementations ==========

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
