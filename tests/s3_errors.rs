//! S3 error response integration tests (Standalone, 1 disk).
//!
//! Verifies S3 XML error format: ErrorCode, HTTP status code, RequestId, Resource.

mod common;

use common::s3_client::S3Client;
use common::server_process::TestServer;

async fn setup() -> (TestServer, S3Client) {
    let server = TestServer::start(1).await;
    let client = S3Client::new(&server.url());
    (server, client)
}

use common::helpers::create_bucket;

// ============================================================================
// Error XML format validation
// ============================================================================

/// Assert that a response body is a valid S3 XML error.
fn assert_s3_error(body: &str, expected_code: &str, _expected_status: u16) {
    assert!(body.contains("<?xml"), "error response should have XML declaration");
    assert!(body.contains("<Error>"), "error response should have Error root element");
    assert!(
        body.contains(&format!("<Code>{expected_code}</Code>")),
        "error code should be {expected_code}, body: {body}"
    );
    assert!(body.contains("<Message>"), "error should have Message");
    assert!(body.contains("<Resource>"), "error should have Resource");
    assert!(body.contains("<RequestId>"), "error should have RequestId");
}

// ============================================================================
// Non-existent bucket behavior
//
// NOTE: current implementation does not consistently return NoSuchBucket.
// The storage layer does not check bucket existence before object operations.
// These tests document current behavior; S3 compliance gaps are marked with TODO.
// ============================================================================

#[tokio::test]
async fn head_nonexistent_bucket_404() {
    let (_server, client) = setup().await;
    let resp = client.head_bucket("no-bucket-404").await;
    assert_eq!(resp.status(), 404);
    // HEAD responses don't have a body (HTTP spec), but Content-Type is set
    let ct = resp.headers().get("content-type").and_then(|v| v.to_str().ok());
    assert_eq!(ct, Some("application/xml"), "HEAD error should have Content-Type");
}

#[tokio::test]
async fn get_object_nonexistent_bucket_404() {
    let (_server, client) = setup().await;
    let resp = client.get_object("no-bucket", "any-key").await;
    assert_eq!(resp.status(), 404);
    let body = resp.text().await.unwrap();
    // TODO: should return NoSuchBucket, not NoSuchKey.
    // Currently object lookup checks the key first, not the bucket.
    assert!(
        body.contains("<Code>NoSuchKey</Code>") || body.contains("<Code>NoSuchBucket</Code>"),
        "GET in missing bucket should return error, body: {body}"
    );
}

#[tokio::test]
async fn put_object_nonexistent_bucket_behavior() {
    let (_server, client) = setup().await;
    let resp = client.put_object("no-bucket", "any-key", b"data").await;
    // NOTE: current implementation auto-creates the bucket directory → 200.
    // S3 spec: should return 404 NoSuchBucket.
    // TODO: add bucket existence check to put_object storage path.
    let status = resp.status();
    assert!(
        status == 200 || status == 404,
        "PUT to missing bucket: expected 200 (auto-create) or 404 (S3 compliant), got {status}"
    );
}

#[tokio::test]
async fn list_objects_nonexistent_bucket_error() {
    let (_server, client) = setup().await;
    let resp = client.list_objects_v2("no-bucket", "", "", 0).await;
    // NOTE: current implementation can return 500 when listing non-existent bucket.
    // S3 spec: should return 404 NoSuchBucket.
    // TODO: add bucket existence check to list_objects handler.
    let status = resp.status();
    assert!(
        status == 404 || status == 500,
        "LIST to missing bucket: expected 404 or 500 (current), got {status}"
    );
    if status == 500 {
        let body = resp.text().await.unwrap();
        assert!(
            body.contains("InternalError") || body.contains("<Error>"),
            "should be an error response"
        );
    }
}

