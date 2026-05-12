//! XML response structs and S3 response helpers

use axum::http::{
    header::{self, HeaderValue},
    HeaderMap, StatusCode,
};
use serde::Serialize;

// ---------------------------------------------------------------------------
// XML response structures (quick-xml + serde)
// ---------------------------------------------------------------------------

// ---- ListAllMyBucketsResult -----------------------------------------------

#[derive(Serialize)]
#[serde(rename = "ListAllMyBucketsResult")]
pub struct ListAllMyBucketsResult {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "Owner")]
    pub owner: Owner,
    #[serde(rename = "Buckets")]
    pub buckets: BucketsList,
}

#[derive(Serialize)]
pub struct Owner {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "DisplayName")]
    pub display_name: String,
}

#[derive(Serialize)]
pub struct BucketsList {
    #[serde(rename = "Bucket")]
    pub bucket: Vec<BucketEntry>,
}

#[derive(Serialize)]
pub struct BucketEntry {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "CreationDate")]
    pub creation_date: String,
}

// ---- ListBucketResult (V2) ------------------------------------------------

#[derive(Serialize)]
#[serde(rename = "ListBucketResult")]
pub struct ListBucketResult {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Prefix")]
    pub prefix: String,
    #[serde(rename = "KeyCount")]
    pub key_count: usize,
    #[serde(rename = "MaxKeys")]
    pub max_keys: usize,
    #[serde(rename = "IsTruncated")]
    pub is_truncated: bool,
    #[serde(rename = "NextContinuationToken", skip_serializing_if = "Option::is_none")]
    pub next_continuation_token: Option<String>,
    #[serde(rename = "ContinuationToken", skip_serializing_if = "Option::is_none")]
    pub continuation_token: Option<String>,
    #[serde(rename = "Contents")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub contents: Vec<ContentEntry>,
    #[serde(rename = "CommonPrefixes")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub common_prefixes: Vec<CommonPrefixesEntry>,
}

#[derive(Serialize)]
pub struct ContentEntry {
    #[serde(rename = "Key")]
    pub key: String,
    #[serde(rename = "LastModified")]
    pub last_modified: String,
    #[serde(rename = "ETag")]
    pub etag: String,
    #[serde(rename = "Size")]
    pub size: i64,
    #[serde(rename = "StorageClass")]
    pub storage_class: String,
}

#[derive(Serialize)]
pub struct CommonPrefixesEntry {
    #[serde(rename = "Prefix")]
    pub prefix: String,
}

// ---- Versioning Configuration ---------------------------------------------

#[derive(Serialize)]
#[serde(rename = "VersioningConfiguration")]
pub struct VersioningConfigurationXml {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "Status")]
    pub status: String,
    #[serde(rename = "MfaDelete")]
    pub mfa_delete: String,
}

// ---- Multipart Upload XML -------------------------------------------------

#[derive(Serialize)]
#[serde(rename = "InitiateMultipartUploadResult")]
pub struct InitiateMultipartUploadResultXml {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "Bucket")]
    pub bucket: String,
    #[serde(rename = "Key")]
    pub key: String,
    #[serde(rename = "UploadId")]
    pub upload_id: String,
}

#[derive(Serialize)]
#[serde(rename = "CompleteMultipartUploadResult")]
pub struct CompleteMultipartUploadResultXml {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "Location")]
    pub location: String,
    #[serde(rename = "Bucket")]
    pub bucket: String,
    #[serde(rename = "Key")]
    pub key: String,
    #[serde(rename = "ETag")]
    pub etag: String,
}

// ---- CopyObjectResult ------------------------------------------------------

#[derive(Serialize)]
#[serde(rename = "CopyObjectResult")]
pub struct CopyObjectResultXml {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "LastModified")]
    pub last_modified: String,
    #[serde(rename = "ETag")]
    pub etag: String,
}

// ---- LocationConstraint (GetBucketLocation) -------------------------------

