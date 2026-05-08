//! Checksum 类型定义 — 封装 S3 校验和算法枚举及其 CLI/SDK 映射。
//!
//! 对应 SPEC.md 第 11.12 节。
//!
//! 支持以下算法：
//! - CRC32 / CRC32-FO
//! - CRC32C / CRC32C-FO
//! - CRC64NVME
//! - SHA1、SHA256、SHA512
//! - MD5 / MD5CS
//! - XXH64 (XXHASH64)、XXH3 (XXHASH3)、XXH128 (XXHASH128)

use std::fmt;
use std::str::FromStr;

/// 支持的校验和算法。
///
/// 所有变体均能映射到 `aws_sdk_s3::types::ChecksumAlgorithm`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChecksumType {
    CRC32,
    CRC32C,
    CRC64NVME,
    SHA1,
    SHA256,
    SHA512,
    MD5,
    XXH64,
    XXH3,
    XXH128,
}

impl FromStr for ChecksumType {
    type Err = String;

    /// 不区分大小写解析校验和算法名称。
    ///
    /// 支持的输入格式（大写/小写均可）：
    /// - `CRC32`、`CRC32-FO`  → `ChecksumType::CRC32`
    /// - `CRC32C`、`CRC32C-FO` → `ChecksumType::CRC32C`
    /// - `CRC64NVME`           → `ChecksumType::CRC64NVME`
    /// - `SHA1`                → `ChecksumType::SHA1`
    /// - `SHA256`              → `ChecksumType::SHA256`
    /// - `SHA512`              → `ChecksumType::SHA512`
    /// - `MD5`、`MD5CS`        → `ChecksumType::MD5`
    /// - `XXH64`、`XXHASH64`   → `ChecksumType::XXH64`
    /// - `XXH3`、`XXHASH3`     → `ChecksumType::XXH3`
    /// - `XXH128`、`XXHASH128` → `ChecksumType::XXH128`
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let upper = s.to_uppercase();
        match upper.as_str() {
            "CRC32" | "CRC32-FO" => Ok(ChecksumType::CRC32),
            "CRC32C" | "CRC32C-FO" => Ok(ChecksumType::CRC32C),
            "CRC64NVME" => Ok(ChecksumType::CRC64NVME),
            "SHA1" => Ok(ChecksumType::SHA1),
            "SHA256" => Ok(ChecksumType::SHA256),
            "SHA512" => Ok(ChecksumType::SHA512),
            "MD5" | "MD5CS" => Ok(ChecksumType::MD5),
            "XXH64" | "XXHASH64" => Ok(ChecksumType::XXH64),
            "XXH3" | "XXHASH3" => Ok(ChecksumType::XXH3),
            "XXH128" | "XXHASH128" => Ok(ChecksumType::XXH128),
            _ => Err(format!("unknown checksum algorithm: {}", s)),
        }
    }
}

impl fmt::Display for ChecksumType {
    /// 返回标准化的校验和算法名称（大写）。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChecksumType::CRC32 => write!(f, "CRC32"),
            ChecksumType::CRC32C => write!(f, "CRC32C"),
            ChecksumType::CRC64NVME => write!(f, "CRC64NVME"),
            ChecksumType::SHA1 => write!(f, "SHA1"),
            ChecksumType::SHA256 => write!(f, "SHA256"),
            ChecksumType::SHA512 => write!(f, "SHA512"),
            ChecksumType::MD5 => write!(f, "MD5"),
            ChecksumType::XXH64 => write!(f, "XXH64"),
            ChecksumType::XXH3 => write!(f, "XXH3"),
            ChecksumType::XXH128 => write!(f, "XXH128"),
        }
    }
}

impl ChecksumType {
    /// 映射到 `aws_sdk_s3::types::ChecksumAlgorithm`。
    ///
    /// aws-sdk-s3 1.131 已为所有变体提供对应枚举值。
    pub fn to_s3_checksum_algorithm(&self) -> aws_sdk_s3::types::ChecksumAlgorithm {
        use aws_sdk_s3::types::ChecksumAlgorithm;
        match self {
            ChecksumType::CRC32 => ChecksumAlgorithm::Crc32,
            ChecksumType::CRC32C => ChecksumAlgorithm::Crc32C,
            ChecksumType::CRC64NVME => ChecksumAlgorithm::Crc64Nvme,
            ChecksumType::SHA1 => ChecksumAlgorithm::Sha1,
            ChecksumType::SHA256 => ChecksumAlgorithm::Sha256,
            ChecksumType::SHA512 => ChecksumAlgorithm::Sha512,
            ChecksumType::MD5 => ChecksumAlgorithm::Md5,
            ChecksumType::XXH64 => ChecksumAlgorithm::Xxhash64,
            ChecksumType::XXH3 => ChecksumAlgorithm::Xxhash3,
            ChecksumType::XXH128 => ChecksumAlgorithm::Xxhash128,
        }
    }

    /// 解析 CLI 标志值。
    ///
    /// - `--md5` 标志使用方直接传入 `Some(ChecksumType::MD5)`
    /// - `--checksum=SHA256` → `ChecksumType::from_cli_flag("SHA256")`
    /// - 空字符串或无法解析 → `None`
    pub fn from_cli_flag(s: &str) -> Option<Self> {
        if s.is_empty() {
            return None;
        }
        s.parse().ok()
    }