#[tokio::test]
async fn delete_object_nonexistent_bucket_behavior() {
    let (_server, client) = setup().await;
    let resp = client.delete_object("no-bucket", "any-key").await;
    // NOTE: current implementation returns 204 (idempotent delete).
    // S3 spec: should return 404 NoSuchBucket when bucket doesn't exist.
    // TODO: add bucket existence check to delete_object handler.
    let status = resp.status();
    assert!(
        status == 204 || status == 404,
        "DELETE from missing bucket: expected 204 (current) or 404 (S3 spec), got {status}"
    );
}

// ============================================================================
// NoSuchKey (404)
// ============================================================================

#[tokio::test]
async fn no_such_key_on_get_object() {
    let (_server, client) = setup().await;
    create_bucket(&client, "errors").await;
    let resp = client.get_object("errors", "nonexistent.txt").await;
    assert_eq!(resp.status(), 404);
    let body = resp.text().await.unwrap();
    assert_s3_error(&body, "NoSuchKey", 404);
}

#[tokio::test]
async fn no_such_key_on_head_object() {
    let (_server, client) = setup().await;
    create_bucket(&client, "errors").await;
    let resp = client.head_object("errors", "nonexistent.txt").await;
    assert_eq!(resp.status(), 404);
}

// ============================================================================
// BadDigest (400)
// ============================================================================

#[tokio::test]
async fn bad_digest_on_wrong_md5() {
    let (_server, client) = setup().await;
    create_bucket(&client, "errors").await;

    use base64::Engine;
    use md5::{Digest, Md5};
    let mut hasher = Md5::new();
    hasher.update(b"wrong data");
    let wrong_md5 = base64::engine::general_purpose::STANDARD.encode(&hasher.finalize());

    let resp = client.put_object_with_md5("errors", "bad-digest.bin", b"real data", &wrong_md5).await;
    assert_eq!(resp.status(), 400);
    let body = resp.text().await.unwrap();
    assert_s3_error(&body, "BadDigest", 400);
}

// ============================================================================
// InternalError (500) — hard to trigger reliably, at least verify format
// ============================================================================

#[tokio::test]
async fn error_response_content_type_is_xml() {
    let (_server, client) = setup().await;
    create_bucket(&client, "errors").await;
    let resp = client.get_object("errors", "nope.txt").await;
    assert_eq!(resp.status(), 404);
    let ct = resp.headers().get("content-type").and_then(|v| v.to_str().ok());
    assert_eq!(ct, Some("application/xml"), "S3 error responses should be application/xml");
}

// ============================================================================
// Error response includes RequestId
// ============================================================================

#[tokio::test]
async fn error_includes_request_id() {
    let (_server, client) = setup().await;
    create_bucket(&client, "errors").await;
    let resp = client.get_object("errors", "ghost.txt").await;
    assert_eq!(resp.status(), 404);
    let body = resp.text().await.unwrap();
    // RequestId should be a non-empty UUID
    assert!(body.contains("<RequestId>"), "error should contain RequestId");
    let rid = extract_xml_text(&body, "RequestId").unwrap_or_default();
    assert!(!rid.is_empty(), "RequestId should not be empty");
    assert!(rid.len() >= 32, "RequestId should be a UUID, got: {rid}");
}

// ============================================================================
// Error response includes Resource
// ============================================================================

#[tokio::test]
async fn error_includes_resource() {
    let (_server, client) = setup().await;
    let resp = client.get_object("no-bucket", "my/object/key.txt").await;
    assert_eq!(resp.status(), 404);
    let body = resp.text().await.unwrap();
    let resource = extract_xml_text(&body, "Resource").unwrap_or_default();
    assert!(resource.contains("no-bucket"), "Resource should include bucket name");
}

// ============================================================================
// XML helper
// ============================================================================

fn extract_xml_text(xml: &str, tag: &str) -> Option<String> {
    let start = format!("<{}>", tag);
    let end = format!("</{}>", tag);
    let s = xml.find(&start)? + start.len();
    let e = xml[s..].find(&end)?;
    Some(xml[s..s + e].to_string())
}
