//! Standalone (single-disk) S3 API integration tests.
//!
//! Every test starts a fresh minio server with 1 disk (StandaloneObjects path)
//! and exercises the S3 REST API through a simple HTTP client.
//!
//! ## Covered S3 operations
//!
//! | Method | Path              | Query        | S3 Operation   |
//! |--------|-------------------|--------------|----------------|
//! | GET    | `/`               | —            | ListBuckets    |
//! | PUT    | `/{bucket}`       | —            | CreateBucket   |
//! | HEAD   | `/{bucket}`       | —            | HeadBucket     |
//! | DELETE | `/{bucket}`       | —            | DeleteBucket   |
//! | GET    | `/{bucket}`       | `list-type=2`| ListObjectsV2  |
//! | PUT    | `/{bucket}/{key}` | —            | PutObject      |
//! | GET    | `/{bucket}/{key}` | —            | GetObject      |
//! | HEAD   | `/{bucket}/{key}` | —            | HeadObject     |
//! | DELETE | `/{bucket}/{key}` | —            | DeleteObject   |

mod common;

use common::s3_client::S3Client;
use common::server_process::TestServer;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Start a **single-disk** server (StandaloneObjects) and return a client.
async fn setup_standalone() -> (TestServer, S3Client) {
    let server = TestServer::start(1).await;
    let client = S3Client::new(&server.url());
    (server, client)
}

use common::helpers::{create_bucket, make_data};

// ============================================================================
// 1. GET / — ListBuckets
// ============================================================================

#[tokio::test]
async fn standalone_list_buckets_empty() {
    let (_server, client) = setup_standalone().await;

    let resp = client.list_buckets().await;
    assert_eq!(resp.status(), 200, "list_buckets on empty server → 200");
    let body = resp.text().await.expect("body");
    assert!(body.contains("<ListAllMyBucketsResult"), "should be ListAllMyBucketsResult XML");
    assert!(body.contains("xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\""));
}

#[tokio::test]
async fn standalone_list_buckets_with_data() {
    let (_server, client) = setup_standalone().await;

    for name in &["alpha", "bravo", "charlie"] {
        create_bucket(&client, name).await;
    }

    let resp = client.list_buckets().await;
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.expect("body");

    for name in &["alpha", "bravo", "charlie"] {
        assert!(body.contains(name), "list response should contain bucket `{name}`");
    }
}

// ============================================================================
// 2. PUT /{bucket} — CreateBucket
// ============================================================================

#[tokio::test]
async fn standalone_create_bucket_ok() {
    let (_server, client) = setup_standalone().await;

    let resp = client.create_bucket("mybucket").await;
    assert_eq!(resp.status(), 200, "create_bucket → 200");
}

#[tokio::test]
async fn standalone_create_bucket_idempotent() {
    let (_server, client) = setup_standalone().await;

    // First creation
    let resp = client.create_bucket("dup").await;
    assert_eq!(resp.status(), 200);

    // Second creation — must return 409 BucketAlreadyExists
    let resp = client.create_bucket("dup").await;
    assert_eq!(resp.status(), 409, "duplicate create_bucket should return 409 Conflict");
}

// ============================================================================
// 3. HEAD /{bucket} — HeadBucket (BucketExists)
// ============================================================================

#[tokio::test]
async fn standalone_head_bucket_exists() {
    let (_server, client) = setup_standalone().await;
    create_bucket(&client, "head-me").await;

    let resp = client.head_bucket("head-me").await;
    assert_eq!(resp.status(), 200, "HEAD existing bucket → 200");
}

#[tokio::test]
async fn standalone_head_bucket_not_found() {
    let (_server, client) = setup_standalone().await;

    let resp = client.head_bucket("no-such").await;
    // HEAD responses have no body (axum follows HTTP spec); S3 error is conveyed via status code
    assert_eq!(resp.status(), 404, "HEAD non-existent bucket → 404");
    // NOTE: x-amz-error-code header could carry the S3 error code in a production
    // implementation; for Phase 1 we rely on HTTP status alone for HEAD requests.
}

// ============================================================================
// 4. DELETE /{bucket} — DeleteBucket
// ============================================================================

