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

/// Parsed HTTP Range header value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RangeSpec {
    /// `bytes=start-end` — both ends are inclusive byte positions
    Bytes { start: i64, end: i64 },
    /// `bytes=start-` — from start to end of object
    From { start: i64 },
    /// `bytes=-suffix` — last N bytes of the object
    Suffix { length: i64 },
}

/// Parse an HTTP `Range` header.
///
/// Supported forms:
/// - `bytes=start-end` → `RangeSpec::Bytes`
/// - `bytes=start-`    → `RangeSpec::From`
/// - `bytes=-suffix`   → `RangeSpec::Suffix`
///
/// Returns `None` if the header cannot be parsed.
pub fn parse_range(header: &str) -> Option<RangeSpec> {
    let header = header.trim();
    let range = header.strip_prefix("bytes=")?;
    if let Some(suffix) = range.strip_prefix('-') {
        // bytes=-suffix
        let length = suffix.parse::<i64>().ok()?;
        return Some(RangeSpec::Suffix { length });
    }
    if let Some((start_str, end_str)) = range.split_once('-') {
        let start = start_str.parse::<i64>().ok()?;
        if end_str.is_empty() {
            // bytes=start-
            return Some(RangeSpec::From { start });
        }
        let end = end_str.parse::<i64>().ok()?;
        return Some(RangeSpec::Bytes { start, end });
    }
    None
}

/// Decode percent-encoded strings (e.g., `%20` → ` `, `%2F` → `/`).
///
/// Invalid sequences (truncated `%XX`, non-hex digits) are left as-is.
pub fn percent_decode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.as_bytes().iter().copied();
    while let Some(b) = chars.next() {
        if b == b'%' {
            let hi = chars.next();
            let lo = chars.next();
            if let (Some(hi), Some(lo)) = (hi, lo) {
                if let (Some(h), Some(l)) = (hex_val(hi), hex_val(lo)) {
                    out.push((h << 4 | l) as char);
                    continue;
                }
            }
            out.push('%');
            if let Some(hi) = hi { out.push(hi as char); }
            if let Some(lo) = lo { out.push(lo as char); }
        } else {
            out.push(b as char);
        }
    }
    out
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderName;

    #[test]
    fn test_parse_range_valid() {
        assert_eq!(parse_range("bytes=0-1023"), Some(RangeSpec::Bytes { start: 0, end: 1023 }));
        assert_eq!(parse_range("bytes=100-200"), Some(RangeSpec::Bytes { start: 100, end: 200 }));
        assert_eq!(parse_range("bytes=500-"), Some(RangeSpec::From { start: 500 }));
        assert_eq!(parse_range("bytes=-100"), Some(RangeSpec::Suffix { length: 100 }));
    }

    #[test]
    fn test_parse_range_invalid() {
        assert_eq!(parse_range(""), None);
        assert_eq!(parse_range("bytes="), None);
        assert_eq!(parse_range("0-1023"), None);
        assert_eq!(parse_range("bytes=abc-def"), None);
    }

    #[test]
    fn test_percent_decode() {
        assert_eq!(percent_decode("hello%20world"), "hello world");
        assert_eq!(percent_decode("key%2Fsub%2Ffile"), "key/sub/file");
        assert_eq!(percent_decode("noencoding"), "noencoding");
        assert_eq!(percent_decode("bad%XXseq"), "bad%XXseq");
        assert_eq!(percent_decode("%"), "%");
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
