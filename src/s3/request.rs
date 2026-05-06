//! Request parsing utilities — metadata extraction, Range header parsing, XML deserialization

use axum::http::HeaderMap;
use serde::Deserialize;

/// XML body for DeleteObjects (POST /:bucket?delete).
#[derive(Debug, Deserialize)]
#[serde(rename = "Delete")]
pub struct DeleteObjectsBody {
    #[serde(rename = "Object", default)]
    pub objects: Vec<DeleteObjectEntry>,
    #[serde(rename = "Quiet", default)]
    pub quiet: Option<bool>,
}

/// A single object key in a DeleteObjects request.
#[derive(Debug, Deserialize)]
pub struct DeleteObjectEntry {
    #[serde(rename = "Key")]
    pub key: String,
}

/// XML body for CompleteMultipartUpload.
#[derive(Debug, Deserialize)]
#[serde(rename = "CompleteMultipartUpload")]
pub struct CompleteMultipartUploadBody {
    #[serde(rename = "Part", default)]
    pub parts: Vec<CompletedPartBody>,
}

#[derive(Debug, Deserialize)]
pub struct CompletedPartBody {
    #[serde(rename = "PartNumber")]
    pub part_number: u32,
    #[serde(rename = "ETag")]
    pub etag: String,
}

/// Extract system and user metadata from HTTP headers.
///
/// Collects `Content-Type` as system metadata and `x-amz-meta-*` entries
/// as user metadata (with the `x-amz-meta-` prefix stripped).
pub fn extract_metadata(headers: &HeaderMap) -> Vec<(String, String)> {
    let mut metadata = Vec::new();

    // Content-Type → system metadata
    if let Some(content_type) = headers.get("content-type").and_then(|v| v.to_str().ok()) {
        metadata.push(("Content-Type".to_string(), content_type.to_string()));
    }

    // x-amz-meta-* → user metadata (strip prefix)
    for (name, value) in headers.iter() {
        let name_str = name.as_str().to_lowercase();
        if let Some(suffix) = name_str.strip_prefix("x-amz-meta-") {
            if let Ok(v) = value.to_str() {
                metadata.push((suffix.to_string(), v.to_string()));
            }
        }
    }

    metadata
}

/// Parse an HTTP `Range` header in the form `bytes=start-end`.
///
/// Returns `Some((start, end))` on success or `None` if the header
/// cannot be parsed.  Both offsets are inclusive byte positions.
pub fn parse_range(header: &str) -> Option<(i64, i64)> {
    let header = header.trim();
    let range = header.strip_prefix("bytes=")?;
    let (start_str, end_str) = range.split_once('-')?;
    let start = start_str.parse::<i64>().ok()?;
    let end = end_str.parse::<i64>().ok()?;
    Some((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderName;

    #[test]
    fn test_parse_range_valid() {
        assert_eq!(parse_range("bytes=0-1023"), Some((0, 1023)));
        assert_eq!(parse_range("bytes=100-200"), Some((100, 200)));
    }

    #[test]
    fn test_parse_range_invalid() {
        assert_eq!(parse_range(""), None);
        assert_eq!(parse_range("bytes="), None);
        assert_eq!(parse_range("0-1023"), None);
    }

    #[test]
    fn test_extract_metadata_content_type() {
        let mut headers = HeaderMap::new();
        headers.insert(HeaderName::from_static("content-type"), "text/plain".parse().unwrap());
        headers.insert(HeaderName::from_static("x-amz-meta-color"), "red".parse().unwrap());

        let meta = extract_metadata(&headers);
        assert!(meta.contains(&("Content-Type".to_string(), "text/plain".to_string())));
        assert!(meta.contains(&("color".to_string(), "red".to_string())));
    }
}
