//! xl.meta (XL Storage Format V2) format definition
//!
//! ## Binary format
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
//! │  │ Entry Type 3: Legacy (V1 placeholder)    │    │
//! │  └─────────────────────────────────────────┘    │
//! └─────────────────────────────────────────────────┘
//! ```

use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::base::constants;
use crate::base::error::{MinioError, MinioResult};

/// Version entry type for xl.meta
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum VersionType {
    /// Version with full object data
    Object = 1,
    /// Delete marker
    Delete = 2,
    /// V1 format placeholder
    Legacy = 3,
}

/// xl.meta file header
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

/// Version entry inside xl.meta (MessagePack serialized)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XlMetaVersionHeader {
    pub version_id: String,
    pub mod_time: i64,           // Unix timestamp (nanoseconds)
    pub signature: Vec<u8>,
    pub r#type: u8,              // VersionType
    pub flags: u8,
    /// Total object size (0 means sum over Parts)
    #[serde(default)]
    pub size: i64,
    pub erasure_algorithm: u8,
    pub erasure_m: u16,
    pub erasure_n: u16,
    pub erasure_block_size: i64,
    pub erasure_dist: Vec<u8>,
    pub parts: Vec<ObjectPart>,
    pub meta_sys: Vec<(String, Vec<u8>)>,   // System metadata
    pub meta_user: Vec<(String, Vec<u8>)>,  // User metadata
}

impl XlMetaVersionHeader {
    /// Create a new version header with default EC block size
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

    /// Compute a deterministic SHA256 signature (cross-disk consistency check)
    ///
    /// Covers: VersionID, ModTime, Type, Flags,
    /// ErasureAlgorithm, ErasureM/N, ErasureBlockSize, ErasureDist, Parts
    /// Excludes: MetaSys, MetaUser, Data (inline data)
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

/// Helper struct for signature computation (deterministic fields only)
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

/// Object part information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectPart {
    pub number: u32,
    pub etag: String,
    pub size: i64,
    pub actual_size: i64,
    pub index: i32,
}

/// Complete xl.meta file content (list of version entries)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XlMeta {
    pub versions: Vec<XlMetaEntry>,
}

/// Maximum allowed xl.meta body size (64 MiB), prevents OOM from malicious/corrupt input
pub const MAX_XL_META_SIZE: usize = 64 * 1024 * 1024;

impl XlMeta {
    /// Serialize to full xl.meta binary format: 8-byte header + MessagePack body
    pub fn to_bytes(&self) -> MinioResult<Vec<u8>> {
        let header = XlMetaHeader::default();
        let mut buf = Vec::with_capacity(4096);
        buf.extend_from_slice(&header.to_bytes());
        let body =
            rmp_serde::to_vec(self).map_err(|e| MinioError::MessagePack(e.to_string()))?;
        buf.extend_from_slice(&body);
        Ok(buf)
    }

    /// Deserialize from xl.meta binary data
    pub fn from_bytes(bytes: &[u8]) -> MinioResult<Self> {
        if bytes.len() < XlMetaHeader::SIZE {
            return Err(MinioError::XlMetaFormat(
                "data too short, insufficient for header length".into(),
            ));
        }
        let header = XlMetaHeader::from_bytes(&bytes[..8])
            .map_err(|e| MinioError::XlMetaFormat(e))?;
        if header.major != constants::XL_VERSION_MAJOR {
            return Err(MinioError::XlMetaFormat(format!(
                "unsupported major version: {}.{} (current: {}.{})",
                header.major,
                header.minor,
                constants::XL_VERSION_MAJOR,
                constants::XL_VERSION_MINOR
            )));
        }
        // Minor version is backward compatible: newer versions can be safely read, new fields are ignored during deserialization
        let body = &bytes[8..];
        if body.len() > MAX_XL_META_SIZE {
            return Err(MinioError::XlMetaFormat(format!(
                "xl.meta body too large: {} bytes (max: {})",
                body.len(),
                MAX_XL_META_SIZE
            )));
        }
        rmp_serde::from_slice(body).map_err(|e| MinioError::MessagePack(e.to_string()))
    }

    /// Read xl.meta from a file path
    ///
    /// **Note**: Uses synchronous I/O. In async contexts, use `spawn_blocking`
    /// or call `tokio::fs::read` + `XlMeta::from_bytes` directly.
    /// This method is primarily for testing and synchronous scenarios.
    pub fn read_from_file(path: impl AsRef<Path>) -> MinioResult<Self> {
        let data = std::fs::read(path.as_ref())?;
        Self::from_bytes(&data)
    }

    /// Write xl.meta to file (write to temp file, then atomic rename)
    ///
    /// **Note**: Uses synchronous I/O with random temp file names to avoid conflicts.
    /// In async contexts, wrap with `spawn_blocking`.
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
            // Clean up temp file; failure does not affect the main error
            if let Err(cleanup_err) = std::fs::remove_file(&tmp_path) {
                tracing::warn!(
                    tmp_path = %tmp_path.display(),
                    error = %cleanup_err,
                    "failed to clean up temporary file after write failure"
                );
            }
            MinioError::DiskIO(e)
        })?;
        Ok(())
    }
}

/// Version entry (enum variant)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum XlMetaEntry {
    #[serde(rename = "1")]
    Object {
        header: XlMetaVersionHeader,
        /// Inline data for small files (<128KiB)
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
        let bytes = original.to_bytes().expect("serialization failed");
        let decoded = XlMeta::from_bytes(&bytes).expect("deserialization failed");

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

        let parsed = XlMetaHeader::from_bytes(&bytes).expect("header parse failed");
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
        original.write_to_file(&tmp).expect("write to file failed");
        let loaded = XlMeta::read_from_file(&tmp).expect("read from file failed");
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

        let sig1 = header.compute_signature().expect("signature computation failed");
        let sig2 = header.compute_signature().expect("signature computation failed");
        assert_eq!(sig1, sig2);
        assert_eq!(sig1.len(), 32); // SHA256 = 32 bytes

        // Signature should change when a field is modified
        header.mod_time = 2000;
        let sig3 = header.compute_signature().expect("signature computation failed");
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
            h1.compute_signature().expect("signature computation failed"),
            h2.compute_signature().expect("signature computation failed")
        );
    }
}