    /// 将校验和算法应用到 `PutObjectInputBuilder`。
    pub fn apply_checksum(
        &self,
        builder: aws_sdk_s3::operation::put_object::builders::PutObjectInputBuilder,
    ) -> aws_sdk_s3::operation::put_object::builders::PutObjectInputBuilder {
        builder.checksum_algorithm(self.to_s3_checksum_algorithm())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_str_case_insensitive() {
        assert_eq!("crc32".parse::<ChecksumType>().unwrap(), ChecksumType::CRC32);
        assert_eq!("CRC32".parse::<ChecksumType>().unwrap(), ChecksumType::CRC32);
        assert_eq!("Crc32".parse::<ChecksumType>().unwrap(), ChecksumType::CRC32);
    }

    #[test]
    fn test_from_str_crc32_fo() {
        assert_eq!("CRC32-FO".parse::<ChecksumType>().unwrap(), ChecksumType::CRC32);
        assert_eq!("crc32-fo".parse::<ChecksumType>().unwrap(), ChecksumType::CRC32);
    }

    #[test]
    fn test_from_str_crc32c_fo() {
        assert_eq!("CRC32C-FO".parse::<ChecksumType>().unwrap(), ChecksumType::CRC32C);
        assert_eq!("crc32c-fo".parse::<ChecksumType>().unwrap(), ChecksumType::CRC32C);
    }

    #[test]
    fn test_from_str_md5cs() {
        assert_eq!("MD5CS".parse::<ChecksumType>().unwrap(), ChecksumType::MD5);
        assert_eq!("md5cs".parse::<ChecksumType>().unwrap(), ChecksumType::MD5);
    }

    #[test]
    fn test_from_str_xxhash() {
        assert_eq!("XXHASH64".parse::<ChecksumType>().unwrap(), ChecksumType::XXH64);
        assert_eq!("XXHASH3".parse::<ChecksumType>().unwrap(), ChecksumType::XXH3);
        assert_eq!("XXHASH128".parse::<ChecksumType>().unwrap(), ChecksumType::XXH128);
    }

    #[test]
    fn test_from_str_unknown() {
        assert!("UNKNOWN".parse::<ChecksumType>().is_err());
    }

    #[test]
    fn test_display() {
        assert_eq!(ChecksumType::CRC32.to_string(), "CRC32");
        assert_eq!(ChecksumType::CRC32C.to_string(), "CRC32C");
        assert_eq!(ChecksumType::CRC64NVME.to_string(), "CRC64NVME");
        assert_eq!(ChecksumType::SHA1.to_string(), "SHA1");
        assert_eq!(ChecksumType::SHA256.to_string(), "SHA256");
        assert_eq!(ChecksumType::SHA512.to_string(), "SHA512");
        assert_eq!(ChecksumType::MD5.to_string(), "MD5");
        assert_eq!(ChecksumType::XXH64.to_string(), "XXH64");
        assert_eq!(ChecksumType::XXH3.to_string(), "XXH3");
        assert_eq!(ChecksumType::XXH128.to_string(), "XXH128");
    }

    #[test]
    fn test_to_s3_checksum_algorithm() {
        use aws_sdk_s3::types::ChecksumAlgorithm;
        assert_eq!(
            ChecksumType::CRC32.to_s3_checksum_algorithm(),
            ChecksumAlgorithm::Crc32
        );
        assert_eq!(
            ChecksumType::CRC32C.to_s3_checksum_algorithm(),
            ChecksumAlgorithm::Crc32C
        );
        assert_eq!(
            ChecksumType::CRC64NVME.to_s3_checksum_algorithm(),
            ChecksumAlgorithm::Crc64Nvme
        );
        assert_eq!(
            ChecksumType::SHA1.to_s3_checksum_algorithm(),
            ChecksumAlgorithm::Sha1
        );
        assert_eq!(
            ChecksumType::SHA256.to_s3_checksum_algorithm(),
            ChecksumAlgorithm::Sha256
        );
        assert_eq!(
            ChecksumType::SHA512.to_s3_checksum_algorithm(),
            ChecksumAlgorithm::Sha512
        );
        assert_eq!(
            ChecksumType::MD5.to_s3_checksum_algorithm(),
            ChecksumAlgorithm::Md5
        );
        assert_eq!(
            ChecksumType::XXH64.to_s3_checksum_algorithm(),
            ChecksumAlgorithm::Xxhash64
        );
        assert_eq!(
            ChecksumType::XXH3.to_s3_checksum_algorithm(),
            ChecksumAlgorithm::Xxhash3
        );
        assert_eq!(
            ChecksumType::XXH128.to_s3_checksum_algorithm(),
            ChecksumAlgorithm::Xxhash128
        );
    }

    #[test]
    fn test_from_cli_flag() {
        assert_eq!(
            ChecksumType::from_cli_flag("SHA256"),
            Some(ChecksumType::SHA256)
        );
        assert_eq!(ChecksumType::from_cli_flag(""), None);
        assert_eq!(ChecksumType::from_cli_flag("INVALID"), None);
    }

    #[test]
    fn test_all_variants_roundtrip() {
        for variant in &[
            ChecksumType::CRC32,
            ChecksumType::CRC32C,
            ChecksumType::CRC64NVME,
            ChecksumType::SHA1,
            ChecksumType::SHA256,
            ChecksumType::SHA512,
            ChecksumType::MD5,
            ChecksumType::XXH64,
            ChecksumType::XXH3,
            ChecksumType::XXH128,
        ] {
            let s = variant.to_string();
            let parsed: ChecksumType = s.parse().unwrap();
            assert_eq!(&parsed, variant, "roundtrip failed for {}", s);
        }
    }
}