#[tokio::test]
async fn standalone_delete_bucket_ok() {
    let (_server, client) = setup_standalone().await;
    create_bucket(&client, "to-delete").await;

    let resp = client.delete_bucket("to-delete").await;
    assert_eq!(resp.status(), 204, "delete_bucket → 204 No Content");

    // Verify bucket is gone
    let resp = client.head_bucket("to-delete").await;
    assert_eq!(resp.status(), 404, "HEAD after DELETE → 404");
}

#[tokio::test]
async fn standalone_delete_bucket_not_found() {
    let (_server, client) = setup_standalone().await;

    // Deleting a non-existent bucket — delete_volume handles NotFound idempotently
    let resp = client.delete_bucket("ghost-bucket").await;
    assert_eq!(resp.status(), 204, "delete non-existent bucket → 204");
}

// ============================================================================
// 5. GET /{bucket}?list-type=2 — ListObjectsV2
// ============================================================================

#[tokio::test]
async fn standalone_list_objects_v2_empty_bucket() {
    let (_server, client) = setup_standalone().await;
    create_bucket(&client, "empty-list").await;

    let resp = client.list_objects_v2("empty-list", "", "", 0).await;
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.expect("body");
    assert!(body.contains("<ListBucketResult"), "should be ListBucketResult XML");
    assert!(body.contains("<KeyCount>0</KeyCount>") || body.contains("<KeyCount>0</KeyCount>"),
        "empty bucket should have KeyCount=0");
}

#[tokio::test]
async fn standalone_list_objects_v2_flat_keys() {
    let (_server, client) = setup_standalone().await;
    create_bucket(&client, "flat-keys").await;

    // Flat keys (no "/" nested path) — these work correctly with list_dir
    for i in 0..5 {
        let key = format!("obj{:02}", i);
        client.put_object("flat-keys", &key, b"data").await;
    }

    let resp = client.list_objects_v2("flat-keys", "", "", 0).await;
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.expect("body");

    for i in 0..5 {
        let key = format!("obj{:02}", i);
        assert!(body.contains(&key), "list should contain `{key}`");
    }
}

#[tokio::test]
async fn standalone_list_objects_v2_with_prefix() {
    let (_server, client) = setup_standalone().await;
    create_bucket(&client, "prefix-test").await;

    client.put_object("prefix-test", "a/alpha.txt", b"a").await;
    client.put_object("prefix-test", "a/apple.txt", b"b").await;
    client.put_object("prefix-test", "b/banana.txt", b"c").await;

    // NOTE: nested keys like "a/alpha.txt" rely on list_dir seeing "a/alpha.txt"
    // as an entry. When prefix="a/", list_dir at "bucket/a/" returns ["alpha.txt",
    // "apple.txt"].  Each entry has a corresponding xl.meta inside its sub-directory.
    // This path works correctly because list_dir lists immediate children of the
    // prefix directory, and those children ARE the object directories.

    let resp = client.list_objects_v2("prefix-test", "a/", "", 0).await;
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.expect("body");

    assert!(body.contains("alpha.txt"), "should contain alpha.txt under prefix a/");
    assert!(body.contains("apple.txt"), "should contain apple.txt under prefix a/");
    assert!(!body.contains("banana.txt"), "should NOT contain banana.txt under prefix a/");
}

#[tokio::test]
async fn standalone_list_objects_v2_with_delimiter() {
    let (_server, client) = setup_standalone().await;
    create_bucket(&client, "delim-test").await;

    client.put_object("delim-test", "photos/sunset.jpg", b"img").await;
    client.put_object("delim-test", "photos/sunrise.jpg", b"img").await;
    client.put_object("delim-test", "readme.txt", b"txt").await;

    let resp = client.list_objects_v2("delim-test", "", "/", 0).await;
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.expect("body");

    assert!(body.contains("readme.txt"), "top-level object should appear in listing");
    assert!(body.contains("<CommonPrefixes>"), "should have CommonPrefixes for photos/");
    // NOTE: photos/sunset.jpg won't be individually listed because it's under "photos/" dir
    // and with delimiter="/", only the prefix "photos/" appears in CommonPrefixes
}

// ============================================================================
// 6. PUT /{bucket}/{*key} — PutObject
// ============================================================================

