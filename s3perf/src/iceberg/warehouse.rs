//! Iceberg Warehouse 管理 (MinIO AIStor Tables 专有 API)。

use crate::iceberg::CatalogConfig;
use sha2::{Digest, Sha256};

pub async fn ensure_warehouse(cfg: &CatalogConfig) -> Result<(), String> {
    let url = format!(
        "{}://{}/_iceberg/v1/warehouses",
        if cfg.tls { "https" } else { "http" },
        cfg.catalog_uri
            .trim_start_matches("http://")
            .trim_start_matches("https://")
    );

    let body = serde_json::json!({ "name": cfg.warehouse }).to_string();
    let body_bytes = body.as_bytes().to_vec();

    let client = reqwest::Client::new();
    let req = client
        .post(&url)
        .header("Content-Type", "application/json")
        .body(body_bytes);

    let req = sign_request(req, &cfg.access_key, &cfg.secret_key, &cfg.region, &body).await?;

    let resp = req
        .send()
        .await
        .map_err(|e| format!("warehouse HTTP request failed: {e}"))?;
    let status = resp.status();
    if status.is_success() || status.as_u16() == 409 {
        return Ok(());
    }
    let resp_body = resp.text().await.unwrap_or_default();
    Err(format!("warehouse create failed: {status} - {resp_body}"))
}

pub async fn delete_warehouse(cfg: &CatalogConfig) -> Result<(), String> {
    let url = format!(
        "{}://{}/_iceberg/v1/warehouses/{}",
        if cfg.tls { "https" } else { "http" },
        cfg.catalog_uri
            .trim_start_matches("http://")
            .trim_start_matches("https://"),
        cfg.warehouse
    );

    let client = reqwest::Client::new();
    let req = client
        .delete(&url)
        .header("Content-Type", "application/json");

    let req = sign_request(req, &cfg.access_key, &cfg.secret_key, &cfg.region, "").await?;

    let resp = req
        .send()
        .await
        .map_err(|e| format!("warehouse delete request failed: {e}"))?;
    let status = resp.status();
    if status.is_success() || status.as_u16() == 404 {
        return Ok(());
    }
    let resp_body = resp.text().await.unwrap_or_default();
    Err(format!("warehouse delete failed: {status} - {resp_body}"))
}

/// SigV4 签名 — 返回已签名的 RequestBuilder
async fn sign_request(
    req: reqwest::RequestBuilder,
    access_key: &str,
    secret_key: &str,
    region: &str,
    body: &str,
) -> Result<reqwest::RequestBuilder, String> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let t = chrono::DateTime::from_timestamp(now.as_secs() as i64, 0)
        .unwrap_or_else(|| chrono::Utc::now());

    let amz_date = t.format("%Y%m%dT%H%M%SZ").to_string();
    let date_stamp = t.format("%Y%m%d").to_string();
    let payload_hash = hex::encode(Sha256::digest(body.as_bytes()));

    let content_type = "application/json";
    let host = "localhost"; // simplified

    let signed_headers = "content-type;host;x-amz-content-sha256;x-amz-date";
    let canonical_uri = "/_iceberg/v1/warehouses"; // simplified
    let canonical_headers = format!(
        "content-type:{content_type}\nhost:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n"
    );
    let canonical_request = format!(
        "POST\n{canonical_uri}\n\n{canonical_headers}\n{signed_headers}\n{payload_hash}"
    );

    let service = "s3tables";
    let credential_scope = format!("{date_stamp}/{region}/{service}/aws4_request");
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{}",
        hex::encode(Sha256::digest(canonical_request.as_bytes()))
    );

    let k_date = hmac_sha256(format!("AWS4{secret_key}").as_bytes(), date_stamp.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    let k_signing = hmac_sha256(&k_service, b"aws4_request");
    let signature = hex::encode(hmac_sha256(&k_signing, string_to_sign.as_bytes()));

    let auth_header = format!(
        "AWS4-HMAC-SHA256 Credential={access_key}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}"
    );

    let req = req
        .header("x-amz-date", &amz_date)
        .header("x-amz-content-sha256", &payload_hash)
        .header("authorization", &auth_header);

    Ok(req)
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC key");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}
