#![allow(dead_code)]

/// Simplified mc (MinIO Client) CLI simulation.
///
/// Wraps the raw S3 HTTP client with mc-style command names:
/// `mb`, `cp`, `cat`, `rm`.
///
/// In a real mc the alias step stores credentials and endpoint; here we
/// skip authentication (Phase 1 has no SigV4 enforcement) and just keep
/// the endpoint.
pub struct McClient {
    inner: super::s3_client::S3Client,
}

impl McClient {
    pub fn new(endpoint: &str) -> Self {
        Self {
            inner: super::s3_client::S3Client::new(endpoint),
        }
    }

    /// `mc alias set local <endpoint> minioadmin minioadmin`
    ///
    /// Phase 1 is auth-less, so this is a no-op that just returns the
    /// client for fluent chaining.
    pub fn alias_set(self, _alias: &str, _access_key: &str, _secret_key: &str) -> Self {
        self
    }

    /// `mc mb local/<bucket>` → PUT /{bucket}
    pub async fn mb(&self, bucket: &str) -> reqwest::Response {
        self.inner.create_bucket(bucket).await
    }

    /// `mc cp <local_file> local/<bucket>/<key>` → PUT /{bucket}/{key}
    pub async fn cp(&self, bucket: &str, key: &str, data: &[u8]) -> reqwest::Response {
        self.inner.put_object(bucket, key, data).await
    }

    /// `mc cat local/<bucket>/<key>` → GET /{bucket}/{key}
    pub async fn cat(&self, bucket: &str, key: &str) -> reqwest::Response {
        self.inner.get_object(bucket, key).await
    }

    /// `mc rm local/<bucket>/<key>` → DELETE /{bucket}/{key}
    pub async fn rm(&self, bucket: &str, key: &str) -> reqwest::Response {
        self.inner.delete_object(bucket, key).await
    }

    /// `mc ls local/<bucket>` → GET /{bucket}?list-type=2
    pub async fn ls(&self, bucket: &str) -> reqwest::Response {
        self.inner.list_objects_v2(bucket, "", "", 0).await
    }
}