#[tokio::test]
async fn standalone_put_object_small() {
    let (_server, client) = setup_standalone().await;
    create_bucket(&client, "put-bucket").await;

    let data = b"hello minio-rs standalone!";
    let resp = client.put_object("put-bucket", "greeting.txt", data).await;
    assert_eq!(resp.status(), 200, "put_object → 200");

    // Verify ETag is returned
    let etag = resp.headers().get("etag").and_then(|v| v.to_str().ok());
    assert!(etag.is_some(), "put_object should return ETag header");
}

#[tokio::test]
async fn standalone_put_object_large() {
    let (_server, client) = setup_standalone().await;
    create_bucket(&client, "large-bucket").await;

    // 256 KiB — above the 128 KiB inline threshold but handled fine by standalone
    let data = make_data(256 * 1024);
    let resp = client.put_object("large-bucket", "big.bin", &data).await;
    assert_eq!(resp.status(), 200);
}

// ============================================================================
// 7. GET /{bucket}/{*key} — GetObject
// ============================================================================

#[tokio::test]
async fn standalone_get_object_roundtrip() {
    let (_server, client) = setup_standalone().await;
    create_bucket(&client, "roundtrip").await;

    let data = make_data(64 * 1024); // 64 KiB
    client.put_object("roundtrip", "data.bin", &data).await;

    let resp = client.get_object("roundtrip", "data.bin").await;
    assert_eq!(resp.status(), 200, "get_object → 200");
    let body = resp.bytes().await.expect("body");
    assert_eq!(body.as_ref(), data.as_slice(), "GET data must match PUT data");
}

#[tokio::test]
async fn standalone_get_object_not_found() {
    let (_server, client) = setup_standalone().await;
    create_bucket(&client, "get-nope").await;

    let resp = client.get_object("get-nope", "ghost.txt").await;
    assert_eq!(resp.status(), 404, "GET non-existent → 404");
    let body = resp.text().await.expect("body");
    assert!(body.contains("NoSuchKey"), "error code should be NoSuchKey");
}

#[tokio::test]
async fn standalone_get_object_with_metadata_headers() {
    let (_server, client) = setup_standalone().await;
    create_bucket(&client, "meta-bucket").await;

    let data = b"metadata test data";
    let resp = client
        .put_object_with_meta("meta-bucket", "meta.txt", data, &[("color", "red"), ("author", "alice")])
        .await;
    assert_eq!(resp.status(), 200);

    let resp = client.get_object("meta-bucket", "meta.txt").await;
    assert_eq!(resp.status(), 200);

    // User metadata should be returned as x-amz-meta-* headers
    let color = resp.headers().get("x-amz-meta-color").and_then(|v| v.to_str().ok());
    let author = resp.headers().get("x-amz-meta-author").and_then(|v| v.to_str().ok());
    assert_eq!(color, Some("red"), "x-amz-meta-color should be `red`");
    assert_eq!(author, Some("alice"), "x-amz-meta-author should be `alice`");

    let body = resp.bytes().await.expect("body");
    assert_eq!(body.as_ref(), data, "body should match put data");
}

// ============================================================================
// 8. HEAD /{bucket}/{*key} — HeadObject
// ============================================================================

#[tokio::test]
async fn standalone_head_object_ok() {
    let (_server, client) = setup_standalone().await;
    create_bucket(&client, "head-obj").await;

    let data = b"head me if you can";
    let put_resp = client.put_object("head-obj", "target.txt", data).await;
    assert_eq!(put_resp.status(), 200);
    let put_etag = put_resp.headers().get("etag").and_then(|v| v.to_str().ok());

    let resp = client.head_object("head-obj", "target.txt").await;
    assert_eq!(resp.status(), 200, "HEAD → 200");

    let head_etag = resp.headers().get("etag").and_then(|v| v.to_str().ok());
    assert_eq!(head_etag, put_etag, "HEAD etag must match PUT etag");

    // Content-Type should be set
    let content_type = resp.headers().get("content-type").and_then(|v| v.to_str().ok());
    assert!(content_type.is_some(), "HEAD should include Content-Type");
}

#[tokio::test]
async fn standalone_head_object_not_found() {
    let (_server, client) = setup_standalone().await;
    create_bucket(&client, "head-nope").await;

    let resp = client.head_object("head-nope", "nobody.txt").await;
    assert_eq!(resp.status(), 404, "HEAD non-existent → 404");
}

// ============================================================================
// 9. DELETE /{bucket}/{*key} — DeleteObject
// ============================================================================

