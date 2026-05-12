//! S3 Content-MD5 validation integration tests (Standalone, 1 disk).
//!
//! Verifies server-side Content-MD5 header validation on PutObject.

mod common;

use common::s3_client::S3Client;
use common::server_process::TestServer;

async fn setup() -> (TestServer, S3Client) {
    let server = TestServer::start(1).await;
    let client = S3Client::new(&server.url());
    (server, client)
}

use common::helpers::create_bucket;

/// Compute base64-encoded MD5 of data.
fn md5_base64(data: &[u8]) -> String {
    use base64::Engine;
    use md5::{Digest, Md5};
    let mut hasher = Md5::new();
    hasher.update(data);
    let digest = hasher.finalize();
    base64::engine::general_purpose::STANDARD.encode(&digest)
}

// ============================================================================
// Valid Content-MD5
// ============================================================================

#[tokio::test]
async fn valid_md5_small_object() {
    let (_server, client) = setup().await;
    create_bucket(&client, "md5-ok").await;

    let data = b"hello content-md5 world!";
    let md5 = md5_base64(data);
    let resp = client.put_object_with_md5("md5-ok", "valid.bin", data, &md5).await;
    assert_eq!(resp.status(), 200, "valid MD5 should return 200");
}

#[tokio::test]
async fn valid_md5_large_object() {
    let (_server, client) = setup().await;
    create_bucket(&client, "md5-large").await;

    let data = (0..256 * 1024).map(|i| (i % 256) as u8).collect::<Vec<_>>();
    let md5 = md5_base64(&data);
    let resp = client.put_object_with_md5("md5-large", "large.bin", &data, &md5).await;
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn valid_md5_zero_bytes() {
    let (_server, client) = setup().await;
    create_bucket(&client, "md5-zero").await;

    let data: Vec<u8> = vec![];
    let md5 = md5_base64(&data);
    let resp = client.put_object_with_md5("md5-zero", "empty.bin", &data, &md5).await;
    assert_eq!(resp.status(), 200, "valid MD5 on zero-byte object → 200");
}

// ============================================================================
// Invalid Content-MD5 → BadDigest
// ============================================================================

#[tokio::test]
async fn invalid_md5_bad_digest() {
    let (_server, client) = setup().await;
    create_bucket(&client, "md5-bad").await;

    let data = b"actual data";
    let wrong_md5 = md5_base64(b"different data");
    let resp = client.put_object_with_md5("md5-bad", "bad.bin", data, &wrong_md5).await;
    assert_eq!(resp.status(), 400, "mismatched MD5 should return 400 Bad Request");
    let body = resp.text().await.unwrap();
    assert!(body.contains("BadDigest"), "error should be BadDigest, got: {body}");
}

#[tokio::test]
async fn malformed_md5_header() {
    let (_server, client) = setup().await;
    create_bucket(&client, "md5-mal").await;

    // Not valid base64
    let resp = client
        .put_object_with_md5("md5-mal", "mal.bin", b"data", "!!!not-valid-base64!!!")
        .await;
    // The handler decodes the MD5; if decoding fails, the checksum check
    // is skipped (the header is treated as absent). So this should pass.
    assert_eq!(resp.status(), 200, "malformed MD5 header should be ignored → 200");
}

#[tokio::test]
async fn missing_md5_header_accepted() {
    let (_server, client) = setup().await;
    create_bucket(&client, "md5-none").await;

    // Regular put (no Content-MD5) should succeed
    let resp = client.put_object("md5-none", "no-md5.bin", b"data").await;
    assert_eq!(resp.status(), 200, "PUT without Content-MD5 should succeed");
}
