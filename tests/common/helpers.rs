#![allow(dead_code)]

//! Shared test helpers reused across integration test files.

use super::s3_client::S3Client;

/// Generate `len` bytes of deterministic repeating data (0..=255).
pub fn make_data(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 256) as u8).collect()
}

/// Create a bucket and assert 200 OK.
pub async fn create_bucket(client: &S3Client, name: &str) {
    let resp = client.create_bucket(name).await;
    assert_eq!(
        resp.status(),
        200,
        "create_bucket({name}) failed with status {}",
        resp.status()
    );
}
