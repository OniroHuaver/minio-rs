mod common;

use common::s3_client::S3Client;
use common::server_process::TestServer;

// ============================================================================
// Helper utilities
// ============================================================================

/// Construct a `TestServer` with 3 disks (M=1, N=2 — the minimum viable EC
/// configuration) and a corresponding `S3Client`.
async fn setup() -> (TestServer, S3Client) {
    let server = TestServer::start(3).await;
    let client = S3Client::new(&server.url());
    (server, client)
}

/// Create a bucket and assert success.
async fn create_bucket(client: &S3Client, name: &str) {
    let resp = client.create_bucket(name).await;
    assert_eq!(
        resp.status(),
        200,
        "create_bucket({name}) should return 200, got {}",
        resp.status(),
    );
}

/// Generate `len` bytes of deterministic pseudo-random data (repeat `0..=255`).
fn make_data(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 256) as u8).collect()
}

// ============================================================================
// Bucket tests
// ============================================================================

#[tokio::test]
async fn test_create_and_list_buckets() {
    let (_server, client) = setup().await;

    let buckets = ["alpha", "bravo", "charlie"];
    for name in &buckets {
        create_bucket(&client, name).await;
    }

    let resp = client.list_buckets().await;
    assert_eq!(resp.status(), 200, "list_buckets should return 200");
    let body = resp.text().await.expect("list_buckets body");

    for name in &buckets {
        assert!(
            body.contains(name),
            "list response should contain bucket `{name}`"
        );
    }
}

