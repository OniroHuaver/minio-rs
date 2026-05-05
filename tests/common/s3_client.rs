/// Thin S3 HTTP client built on reqwest.
///
/// No authentication is applied (SigV4 is deferred to a later phase).
/// All methods return the raw `reqwest::Response` so tests can inspect
/// status codes, headers, and body as needed.
pub struct S3Client {
    client: reqwest::Client,
    endpoint: String,
}

impl S3Client {
    pub fn new(endpoint: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            endpoint: endpoint.trim_end_matches('/').to_string(),
        }
    }

    // ---- Bucket operations ----

    pub async fn create_bucket(&self, bucket: &str) -> reqwest::Response {
        self.client
            .put(format!("{}/{}", self.endpoint, bucket))
            .send()
            .await
            .expect("create_bucket")
    }

    pub async fn delete_bucket(&self, bucket: &str) -> reqwest::Response {
        self.client
            .delete(format!("{}/{}", self.endpoint, bucket))
            .send()
            .await
            .expect("delete_bucket")
    }

    pub async fn list_buckets(&self) -> reqwest::Response {
        self.client
            .get(&self.endpoint)
            .send()
            .await
            .expect("list_buckets")
    }

    pub async fn head_bucket(&self, bucket: &str) -> reqwest::Response {
        self.client
            .head(format!("{}/{}", self.endpoint, bucket))
            .send()
            .await
            .expect("head_bucket")
    }

    // ---- Object operations ----

    pub async fn put_object(&self, bucket: &str, key: &str, body: &[u8]) -> reqwest::Response {
        self.client
            .put(format!("{}/{}/{}", self.endpoint, bucket, key))
            .body(body.to_vec())
            .send()
            .await
            .expect("put_object")
    }

    /// PUT object with `x-amz-meta-*` headers.
    pub async fn put_object_with_meta(
        &self,
        bucket: &str,
        key: &str,
        body: &[u8],
        meta: &[(&str, &str)],
    ) -> reqwest::Response {
        let mut rb = self
            .client
            .put(format!("{}/{}/{}", self.endpoint, bucket, key))
            .body(body.to_vec());
        for (k, v) in meta {
            rb = rb.header(format!("x-amz-meta-{}", k), *v);
        }
        rb.send().await.expect("put_object_with_meta")
    }

    pub async fn get_object(&self, bucket: &str, key: &str) -> reqwest::Response {
        self.client
            .get(format!("{}/{}/{}", self.endpoint, bucket, key))
            .send()
            .await
            .expect("get_object")
    }

    pub async fn get_object_range(
        &self,
        bucket: &str,
        key: &str,
        range: &str,
    ) -> reqwest::Response {
        self.client
            .get(format!("{}/{}/{}", self.endpoint, bucket, key))
            .header("Range", range)
            .send()
            .await
            .expect("get_object_range")
    }

    pub async fn head_object(&self, bucket: &str, key: &str) -> reqwest::Response {
        self.client
            .head(format!("{}/{}/{}", self.endpoint, bucket, key))
            .send()
            .await
            .expect("head_object")
    }

    pub async fn delete_object(&self, bucket: &str, key: &str) -> reqwest::Response {
        self.client
            .delete(format!("{}/{}/{}", self.endpoint, bucket, key))
            .send()
            .await
            .expect("delete_object")
    }

    pub async fn list_objects_v2(
        &self,
        bucket: &str,
        prefix: &str,
        delimiter: &str,
        max_keys: usize,
    ) -> reqwest::Response {
        let mut url = format!("{}/{}?list-type=2", self.endpoint, bucket);
        if !prefix.is_empty() {
            url.push_str(&format!("&prefix={}", urlencoding(prefix)));
        }
        if !delimiter.is_empty() {
            url.push_str(&format!("&delimiter={}", delimiter));
        }
        if max_keys > 0 {
            url.push_str(&format!("&max-keys={}", max_keys));
        }
        self.client.get(&url).send().await.expect("list_objects_v2")
    }
}

/// Minimal URL-encoding (replaces space with `%20`).
fn urlencoding(s: &str) -> String {
    s.replace(' ', "%20")
}
