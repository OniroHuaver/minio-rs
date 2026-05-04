//! 基础类型定义

use uuid::Uuid;

/// 版本 ID
pub type VersionId = String;

/// 对象键 (路径)
pub type ObjectKey = String;

/// Bucket 名称
pub type BucketName = String;

/// 磁盘路径
pub type DiskPath = String;

/// 部署 ID (UUID v4)
pub type DeploymentId = Uuid;

/// ETag (对象内容哈希)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ETag(String);

impl ETag {
    pub fn new(value: String) -> Self {
        // 去掉引号 (S3 API 常见格式)
        let cleaned = value.trim_matches('"').to_string();
        Self(cleaned)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ETag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "\"{}\"", self.0)
    }
}

impl From<String> for ETag {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for ETag {
    fn from(value: &str) -> Self {
        Self::new(value.to_string())
    }
}
