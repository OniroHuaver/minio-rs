//! S3 user metadata integration tests (Standalone, 1 disk).
//!
//! Covers: x-amz-meta-* header roundtrip through PutObject → GetObject / HeadObject.

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
// Basic metadata roundtrip
// ============================================================================

#[tokio::test]
async fn single_metadata_key() {
    let (_server, client) = setup().await;
    create_bucket(&client, "meta").await;

    let resp = client
        .put_object_with_meta("meta", "obj.txt", b"data", &[("color", "red")])
        .await;
    assert_eq!(resp.status(), 200);

    let resp = client.get_object("meta", "obj.txt").await;
    assert_eq!(resp.status(), 200);
    let val = resp
        .headers()
        .get("x-amz-meta-color")
        .and_then(|v| v.to_str().ok());
    assert_eq!(val, Some("red"));
}

#[tokio::test]
async fn multiple_metadata_keys() {
    let (_server, client) = setup().await;
    create_bucket(&client, "meta2").await;

    let resp = client
        .put_object_with_meta(
            "meta2",
            "multi.txt",
            b"data",
            &[("color", "red"), ("author", "alice"), ("version", "1")],
        )
        .await;
    assert_eq!(resp.status(), 200);

    let resp = client.get_object("meta2", "multi.txt").await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("x-amz-meta-color")
            .and_then(|v| v.to_str().ok()),
        Some("red")
    );
    assert_eq!(
        resp.headers()
            .get("x-amz-meta-author")
            .and_then(|v| v.to_str().ok()),
        Some("alice")
    );
    assert_eq!(
        resp.headers()
            .get("x-amz-meta-version")
            .and_then(|v| v.to_str().ok()),
        Some("1")
    );
}

// ============================================================================
// Metadata via HeadObject
// ============================================================================

#[tokio::test]
async fn metadata_via_head_object() {
    let (_server, client) = setup().await;
    create_bucket(&client, "meta-head").await;

    client
        .put_object_with_meta("meta-head", "h.txt", b"data", &[("tag", "important")])
        .await;

    let resp = client.head_object("meta-head", "h.txt").await;
    assert_eq!(resp.status(), 200);
    let val = resp
        .headers()
        .get("x-amz-meta-tag")
        .and_then(|v| v.to_str().ok());
    assert_eq!(val, Some("important"));
    // HEAD body must be empty
    assert!(resp.text().await.unwrap().is_empty());
}

// ============================================================================
// Metadata edge cases
// ============================================================================

#[tokio::test]
async fn metadata_empty_value() {
    let (_server, client) = setup().await;
    create_bucket(&client, "meta-empty").await;

    let resp = client
        .put_object_with_meta("meta-empty", "empty.txt", b"data", &[("blank", "")])
        .await;
    assert_eq!(resp.status(), 200);

    let resp = client.get_object("meta-empty", "empty.txt").await;
    assert_eq!(resp.status(), 200);
    let val = resp
        .headers()
        .get("x-amz-meta-blank")
        .and_then(|v| v.to_str().ok());
    assert_eq!(val, Some(""), "empty metadata value should roundtrip");
}

#[tokio::test]
async fn metadata_case_preservation() {
    let (_server, client) = setup().await;
    create_bucket(&client, "meta-case").await;

    // HTTP headers are case-insensitive; x-amz-meta- keys should be lowercase
    let resp = client
        .put_object_with_meta("meta-case", "case.txt", b"data", &[("MyKey", "MyValue")])
        .await;
    assert_eq!(resp.status(), 200);

    let resp = client.get_object("meta-case", "case.txt").await;
    assert_eq!(resp.status(), 200);
    // Headers are normalized to lower-case by HTTP/2 and many HTTP stacks
    let val_lower = resp
        .headers()
        .get("x-amz-meta-mykey")
        .and_then(|v| v.to_str().ok());
    let val_orig = resp
        .headers()
        .get("x-amz-meta-MyKey")
        .and_then(|v| v.to_str().ok());
    // At least one variant should be present
    assert!(
        val_lower.is_some() || val_orig.is_some(),
        "metadata MyKey should be retrievable"
    );
}

#[tokio::test]
async fn metadata_special_chars_in_value() {
    let (_server, client) = setup().await;
    create_bucket(&client, "meta-special").await;

    let resp = client
        .put_object_with_meta(
            "meta-special",
            "special.txt",
            b"data",
            &[("desc", "value with spaces and = signs")],
        )
        .await;
    assert_eq!(resp.status(), 200);

    let resp = client.get_object("meta-special", "special.txt").await;
    assert_eq!(resp.status(), 200);
    let val = resp
        .headers()
        .get("x-amz-meta-desc")
        .and_then(|v| v.to_str().ok());
    assert_eq!(val, Some("value with spaces and = signs"));
}

#[tokio::test]
async fn metadata_many_keys() {
    let (_server, client) = setup().await;
    create_bucket(&client, "meta-many").await;

    let meta: Vec<(&str, &str)> = (0..20)
        .map(|i| {
            let key = Box::leak(format!("key-{i:02}").into_boxed_str());
            let val = Box::leak(format!("value-{i:02}").into_boxed_str());
            (key as &str, val as &str)
        })
        .collect();

    let resp = client
        .put_object_with_meta("meta-many", "many.txt", b"data", &meta)
        .await;
    assert_eq!(resp.status(), 200);

    let resp = client.head_object("meta-many", "many.txt").await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("x-amz-meta-key-00")
            .and_then(|v| v.to_str().ok()),
        Some("value-00")
    );
    assert_eq!(
        resp.headers()
            .get("x-amz-meta-key-19")
            .and_then(|v| v.to_str().ok()),
        Some("value-19")
    );
}

#[tokio::test]
async fn metadata_not_returned_on_unrelated_objects() {
    let (_server, client) = setup().await;
    create_bucket(&client, "meta-iso").await;

    client
        .put_object_with_meta("meta-iso", "with-meta.txt", b"data", &[("color", "blue")])
        .await;
    client.put_object("meta-iso", "no-meta.txt", b"data").await;

    let resp = client.get_object("meta-iso", "no-meta.txt").await;
    assert_eq!(resp.status(), 200);
    // None of the x-amz-meta-* headers from the other object should leak
    let has_meta = resp
        .headers()
        .keys()
        .any(|k| k.as_str().to_lowercase().starts_with("x-amz-meta-"));
    assert!(
        !has_meta,
        "object without metadata should not have x-amz-meta-* headers"
    );
}

#[tokio::test]
async fn metadata_no_keys() {
    let (_server, client) = setup().await;
    create_bucket(&client, "meta-none").await;

    // PUT with empty metadata list
    let resp = client
        .put_object_with_meta("meta-none", "plain.txt", b"data", &[])
        .await;
    assert_eq!(resp.status(), 200);

    let resp = client.get_object("meta-none", "plain.txt").await;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.bytes().await.unwrap().as_ref(), b"data");
}
