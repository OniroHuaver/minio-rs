//! S3 large-object integration tests (Standalone, 1 disk).
//!
//! Verifies data integrity for objects above the xl.meta inline threshold
//! (128 KiB) and up to multiple hundreds of MiB.
//!
//! These tests produce real disk I/O — the heavyweight ones (≥256 MiB) are
//! marked `#[ignore]` and should be run explicitly when validating large-file
//! correctness.

mod common;

use common::s3_client::S3Client;
use common::server_process::TestServer;

async fn setup() -> (TestServer, S3Client) {
    let server = TestServer::start(1).await;
    let client = S3Client::new(&server.url());
    (server, client)
}

use common::helpers::{create_bucket, make_data};

/// Verify a roundtrip: PUT data, GET back, compare byte-for-byte.
async fn roundtrip(client: &S3Client, bucket: &str, key: &str, data: &[u8]) {
    let put = client.put_object(bucket, key, data).await;
    assert_eq!(put.status(), 200, "PUT {key} ({:.1} MiB) failed", data.len() as f64 / 1_048_576.0);

    let etag_put = put.headers().get("etag").and_then(|v| v.to_str().ok());
    assert!(etag_put.is_some(), "PUT should return ETag");

    let get = client.get_object(bucket, key).await;
    assert_eq!(get.status(), 200, "GET {key} failed");
    assert_eq!(get.bytes().await.unwrap().as_ref(), data, "data mismatch for {key}");

    let head = client.head_object(bucket, key).await;
    assert_eq!(head.status(), 200, "HEAD {key} failed");
    assert_eq!(
        head.headers().get("etag").and_then(|v| v.to_str().ok()),
        etag_put,
        "HEAD ETag must match PUT ETag"
    );
}

// ============================================================================
// 10 MiB — basic shard-file path verification
// ============================================================================

#[tokio::test]
async fn large_10mib_roundtrip() {
    let (_server, client) = setup().await;
    create_bucket(&client, "large-10m").await;
    let data = make_data(10 * 1024 * 1024);
    roundtrip(&client, "large-10m", "10mib.bin", &data).await;
}

// ============================================================================
// 64 MiB — moderate stress
// ============================================================================

#[tokio::test]
async fn large_64mib_roundtrip() {
    let (_server, client) = setup().await;
    create_bucket(&client, "large-64m").await;
    let data = make_data(64 * 1024 * 1024);
    roundtrip(&client, "large-64m", "64mib.bin", &data).await;
}

// ============================================================================
// 128 MiB — boundary per user request
// ============================================================================

#[tokio::test]
async fn large_128mib_roundtrip() {
    let (_server, client) = setup().await;
    create_bucket(&client, "large-128m").await;
    let data = make_data(128 * 1024 * 1024);
    roundtrip(&client, "large-128m", "128mib.bin", &data).await;
}

// ============================================================================
// 256 MiB — heavy; ignored by default
// ============================================================================

#[tokio::test]
#[ignore = "heavy: 256 MiB roundtrip, run with `cargo test -- --ignored`"]
async fn large_256mib_roundtrip() {
    let (_server, client) = setup().await;
    create_bucket(&client, "large-256m").await;
    let data = make_data(256 * 1024 * 1024);
    roundtrip(&client, "large-256m", "256mib.bin", &data).await;
}

// ============================================================================
// Multiple large objects in same bucket — stress listing
// ============================================================================

#[tokio::test]
async fn large_multiple_objects() {
    let (_server, client) = setup().await;
    create_bucket(&client, "large-multi").await;

    // 3 objects × 10 MiB
    let sizes = [10_485_760, 10_485_761, 10_485_762]; // not exact MiB to test odd sizes
    let keys = ["a.bin", "b.bin", "c.bin"];

    for i in 0..3 {
        let data = make_data(sizes[i]);
        let put = client.put_object("large-multi", keys[i], &data).await;
        assert_eq!(put.status(), 200, "PUT {key} ({size} bytes) failed", key = keys[i], size = sizes[i]);

        let get = client.get_object("large-multi", keys[i]).await;
        assert_eq!(get.status(), 200);
        assert_eq!(get.bytes().await.unwrap().as_ref(), data.as_slice(), "data mismatch for {}", keys[i]);
    }

    // Verify all listed correctly
    let resp = client.list_objects_v2("large-multi", "", "", 0).await;
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    for key in &keys {
        assert!(body.contains(key), "list should contain {key}");
    }
    assert!(body.contains("<KeyCount>3</KeyCount>"), "KeyCount should be 3");
}

// ============================================================================
// Large object with Content-MD5
// ============================================================================

#[tokio::test]
async fn large_10mib_with_md5() {
    let (_server, client) = setup().await;
    create_bucket(&client, "large-md5").await;

    let data = make_data(10 * 1024 * 1024);
    use base64::Engine;
    use md5::{Digest, Md5};
    let mut hasher = Md5::new();
    hasher.update(&data);
    let md5 = base64::engine::general_purpose::STANDARD.encode(&hasher.finalize());

    let resp = client.put_object_with_md5("large-md5", "md5.bin", &data, &md5).await;
    assert_eq!(resp.status(), 200, "10 MiB PUT with valid MD5 → 200");

    // Verify data integrity
    let resp = client.get_object("large-md5", "md5.bin").await;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.bytes().await.unwrap().as_ref(), data.as_slice());
}

// ============================================================================
// Large object overwrite
// ============================================================================

#[tokio::test]
async fn large_overwrite_different_size() {
    let (_server, client) = setup().await;
    create_bucket(&client, "large-ow").await;

    let v1 = make_data(5 * 1024 * 1024); // 5 MiB
    let v2 = make_data(15 * 1024 * 1024); // 15 MiB — much larger overwrite

    let r1 = client.put_object("large-ow", "growing.bin", &v1).await;
    assert_eq!(r1.status(), 200);
    let etag1 = r1.headers().get("etag").and_then(|v| v.to_str().ok());

    let r2 = client.put_object("large-ow", "growing.bin", &v2).await;
    assert_eq!(r2.status(), 200);
    let etag2 = r2.headers().get("etag").and_then(|v| v.to_str().ok());
    assert_ne!(etag1, etag2, "overwrite with different data → different ETag");

    let get = client.get_object("large-ow", "growing.bin").await;
    assert_eq!(get.status(), 200);
    assert_eq!(get.bytes().await.unwrap().as_ref(), v2.as_slice(), "overwrite data mismatch");
}

// ============================================================================
// Large object Range GET
// ============================================================================

#[tokio::test]
async fn large_range_across_shard_boundaries() {
    let (_server, client) = setup().await;
    create_bucket(&client, "large-range").await;

    let data = make_data(10 * 1024 * 1024); // 10 MiB
    client.put_object("large-range", "big.bin", &data).await;

    // Range spanning multiple internal shards (128 KiB each)
    let offset = 64 * 1024; // 64 KiB in
    let end = offset + 512 * 1024 - 1; // span 512 KiB across ~4 shards
    let range = format!("bytes={offset}-{end}");

    let resp = client.get_object_range("large-range", "big.bin", &range).await;
    assert_eq!(resp.status(), 206, "cross-shard range → 206");
    let body = resp.bytes().await.unwrap();
    let expected = &data[offset as usize..=end as usize];
    assert_eq!(body.as_ref(), expected, "cross-shard range data mismatch");
    assert_eq!(body.len(), (end - offset + 1) as usize);
}
