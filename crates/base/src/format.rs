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

use serde::{Deserialize, Serialize};

use crate::constants;

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
        if bytes.len() < 8 {
            return Err("header too short".into());
        }
        let magic: [u8; 4] = bytes[0..4].try_into().unwrap();
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
    pub erasure_algorithm: u8,
    pub erasure_m: u16,
    pub erasure_n: u16,
    pub erasure_block_size: i64,
    pub erasure_dist: Vec<u8>,
    pub parts: Vec<ObjectPart>,
    pub meta_sys: Vec<(String, Vec<u8>)>,   // 系统元数据
    pub meta_user: Vec<(String, Vec<u8>)>,  // 用户元数据
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
