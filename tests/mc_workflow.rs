//! mc (MinIO Client) workflow integration tests.
//!
//! Simulates the standard `mc` CLI workflow against a standalone (single-disk)
//! minio-rs server:
//!
//! ```bash
//! mc alias set local http://localhost:9000 minioadmin minioadmin
//! mc mb local/testbucket
//! mc cp hello.txt local/testbucket/
//! mc cat local/testbucket/hello.txt
//! mc rm local/testbucket/hello.txt
//! ```

mod common;

use common::mc_client::McClient;
use common::server_process::TestServer;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn setup_mc() -> (TestServer, McClient) {
    let server = TestServer::start(1).await;
    let mc = McClient::new(&server.url()).alias_set("local", "minioadmin", "minioadmin");
    (server, mc)
}

// ============================================================================
// mc alias set
// ============================================================================

#[tokio::test]
async fn mc_alias_set_is_noop() {
    let (_server, mc) = setup_mc().await;

    // alias_set in Phase 1 is a no-op (no auth enforcement).
    // Verify that subsequent operations work without explicit credentials.
    let resp = mc.mb("alias-test").await;
    assert_eq!(resp.status(), 200, "mb after alias_set should work");
}

// ============================================================================
// mc mb (make bucket)
// ============================================================================

#[tokio::test]
async fn mc_mb_create_bucket() {
    let (_server, mc) = setup_mc().await;

    let resp = mc.mb("mybucket").await;
    assert_eq!(resp.status(), 200, "mc mb → 200");

    // Verify bucket exists (implicitly via listing)
    let resp = mc.ls("mybucket").await;
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn mc_mb_duplicate() {
    let (_server, mc) = setup_mc().await;

    mc.mb("dup-bucket").await;
    let resp = mc.mb("dup-bucket").await;
    assert_eq!(resp.status(), 409, "mc mb duplicate → 409 Conflict");
}

// ============================================================================
// mc cp (copy file → put object)
// ============================================================================

#[tokio::test]
async fn mc_cp_put_object() {
    let (_server, mc) = setup_mc().await;
    mc.mb("files").await;

    let content = b"Hello, minio-rs!\nThis is a test file.\n";
    let resp = mc.cp("files", "hello.txt", content).await;
    assert_eq!(resp.status(), 200, "mc cp → 200");

    // Verify Content-MD5/ETag
    let etag = resp.headers().get("etag").and_then(|v| v.to_str().ok());
    assert!(etag.is_some(), "mc cp should return ETag");
}

// ============================================================================
// mc cat (read object → stdout)
// ============================================================================

#[tokio::test]
async fn mc_cat_read_object() {
    let (_server, mc) = setup_mc().await;
    mc.mb("read-bucket").await;

    let content = b"The quick brown fox jumps over the lazy dog.\n";
    mc.cp("read-bucket", "fox.txt", content).await;

    let resp = mc.cat("read-bucket", "fox.txt").await;
    assert_eq!(resp.status(), 200, "mc cat → 200");

    let body = resp.bytes().await.expect("body");
    assert_eq!(body.as_ref(), content, "mc cat output must match mc cp input");
}

#[tokio::test]
async fn mc_cat_nonexistent_file() {
    let (_server, mc) = setup_mc().await;
    mc.mb("cat-miss").await;

    let resp = mc.cat("cat-miss", "nope.txt").await;
    assert_eq!(resp.status(), 404, "mc cat non-existent → 404");
}

// ============================================================================
// mc rm (remove object)
// ============================================================================
#[tokio::test]
async fn mc_rm_delete_object() {
    let (_server, mc) = setup_mc().await;
    mc.mb("rm-bucket").await;

    mc.cp("rm-bucket", "garbage.txt", b"delete me").await;

    let resp = mc.rm("rm-bucket", "garbage.txt").await;
    assert_eq!(resp.status(), 204, "mc rm → 204 No Content");

    // Verify it's really gone
    let resp = mc.cat("rm-bucket", "garbage.txt").await;
    assert_eq!(resp.status(), 404, "mc cat after mc rm → 404");
}

#[tokio::test]
async fn mc_rm_nonexistent() {
    let (_server, mc) = setup_mc().await;
    mc.mb("rm-ghost").await;

    let resp = mc.rm("rm-ghost", "phantom.txt").await;
    assert_eq!(resp.status(), 204, "mc rm non-existent → 204 (idempotent)");
}

// ============================================================================
// mc ls (list objects)
// ============================================================================

#[tokio::test]
async fn mc_ls_list_objects() {
    let (_server, mc) = setup_mc().await;
    mc.mb("ls-bucket").await;

    mc.cp("ls-bucket", "alpha.txt", b"A").await;
    mc.cp("ls-bucket", "bravo.txt", b"B").await;
    mc.cp("ls-bucket", "charlie.txt", b"C").await;

    let resp = mc.ls("ls-bucket").await;
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.expect("body");

    assert!(body.contains("alpha.txt"), "ls should show alpha.txt");
    assert!(body.contains("bravo.txt"), "ls should show bravo.txt");
    assert!(body.contains("charlie.txt"), "ls should show charlie.txt");
}

// ============================================================================
// Full mc workflow (end-to-end)
// ============================================================================

#[tokio::test]
async fn mc_full_workflow() {
    let (_server, mc) = setup_mc().await;

    // Step 1: mc alias set local http://<addr> minioadmin minioadmin
    // (already done in setup_mc)

    // Step 2: mc mb local/testbucket
    let resp = mc.mb("testbucket").await;
    assert_eq!(resp.status(), 200, "mc mb → 200");

    // Step 3: mc cp hello.txt local/testbucket/
    let hello_content = b"hello from mc integration test\n";
    let resp = mc.cp("testbucket", "hello.txt", hello_content).await;
    assert_eq!(resp.status(), 200, "mc cp → 200");
    let put_etag = resp
        .headers()
        .get("etag")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // Step 4: mc cat local/testbucket/hello.txt
    let resp = mc.cat("testbucket", "hello.txt").await;
    assert_eq!(resp.status(), 200, "mc cat → 200");
    // Capture ETag BEFORE consuming body (bytes() moves resp)
    let cat_etag = resp
        .headers()
        .get("etag")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let body = resp.bytes().await.expect("body");
    assert_eq!(
        body.as_ref(),
        hello_content,
        "cat output must match cp input"
    );

    // Step 5: Verify ETag consistency
    assert_eq!(cat_etag.as_deref(), put_etag.as_deref(), "ETag on cat must match ETag on cp");

    // Step 6: mc rm local/testbucket/hello.txt
    let resp = mc.rm("testbucket", "hello.txt").await;
    assert_eq!(resp.status(), 204, "mc rm → 204");

    // Step 7: Verify rm — cat should return 404
    let resp = mc.cat("testbucket", "hello.txt").await;
    assert_eq!(resp.status(), 404, "mc cat after mc rm → 404");

    // Step 8: List should be empty (or have key count 0)
    let resp = mc.ls("testbucket").await;
    assert_eq!(resp.status(), 200);
    let list_body = resp.text().await.expect("body");
    assert!(
        list_body.contains("<KeyCount>0</KeyCount>") ||
        list_body.matches("<Key>").count() == 0,
        "listing after rm should be empty or KeyCount=0"
    );
}

// ============================================================================
// mc workflow with multiple files
// ============================================================================

#[tokio::test]
async fn mc_workflow_multiple_files() {
    let (_server, mc) = setup_mc().await;
    mc.mb("multi-bucket").await;

    let files: Vec<(&str, &[u8])> = vec![
        ("config.toml", b"[server]\nport = 9000\n"),
        ("readme.md", b"# Project\n\nHello world.\n"),
        ("data.csv", b"name,value\nalpha,1\nbravo,2\n"),
    ];

    // Upload all
    for (name, content) in &files {
        let resp = mc.cp("multi-bucket", name, content).await;
        assert_eq!(resp.status(), 200, "mc cp {name} → 200");
    }

    // Read back each
    for (name, content) in &files {
        let resp = mc.cat("multi-bucket", name).await;
        assert_eq!(resp.status(), 200, "mc cat {name} → 200");
        let body = resp.bytes().await.expect("body");
        assert_eq!(body.as_ref(), *content, "content mismatch for {name}");
    }

    // List should show all
    let resp = mc.ls("multi-bucket").await;
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.expect("body");
    for (name, _) in &files {
        assert!(body.contains(name), "ls should contain {name}");
    }

    // Delete one
    let resp = mc.rm("multi-bucket", "data.csv").await;
    assert_eq!(resp.status(), 204);

    // Verify deletion
    let resp = mc.cat("multi-bucket", "data.csv").await;
    assert_eq!(resp.status(), 404);
}

// ============================================================================
// mc workflow with directories (nested keys)
// ============================================================================

#[tokio::test]
async fn mc_workflow_nested_keys() {
    let (_server, mc) = setup_mc().await;
    mc.mb("nested").await;

    // Create objects with "/" in key
    mc.cp("nested", "photos/2024/sunset.jpg", b"JPEG_DATA").await;
    mc.cp("nested", "photos/2024/sunrise.jpg", b"JPEG_DATA").await;
    mc.cp("nested", "docs/readme.txt", b"README").await;

    // Read back a nested key
    let resp = mc.cat("nested", "photos/2024/sunset.jpg").await;
    assert_eq!(resp.status(), 200, "cat on nested key → 200");
    let body = resp.bytes().await.expect("body");
    assert_eq!(body.as_ref(), b"JPEG_DATA");

    // List with prefix
    let resp = mc.cat("nested", "docs/readme.txt").await;
    assert_eq!(resp.status(), 200);
    let body = resp.bytes().await.expect("body");
    assert_eq!(body.as_ref(), b"README");

    // Delete a nested key
    let resp = mc.rm("nested", "photos/2024/sunrise.jpg").await;
    assert_eq!(resp.status(), 204);

    // Verify deletion
    let resp = mc.cat("nested", "photos/2024/sunrise.jpg").await;
    assert_eq!(resp.status(), 404, "deleted nested key → 404");

    // sunset.jpg should still exist
    let resp = mc.cat("nested", "photos/2024/sunset.jpg").await;
    assert_eq!(resp.status(), 200);
}