#[tokio::test]
async fn standalone_delete_object_ok() {
    let (_server, client) = setup_standalone().await;
    create_bucket(&client, "delete-obj").await;

    let data = b"born to die";
    client.put_object("delete-obj", "ephemeral.txt", data).await;

    let resp = client.delete_object("delete-obj", "ephemeral.txt").await;
    assert_eq!(resp.status(), 204, "delete_object → 204 No Content");

    // After DELETE, GET should return 404 (tombstone-aware read is implemented)
    let resp = client.get_object("delete-obj", "ephemeral.txt").await;
    assert_eq!(resp.status(), 404, "GET after DELETE → 404");
}

#[tokio::test]
async fn standalone_delete_object_not_found() {
    let (_server, client) = setup_standalone().await;
    create_bucket(&client, "delete-ghost").await;

    // Deleting a non-existent object must be idempotent (S3 spec)
    let resp = client.delete_object("delete-ghost", "phantom.txt").await;
    assert_eq!(resp.status(), 204, "delete non-existent → 204 (idempotent)");
}

#[tokio::test]
async fn standalone_delete_then_reput() {
    let (_server, client) = setup_standalone().await;
    create_bucket(&client, "delete-reput").await;

    let original = b"original content";
    client.put_object("delete-reput", "cycle.txt", original).await;
    client.delete_object("delete-reput", "cycle.txt").await;

    // PUT again after DELETE — overwrites xl.meta with fresh Object entry
    let new_data = b"new content after delete";
    let resp = client.put_object("delete-reput", "cycle.txt", new_data).await;
    assert_eq!(resp.status(), 200, "re-PUT after DELETE → 200");

    let resp = client.get_object("delete-reput", "cycle.txt").await;
    assert_eq!(resp.status(), 200);
    let body = resp.bytes().await.expect("body");
    assert_eq!(body.as_ref(), new_data, "re-PUT data should be the new content");
}

// ============================================================================
// Range GET
// ============================================================================

#[tokio::test]
async fn standalone_get_object_range() {
    let (_server, client) = setup_standalone().await;
    create_bucket(&client, "range-test").await;

    let data = make_data(512);
    client.put_object("range-test", "range.bin", &data).await;

    let resp = client.get_object_range("range-test", "range.bin", "bytes=0-99").await;
    assert_eq!(resp.status(), 206, "range GET → 206 Partial Content");
    let body = resp.bytes().await.expect("body");
    assert_eq!(body.as_ref(), &data[..100], "range body [0,99] should match");
}

// ============================================================================
// Cross-operation scenario test
// ============================================================================

#[tokio::test]
async fn standalone_full_crud_scenario() {
    let (_server, client) = setup_standalone().await;

    // 1. List empty
    let resp = client.list_buckets().await;
    assert_eq!(resp.status(), 200);

    // 2. Create bucket
    create_bucket(&client, "scenario").await;

    // 3. Verify bucket exists
    let resp = client.head_bucket("scenario").await;
    assert_eq!(resp.status(), 200);

    // 4. PUT several objects
    for i in 0..3 {
        let key = format!("file{}.txt", i);
        let data = format!("content {i}").into_bytes();
        let resp = client.put_object("scenario", &key, &data).await;
        assert_eq!(resp.status(), 200);
    }

    // 5. GET object and verify
    let resp = client.get_object("scenario", "file0.txt").await;
    assert_eq!(resp.status(), 200);
    let body = resp.bytes().await.expect("body");
    assert_eq!(body.as_ref(), b"content 0");

    // 6. HEAD object
    let resp = client.head_object("scenario", "file1.txt").await;
    assert_eq!(resp.status(), 200);

    // 7. List objects
    let resp = client.list_objects_v2("scenario", "", "", 0).await;
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.expect("body");
    for i in 0..3 {
        assert!(body.contains(&format!("file{}.txt", i)));
    }

    // 8. DELETE one object
    let resp = client.delete_object("scenario", "file2.txt").await;
    assert_eq!(resp.status(), 204);

    // 9. Verify deletion
    let resp = client.get_object("scenario", "file2.txt").await;
    assert_eq!(resp.status(), 404);

    // 10. Delete bucket
    let resp = client.delete_bucket("scenario").await;
    assert_eq!(resp.status(), 204);
}
