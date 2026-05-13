//! S3 concurrent operation integration tests (Standalone, 1 disk).
//!
//! Verifies basic concurrent PUT/GET/HEAD/LIST behavior under modest parallelism.

mod common;

use common::s3_client::S3Client;
use common::server_process::TestServer;
use std::sync::Arc;

async fn setup() -> (TestServer, S3Client) {
    let server = TestServer::start(1).await;
    let client = S3Client::new(&server.url());
    (server, client)
}

use common::helpers::{create_bucket, make_data};

// ============================================================================
// Concurrent PUTs to different keys
// ============================================================================

#[tokio::test]
async fn concurrent_puts_different_keys() {
    let (_server, client) = setup().await;
    create_bucket(&client, "con-put").await;
    let client = Arc::new(client);
    let num_tasks = 16;

    let mut handles = vec![];
    for i in 0..num_tasks {
        let c = client.clone();
        let data = make_data(1024 + i * 64);
        handles.push(tokio::spawn(async move {
            let key = format!("concurrent-{:03}", i);
            let resp = c.put_object("con-put", &key, &data).await;
            (key, data, resp.status())
        }));
    }

    let mut results = vec![];
    for h in handles {
        results.push(h.await.unwrap());
    }

    for (key, _data, status) in &results {
        assert_eq!(*status, 200, "concurrent PUT {key} failed with {status}");
    }

    // Verify all objects are readable
    let c = Arc::try_unwrap(client).unwrap();
    for (key, data, _) in &results {
        let resp = c.get_object("con-put", key).await;
        assert_eq!(resp.status(), 200, "GET {key} after concurrent PUTs failed");
        let body = resp.bytes().await.unwrap();
        assert_eq!(body.as_ref(), data.as_slice(), "data mismatch for {key}");
    }
}

// ============================================================================
// Concurrent GETs on same object
// ============================================================================

#[tokio::test]
async fn concurrent_gets_same_object() {
    let (_server, client) = setup().await;
    create_bucket(&client, "con-get").await;
    let data = make_data(64 * 1024);
    client.put_object("con-get", "shared.bin", &data).await;

    let client = Arc::new(client);
    let mut handles = vec![];
    for _ in 0..16 {
        let c = client.clone();
        let expected = data.clone();
        handles.push(tokio::spawn(async move {
            let resp = c.get_object("con-get", "shared.bin").await;
            let status = resp.status();
            let body = resp.bytes().await.unwrap();
            (status, body.to_vec() == expected)
        }));
    }

    for h in handles {
        let (status, matches) = h.await.unwrap();
        assert_eq!(status, 200);
        assert!(matches, "concurrent GET returned wrong data");
    }
}

// ============================================================================
// Concurrent PUT + GET on same bucket
// ============================================================================

#[tokio::test]
async fn concurrent_put_and_get() {
    let (_server, client) = setup().await;
    create_bucket(&client, "con-mixed").await;
    let client = Arc::new(client);

    // PUT one object first for readers
    client
        .put_object("con-mixed", "reader-target.bin", &make_data(4096))
        .await;

    let mut handles = vec![];

    // Writers: PUT new objects
    for i in 0..8 {
        let c = client.clone();
        let data = make_data(2048 + i * 128);
        handles.push(tokio::spawn(async move {
            let key = format!("writer-{:02}", i);
            c.put_object("con-mixed", &key, &data).await.status()
        }));
    }

    // Readers: GET the pre-existing object
    for _ in 0..8 {
        let c = client.clone();
        handles.push(tokio::spawn(async move {
            c.get_object("con-mixed", "reader-target.bin")
                .await
                .status()
        }));
    }

    for h in handles {
        let status = h.await.unwrap();
        assert_eq!(status, 200, "concurrent mixed op failed");
    }
}

// ============================================================================
// Concurrent HEAD operations
// ============================================================================

#[tokio::test]
async fn concurrent_heads() {
    let (_server, client) = setup().await;
    create_bucket(&client, "con-head").await;
    client
        .put_object("con-head", "head-target.txt", b"head me")
        .await;

    let client = Arc::new(client);
    let mut handles = vec![];
    for _ in 0..20 {
        let c = client.clone();
        handles.push(tokio::spawn(async move {
            let resp = c.head_object("con-head", "head-target.txt").await;
            resp.status()
        }));
    }

    for h in handles {
        assert_eq!(h.await.unwrap(), 200);
    }
}

// ============================================================================
// Concurrent LIST + PUT
// ============================================================================

#[tokio::test]
async fn concurrent_list_and_put() {
    let (_server, client) = setup().await;
    create_bucket(&client, "con-list").await;
    let client = Arc::new(client);

    let mut handles = vec![];

    // PUT some objects concurrently
    for i in 0..10 {
        let c = client.clone();
        handles.push(tokio::spawn(async move {
            let key = format!("item-{:02}", i);
            c.put_object("con-list", &key, b"data").await.status()
        }));
    }

    // LIST concurrently
    for _ in 0..4 {
        let c = client.clone();
        handles.push(tokio::spawn(async move {
            c.list_objects_v2("con-list", "", "", 0).await.status()
        }));
    }

    for h in handles {
        let status = h.await.unwrap();
        assert!(status == 200, "concurrent LIST+PUT failed with {status}");
    }

    // Final verification: all items should be present
    let resp = client.list_objects_v2("con-list", "", "", 0).await;
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    for i in 0..10 {
        assert!(
            body.contains(&format!("item-{:02}", i)),
            "final list should contain all items"
        );
    }
}

// ============================================================================
// Same-key concurrent PUT (last writer wins)
// ============================================================================

#[tokio::test]
async fn concurrent_put_same_key_last_writer_wins() {
    let (_server, client) = setup().await;
    create_bucket(&client, "con-same").await;
    let client = Arc::new(client);

    let mut handles = vec![];
    for i in 0..10 {
        let c = client.clone();
        let data = format!("writer {:02} content", i).into_bytes();
        handles.push(tokio::spawn(async move {
            (
                c.put_object("con-same", "shared.txt", &data).await.status(),
                data,
            )
        }));
    }

    let mut all_data = vec![];
    for h in handles {
        let (status, data) = h.await.unwrap();
        assert_eq!(status, 200);
        all_data.push(data);
    }

    // After all writes, the object should exist and contain one of the versions
    let resp = client.get_object("con-same", "shared.txt").await;
    assert_eq!(resp.status(), 200);
    let body = resp.bytes().await.unwrap();
    let body_str = String::from_utf8_lossy(&body);
    assert!(
        all_data.iter().any(|d| d.as_slice() == body.as_ref()),
        "final data should match one of the writers: {body_str}"
    );
}