#[derive(Serialize)]
#[serde(rename = "LocationConstraint")]
pub struct LocationConstraintResult {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "$value")]
    pub location: String,
}

// ---- DeleteResult (DeleteObjects) -----------------------------------------

#[derive(Serialize)]
#[serde(rename = "DeleteResult")]
pub struct DeleteResultXml {
    #[serde(rename = "Deleted", skip_serializing_if = "Vec::is_empty")]
    pub deleted: Vec<DeletedEntry>,
    #[serde(rename = "Error", skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<DeleteErrorXml>,
}

#[derive(Serialize)]
pub struct DeletedEntry {
    #[serde(rename = "Key")]
    pub key: String,
}

#[derive(Serialize)]
pub struct DeleteErrorXml {
    #[serde(rename = "Key")]
    pub key: String,
    #[serde(rename = "Code")]
    pub code: String,
    #[serde(rename = "Message")]
    pub message: String,
}

// ---- ErrorResponse --------------------------------------------------------

#[derive(Serialize)]
#[serde(rename = "Error")]
pub struct ErrorResponse {
    #[serde(rename = "Code")]
    pub code: String,
    #[serde(rename = "Message")]
    pub message: String,
    #[serde(rename = "Resource")]
    pub resource: String,
    #[serde(rename = "RequestId")]
    pub request_id: String,
}

// ---------------------------------------------------------------------------
// Response builders
// ---------------------------------------------------------------------------

/// XML declaration prepended to every S3 XML response.
const XML_DECLARATION: &str = r#"<?xml version="1.0" encoding="UTF-8"?>"#;
pub const S3_XMLNS: &str = "http://s3.amazonaws.com/doc/2006-03-01/";

/// Build a successful S3 XML response.
///
/// Returns `(StatusCode, HeaderMap, String)` with `Content-Type: application/xml`.
pub fn s3_xml_response<T: Serialize>(body: &T) -> (StatusCode, HeaderMap, String) {
    let xml_body = quick_xml::se::to_string(body).unwrap_or_default();
    let full_body = format!("{}\n{}", XML_DECLARATION, xml_body);
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/xml"),
    );
    (StatusCode::OK, headers, full_body)
}

/// Build an S3 XML error response.
///
/// Returns `(StatusCode, HeaderMap, String)` with `Content-Type: application/xml`.
pub fn s3_error_response(
    status: StatusCode,
    s3_code: &str,
    message: &str,
    request_id: &str,
    resource: &str,
) -> (StatusCode, HeaderMap, String) {
    let error = ErrorResponse {
        code: s3_code.to_string(),
        message: message.to_string(),
        resource: resource.to_string(),
        request_id: request_id.to_string(),
    };
    let xml_body = quick_xml::se::to_string(&error).unwrap_or_else(|_| {
        format!(
            "<Error><Code>{}</Code><Message>{}</Message></Error>",
            s3_code, message
        )
    });
    let full_body = format!("{}{}", XML_DECLARATION, xml_body);
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/xml"),
    );
    (status, headers, full_body)
}

// ---------------------------------------------------------------------------
// Date/time helpers
// ---------------------------------------------------------------------------

/// Format a Unix timestamp (seconds) to S3 ISO 8601: `2025-01-01T00:00:00.000Z`.
pub fn format_s3_timestamp(timestamp_secs: i64) -> String {
    chrono::DateTime::from_timestamp(timestamp_secs, 0)
        .unwrap_or_default()
        .format("%Y-%m-%dT%H:%M:%S.000Z")
        .to_string()
}

/// Format a Unix timestamp (seconds) to HTTP header format: `Wed, 01 Jan 2025 00:00:00 GMT`.
pub fn format_http_timestamp(timestamp_secs: i64) -> String {
    chrono::DateTime::from_timestamp(timestamp_secs, 0)
        .unwrap_or_default()
        .format("%a, %d %b %Y %H:%M:%S GMT")
        .to_string()
}