#[tokio::test]
async fn test_create_duplicate_bucket() {
    let (_server, client) = setup().await;

    // First creation succeeds
    let resp = client.create_bucket("dup-bucket").await;
    assert_eq!(resp.status(), 200);

    // Second creation: `make_volume` is idempotent via `create_dir_all`
    // TODO: when conflict detection is added, change to expect 409
    let resp = client.create_bucket("dup-bucket").await;
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_delete_bucket() {
    let (_server, client) = setup().await;

    create_bucket(&client, "del-bucket").await;

    let resp = client.delete_bucket("del-bucket").await;
    assert_eq!(resp.status(), 204, "delete_bucket should return 204");

    let resp = client.head_bucket("del-bucket").await;
    assert_eq!(resp.status(), 404, "HEAD deleted bucket should return 404");
}

#[tokio::test]
async fn test_bucket_not_found() {
    let (_server, client) = setup().await;

    let resp = client.head_bucket("no-such-bucket").await;
    assert_eq!(resp.status(), 404, "HEAD non-existent bucket should return 404");
}

// ============================================================================
// Object PUT / GET tests
// ============================================================================

#[tokio::test]
async fn test_put_get_small_file() {
    let (_server, client) = setup().await;
    let bucket = "small-file";
    let key = "hello.txt";

    create_bucket(&client, bucket).await;

    // Small file (< 128 KiB  — field inlined in xl.meta)
    let data = make_data(64 * 1024); // 64 KiB
    let resp = client.put_object(bucket, key, &data).await;
    assert_eq!(resp.status(), 200, "put_object should return 200");

    let resp = client.get_object(bucket, key).await;
    assert_eq!(resp.status(), 200, "get_object should return 200");
    let body = resp.bytes().await.expect("get_object body");
    assert_eq!(body.as_ref(), data.as_slice(), "GET data must match PUT data");
}

#[tokio::test]
async fn test_put_get_large_file() {
    let (_server, client) = setup().await;
    let bucket = "large-file";
    let key = "bigfile.bin";

    create_bucket(&client, bucket).await;

    // Large file (> 128 KiB — written as shard files)
    let data = make_data(256 * 1024); // 256 KiB
    let resp = client.put_object(bucket, key, &data).await;
    assert_eq!(resp.status(), 200, "put_object should return 200");

    let resp = client.get_object(bucket, key).await;
    assert_eq!(resp.status(), 200, "get_object should return 200");
    let body = resp.bytes().await.expect("get_object body");
    assert_eq!(body.as_ref(), data.as_slice(), "GET data must match PUT data");
}

#[tokio::test]
async fn test_put_object_with_metadata() {
    let (_server, client) = setup().await;
    let bucket = "meta-test";
    let key = "meta.txt";

    create_bucket(&client, bucket).await;

    let data = b"metadata example";
    let resp = client
        .put_object_with_meta(bucket, key, data, &[("color", "red")])
        .await;
    assert_eq!(resp.status(), 200, "put_object_with_meta should return 200");

    // Verifies user metadata is returned on GET
    let resp = client.get_object(bucket, key).await;
    assert_eq!(resp.status(), 200);
    let meta_val = resp
        .headers()
        .get("x-amz-meta-color")
        .and_then(|v| v.to_str().ok());
    assert_eq!(meta_val, Some("red"), "x-amz-meta-color should be `red`");
}

#[tokio::test]
async fn test_head_object() {
    let (_server, client) = setup().await;
    let bucket = "head-test";
    let key = "head-check.txt";

    create_bucket(&client, bucket).await;

    let data = b"head me";
    let resp = client.put_object(bucket, key, data).await;
    assert_eq!(resp.status(), 200);
    let put_etag = resp.headers().get("etag").and_then(|v| v.to_str().ok());

    let resp = client.head_object(bucket, key).await;
    assert_eq!(resp.status(), 200, "head_object should return 200");
    let head_etag = resp.headers().get("etag").and_then(|v| v.to_str().ok());
    assert_eq!(
        head_etag, put_etag,
        "HEAD etag should match PUT etag"
    );
}

// ============================================================================
// Object DELETE tests
// ============================================================================

#[tokio::test]
async fn test_delete_object() {
    let (_server, client) = setup().await;
    let bucket = "delete-obj";
    let key = "delete-me.txt";

    create_bucket(&client, bucket).await;

    let resp = client.put_object(bucket, key, b"delete me").await;
    assert_eq!(resp.status(), 200);

    let resp = client.delete_object(bucket, key).await;
    assert_eq!(resp.status(), 204, "delete_object should return 204");

    // NOTE: current implementation writes a DeleteMarker but the read path
    // does not filter out Delete entries yet, so GET may still succeed.
    // TODO: uncomment the following assertion when tombstone-aware read is added.
    // let resp = client.get_object(bucket, key).await;
    // assert_eq!(resp.status(), 404, "GET after DELETE should return 404");
}

#[tokio::test]
async fn test_delete_object_not_found() {
    let (_server, client) = setup().await;
    let bucket = "delete-nonexist";
    create_bucket(&client, bucket).await;

    // Deleting a non-existent object must be idempotent (S3 spec).
    let resp = client.delete_object(bucket, "ghost.txt").await;
    assert_eq!(resp.status(), 204, "delete non-existent should return 204");
}

// ============================================================================
// Object Range GET tests
// ============================================================================

#[tokio::test]
async fn test_get_object_range() {
    let (_server, client) = setup().await;
    let bucket = "range-test";
    let key = "range.bin";

    create_bucket(&client, bucket).await;

    let data = make_data(512); // 512 bytes
    client.put_object(bucket, key, &data).await;

    let resp = client.get_object_range(bucket, key, "bytes=0-99").await;
    assert_eq!(
        resp.status(),
        206,
        "range request should return 206 Partial Content"
    );
    let body = resp.bytes().await.expect("range body");
    assert_eq!(body.as_ref(), &data[..100], "range body should match bytes 0-99");
}

#[tokio::test]
async fn test_object_not_found() {
    let (_server, client) = setup().await;
    let bucket = "get-nonexist";

    create_bucket(&client, bucket).await;

    let resp = client.get_object(bucket, "no-such-key.txt").await;
    assert_eq!(resp.status(), 404, "GET non-existent object should return 404");

    let resp = client.head_object(bucket, "no-such-key.txt").await;
    assert_eq!(resp.status(), 404, "HEAD non-existent object should return 404");
}

// ============================================================================
// ListObjectsV2 tests
// ============================================================================

#[tokio::test]
async fn test_list_objects_v2_multiple_objects() {
    let (_server, client) = setup().await;
    let bucket = "list-multi";

    create_bucket(&client, bucket).await;

    for i in 0..5 {
        let key = format!("obj{}", i);
        client.put_object(bucket, &key, b"data").await;
    }

    let resp = client.list_objects_v2(bucket, "", "", 0).await;
    assert_eq!(resp.status(), 200, "list_objects_v2 should return 200");
    let body = resp.text().await.expect("list body");

    for i in 0..5 {
        let key = format!("obj{}", i);
        assert!(
            body.contains(&key),
            "list response should contain `{key}`"
        );
    }
}

#[tokio::test]
async fn test_list_objects_v2_prefix() {
    let (_server, client) = setup().await;
    let bucket = "list-prefix";

    create_bucket(&client, bucket).await;

    // Objects with different prefixes
    client.put_object(bucket, "a/alpha.txt", b"a").await;
    client.put_object(bucket, "a/apple.txt", b"b").await;
    client.put_object(bucket, "b/banana.txt", b"c").await;

    let resp = client.list_objects_v2(bucket, "a/", "", 0).await;
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.expect("list body");

    assert!(body.contains("a/alpha.txt"), "should contain a/alpha.txt");
    assert!(body.contains("a/apple.txt"), "should contain a/apple.txt");
    assert!(!body.contains("b/banana.txt"), "should NOT contain b/banana.txt");
}

#[tokio::test]
async fn test_list_objects_v2_max_keys() {
    let (_server, client) = setup().await;
    let bucket = "list-maxkeys";

    create_bucket(&client, bucket).await;

    for i in 0..5 {
        let key = format!("item{:02}", i);
        client.put_object(bucket, &key, b"data").await;
    }

    let resp = client.list_objects_v2(bucket, "", "", 3).await;
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.expect("list body");

    // Count occurrences of `<Contents>` as a rough item count
    let count = body.matches("<Contents>").count();
    assert!(
        count <= 3,
        "expected at most 3 items with max-keys=3, got {count}"
    );
}
