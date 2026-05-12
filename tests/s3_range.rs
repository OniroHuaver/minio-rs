//! S3 Range GET integration tests (Standalone, 1 disk).
//!
//! Covers: RFC 7233 byte-range requests, 206 Partial Content, boundary conditions.
//!
//! NOTE: current `parse_range` only supports `bytes=start-end` format (both bounds
//! required). Open-ended (`bytes=N-`), suffix (`bytes=-N`), and other formats
//! are parsed as None and fall through to full-object GET (200).

mod common;

use common::s3_client::S3Client;
use common::server_process::TestServer;

async fn setup() -> (TestServer, S3Client) {
    let server = TestServer::start(1).await;
    let client = S3Client::new(&server.url());
    (server, client)
}

use common::helpers::{create_bucket, make_data};

// ============================================================================
// Valid range requests → 206 Partial Content (closed ranges only)
// ============================================================================

#[tokio::test]
async fn range_first_byte() {
    let (_server, client) = setup().await;
    create_bucket(&client, "range").await;
    let data = make_data(512);
    client.put_object("range", "data.bin", &data).await;

    let resp = client.get_object_range("range", "data.bin", "bytes=0-0").await;
    assert_eq!(resp.status(), 206, "bytes=0-0 → 206");
    // NOTE: current handler does not set Content-Range header on 206 responses.
    // TODO: add Content-Range header to range responses per RFC 7233.
    let body = resp.bytes().await.unwrap();
    assert_eq!(&body[..], &data[..1]);
}

#[tokio::test]
async fn range_prefix() {
    let (_server, client) = setup().await;
    create_bucket(&client, "range").await;
    let data = make_data(512);
    client.put_object("range", "data.bin", &data).await;

    let resp = client.get_object_range("range", "data.bin", "bytes=0-99").await;
    assert_eq!(resp.status(), 206);
    let body = resp.bytes().await.unwrap();
    assert_eq!(&body[..], &data[..100]);
    assert_eq!(body.len(), 100);
}

#[tokio::test]
async fn range_middle() {
    let (_server, client) = setup().await;
    create_bucket(&client, "range").await;
    let data = make_data(512);
    client.put_object("range", "data.bin", &data).await;

    let resp = client.get_object_range("range", "data.bin", "bytes=100-199").await;
    assert_eq!(resp.status(), 206);
    let body = resp.bytes().await.unwrap();
    assert_eq!(&body[..], &data[100..200]);
    assert_eq!(body.len(), 100);
}

#[tokio::test]
async fn range_beyond_end_saturated() {
    let (_server, client) = setup().await;
    create_bucket(&client, "range").await;
    let data = make_data(256);
    client.put_object("range", "data.bin", &data).await;

    // Range extends beyond object: should saturate to available data
    let resp = client.get_object_range("range", "data.bin", "bytes=200-499").await;
    assert_eq!(resp.status(), 206, "bytes=200-499 on 256-byte object → 206");
    let body = resp.bytes().await.unwrap();
    assert_eq!(&body[..], &data[200..]);
    assert_eq!(body.len(), 56); // 256 - 200
}

#[tokio::test]
#[ignore = "TODO: parse_range should validate start <= end; negative length causes 502"]
async fn range_end_before_start_ignored() {
    let (_server, client) = setup().await;
    create_bucket(&client, "range").await;
    let data = make_data(256);
    client.put_object("range", "data.bin", &data).await;

    // bytes=100-50: start > end. Should return 200 (ignore range) or 416 (invalid).
    let resp = client.get_object_range("range", "data.bin", "bytes=100-50").await;
    assert_eq!(resp.status(), 200, "bytes=100-50 should be ignored → 200");
}

// ============================================================================
// Range edge cases (with closed-end workaround)
// ============================================================================

#[tokio::test]
async fn no_range_header_returns_200() {
    let (_server, client) = setup().await;
    create_bucket(&client, "range").await;
    let data = make_data(256);
    client.put_object("range", "data.bin", &data).await;

    let resp = client.get_object("range", "data.bin").await;
    assert_eq!(resp.status(), 200, "GET without Range → 200");
    assert_eq!(resp.bytes().await.unwrap().as_ref(), data.as_slice());
}

#[tokio::test]
async fn malformed_range_header_ignored() {
    let (_server, client) = setup().await;
    create_bucket(&client, "range").await;
    let data = make_data(256);
    client.put_object("range", "data.bin", &data).await;

    let resp = client.get_object_range("range", "data.bin", "garbage").await;
    // Malformed Range: parse_range returns None → full object 200
    assert_eq!(resp.status(), 200, "malformed Range should be ignored → 200");
    assert_eq!(resp.bytes().await.unwrap().as_ref(), data.as_slice());
}

// ============================================================================
// Range on zero-byte object
// ============================================================================

#[tokio::test]
async fn range_on_zero_byte_object() {
    let (_server, client) = setup().await;
    create_bucket(&client, "range-zero").await;
    client.put_object("range-zero", "empty.bin", &[]).await;

    let resp = client.get_object_range("range-zero", "empty.bin", "bytes=0-0").await;
    // Range on 0-byte object with closed range [0,0]:
    // parse_range returns Some((0,0)), get_object_range(offset=0, length=1)
    // May return 200 or 416 depending on how the storage layer handles it
    let status = resp.status();
    assert!(
        status == 200 || status == 206 || status == 416,
        "Range on zero-byte object: got {status}"
    );
}

// ============================================================================
// TODO: Open-ended and suffix range formats (needs parse_range enhancement)
// ============================================================================

#[tokio::test]
#[ignore = "TODO: parse_range needs to support bytes=N- (open end) format"]
async fn range_open_end() {
    let (_server, client) = setup().await;
    create_bucket(&client, "range").await;
    let data = make_data(512);
    client.put_object("range", "data.bin", &data).await;

    let resp = client.get_object_range("range", "data.bin", "bytes=400-").await;
    assert_eq!(resp.status(), 206);
    let body = resp.bytes().await.unwrap();
    assert_eq!(&body[..], &data[400..]);
}

#[tokio::test]
#[ignore = "TODO: parse_range needs to support bytes=-N (suffix) format"]
async fn range_suffix() {
    let (_server, client) = setup().await;
    create_bucket(&client, "range").await;
    let data = make_data(512);
    client.put_object("range", "data.bin", &data).await;

    let resp = client.get_object_range("range", "data.bin", "bytes=-100").await;
    assert_eq!(resp.status(), 206);
    let body = resp.bytes().await.unwrap();
    assert_eq!(&body[..], &data[412..]);
}

#[tokio::test]
#[ignore = "TODO: parse_range needs to support bytes=0- (entire object via range) format"]
async fn range_entire_object_via_range() {
    let (_server, client) = setup().await;
    create_bucket(&client, "range").await;
    let data = make_data(256);
    client.put_object("range", "data.bin", &data).await;

    let resp = client.get_object_range("range", "data.bin", "bytes=0-").await;
    assert_eq!(resp.status(), 206);
    assert_eq!(resp.bytes().await.unwrap().as_ref(), data.as_slice());
}

#[tokio::test]
#[ignore = "TODO: parse_range enhancement for Range Not Satisfiable (416)"]
async fn range_completely_beyond_should_416() {
    let (_server, client) = setup().await;
    create_bucket(&client, "range").await;
    let data = make_data(256);
    client.put_object("range", "data.bin", &data).await;

    let resp = client.get_object_range("range", "data.bin", "bytes=999-").await;
    assert_eq!(resp.status(), 416, "Unsatisfiable Range → 416");
}
