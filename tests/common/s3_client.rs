#![allow(dead_code)]

/// Thin S3 HTTP client built on reqwest.
///
/// No authentication is applied (SigV4 is deferred to a later phase).
/// All methods return the raw `reqwest::Response` so tests can inspect
/// status codes, headers, and body as needed.
#[derive(Debug)]
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

    pub async fn get_bucket_location(&self, bucket: &str) -> reqwest::Response {
        self.client
            .get(format!("{}/{}?location", self.endpoint, bucket))
            .send()
            .await
            .expect("get_bucket_location")
    }

    // ---- Object operations ----

    pub async fn put_object(&self, bucket: &str, key: &str, body: &[u8]) -> reqwest::Response {
        self.client
            .put(format!(
                "{}/{}/{}",
                self.endpoint,
                bucket,
                urlencode_key(key)
            ))
            .body(body.to_vec())
            .send()
            .await
            .expect("put_object")
    }

    /// PUT object with `Content-MD5` header (base64-encoded MD5 digest).
    pub async fn put_object_with_md5(
        &self,
        bucket: &str,
        key: &str,
        body: &[u8],
        md5_base64: &str,
    ) -> reqwest::Response {
        self.client
            .put(format!(
                "{}/{}/{}",
                self.endpoint,
                bucket,
                urlencode_key(key)
            ))
            .body(body.to_vec())
            .header("Content-MD5", md5_base64)
            .send()
            .await
            .expect("put_object_with_md5")
    }

    /// PUT object with explicit `Content-Type` header.
    pub async fn put_object_with_content_type(
        &self,
        bucket: &str,
        key: &str,
        body: &[u8],
        content_type: &str,
    ) -> reqwest::Response {
        self.client
            .put(format!(
                "{}/{}/{}",
                self.endpoint,
                bucket,
                urlencode_key(key)
            ))
            .body(body.to_vec())
            .header("Content-Type", content_type)
            .send()
            .await
            .expect("put_object_with_content_type")
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
            .put(format!(
                "{}/{}/{}",
                self.endpoint,
                bucket,
                urlencode_key(key)
            ))
            .body(body.to_vec());
        for (k, v) in meta {
            rb = rb.header(format!("x-amz-meta-{}", k), *v);
        }
        rb.send().await.expect("put_object_with_meta")
    }

    pub async fn get_object(&self, bucket: &str, key: &str) -> reqwest::Response {
        self.client
            .get(format!(
                "{}/{}/{}",
                self.endpoint,
                bucket,
                urlencode_key(key)
            ))
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
            .get(format!(
                "{}/{}/{}",
                self.endpoint,
                bucket,
                urlencode_key(key)
            ))
            .header("Range", range)
            .send()
            .await
            .expect("get_object_range")
    }

    pub async fn head_object(&self, bucket: &str, key: &str) -> reqwest::Response {
        self.client
            .head(format!(
                "{}/{}/{}",
                self.endpoint,
                bucket,
                urlencode_key(key)
            ))
            .send()
            .await
            .expect("head_object")
    }

    pub async fn delete_object(&self, bucket: &str, key: &str) -> reqwest::Response {
        self.client
            .delete(format!(
                "{}/{}/{}",
                self.endpoint,
                bucket,
                urlencode_key(key)
            ))
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
            url.push_str(&format!("&prefix={}", urlencode_query(prefix)));
        }
        if !delimiter.is_empty() {
            url.push_str(&format!("&delimiter={}", urlencode_query(delimiter)));
        }
        if max_keys > 0 {
            url.push_str(&format!("&max-keys={}", max_keys));
        }
        self.client.get(&url).send().await.expect("list_objects_v2")
    }

    /// ListObjectsV2 with full parameter set including continuation-token.
    pub async fn list_objects_v2_full(
        &self,
        bucket: &str,
        prefix: &str,
        delimiter: &str,
        max_keys: usize,
        continuation_token: &str,
        start_after: &str,
    ) -> reqwest::Response {
        let mut url = format!("{}/{}?list-type=2", self.endpoint, bucket);
        if !prefix.is_empty() {
            url.push_str(&format!("&prefix={}", urlencode_query(prefix)));
        }
        if !delimiter.is_empty() {
            url.push_str(&format!("&delimiter={}", urlencode_query(delimiter)));
        }
        if max_keys > 0 {
            url.push_str(&format!("&max-keys={}", max_keys));
        }
        if !continuation_token.is_empty() {
            url.push_str(&format!(
                "&continuation-token={}",
                urlencode_query(continuation_token)
            ));
        }
        if !start_after.is_empty() {
            url.push_str(&format!("&start-after={}", urlencode_query(start_after)));
        }
        self.client
            .get(&url)
            .send()
            .await
            .expect("list_objects_v2_full")
    }

    /// Copy an object server-side (PUT /{dst-bucket}/{dst-key} + x-amz-copy-source).
    pub async fn copy_object(
        &self,
        src_bucket: &str,
        src_key: &str,
        dst_bucket: &str,
        dst_key: &str,
    ) -> reqwest::Response {
        let url = format!("{}/{}/{}", self.endpoint, dst_bucket, urlencode_key(dst_key));
        self.client
            .put(&url)
            .header(
                "x-amz-copy-source",
                format!("/{}/{}", src_bucket, src_key),
            )
            .send()
            .await
            .expect("copy_object")
    }

    /// Copy an object with metadata replacement.
    pub async fn copy_object_replace_meta(
        &self,
        src_bucket: &str,
        src_key: &str,
        dst_bucket: &str,
        dst_key: &str,
        meta: &[(&str, &str)],
    ) -> reqwest::Response {
        let url = format!("{}/{}/{}", self.endpoint, dst_bucket, urlencode_key(dst_key));
        let mut req = self
            .client
            .put(&url)
            .header(
                "x-amz-copy-source",
                format!("/{}/{}", src_bucket, src_key),
            )
            .header("x-amz-metadata-directive", "REPLACE");
        for (k, v) in meta {
            let header_name = format!("x-amz-meta-{}", k);
            req = req.header(header_name, *v);
        }
        req.send().await.expect("copy_object_replace_meta")
    }

    /// Delete multiple objects in a single request (POST /{bucket}?delete).
    pub async fn delete_objects(
        &self,
        bucket: &str,
        keys: &[&str],
        quiet: bool,
    ) -> reqwest::Response {
        // Build XML body
        let mut objects_xml = String::new();
        for key in keys {
            objects_xml.push_str(&format!(
                "<Object><Key>{}</Key></Object>",
                key
            ));
        }
        let quiet_xml = if quiet {
            "<Quiet>true</Quiet>".to_string()
        } else {
            String::new()
        };
        let body = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><Delete>{}{}</Delete>"#,
            quiet_xml, objects_xml
        );

        let url = format!("{}/{}?delete", self.endpoint, bucket);
        self.client
            .post(&url)
            .header("content-type", "application/xml")
            .body(body)
            .send()
            .await
            .expect("delete_objects")
    }
}

/// Percent-encode an object key for use in the URL path.
///
/// Only characters that are safe in URI path segments are left un-encoded.
/// This handles spaces, unicode, and S3 special characters like `+`, `=`, `&`, etc.
fn urlencode_key(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                String::from(b as char)
            }
            _ => format!("%{:02X}", b),
        })
        .collect()
}

/// Percent-encode a query parameter value.
fn urlencode_query(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                String::from(b as char)
            }
            _ => format!("%{:02X}", b),
        })
        .collect()
}
