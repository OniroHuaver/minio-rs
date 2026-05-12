//! Basic type definitions

use uuid::Uuid;

/// Version ID
pub type VersionId = String;

/// Object key (path)
pub type ObjectKey = String;

/// Bucket name
pub type BucketName = String;

/// Disk path
pub type DiskPath = String;

/// Deployment ID (UUID v4)
pub type DeploymentId = Uuid;

/// ETag (object content hash)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ETag(String);

impl ETag {
    pub fn new(value: String) -> Self {
        // Strip surrounding quotes (common S3 API format)
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
