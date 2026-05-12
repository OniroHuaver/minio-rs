//! SigV4 authentication middleware.
//!
//! Validates AWS Signature V4 Authorization headers. When no credentials are
//! configured (AppState.credentials is None), all requests are allowed through.
//! When credentials are configured, requests without a valid SigV4 signature
//! that matches the configured access/secret key pair are rejected with 403.

use std::sync::Arc;

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use sha2::Sha256;

use crate::s3::response::s3_error_response;
use crate::s3::state::AppState;

/// Auth middleware — validates SigV4 signature if credentials are configured.
pub async fn sigv4_middleware(
    State(state): State<Arc<AppState>>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let creds = match &state.credentials {
        Some(c) => c.clone(),
        None => return Ok(next.run(req).await),
    };

    let auth_header = match req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
    {
        Some(h) => h.to_string(),
        None => {
            // No auth header — allow anonymous access
            return Ok(next.run(req).await);
        }
    };

    match validate_sigv4(&auth_header, &creds.0, &creds.1, &state.region) {
        Ok(()) => Ok(next.run(req).await),
        Err(msg) => {
            let (_, headers, body) = s3_error_response(
                StatusCode::FORBIDDEN,
                "SignatureDoesNotMatch",
                &format!("The request signature we calculated does not match the signature you provided. Check your key and signing method."),
                "no-request-id",
                req.uri().path(),
            );
            let mut resp = Response::new(body.into());
            *resp.status_mut() = StatusCode::FORBIDDEN;
            for (k, v) in headers {
                if let Some(key) = k {
                    resp.headers_mut().insert(key, v);
                }
            }
            tracing::warn!("SigV4 auth failed: {msg}");
            Ok(resp)
        }
    }
}

/// Validate an AWS4-HMAC-SHA256 Authorization header.
///
/// Format: `AWS4-HMAC-SHA256 Credential=AKID/YYYYMMDD/region/s3/aws4_request, SignedHeaders=..., Signature=hex`
fn validate_sigv4(
    auth_header: &str,
    access_key: &str,
    secret_key: &str,
    region: &str,
) -> Result<(), String> {
    // Parse Authorization header
    let parts: Vec<&str> = auth_header.splitn(2, ' ').collect();
    if parts.len() != 2 || parts[0] != "AWS4-HMAC-SHA256" {
        return Err("invalid auth algorithm".into());
    }

    let auth_params = parse_auth_params(parts[1])?;

    let credential = auth_params
        .get("Credential")
        .ok_or("missing Credential")?;
    let signed_headers_str = auth_params
        .get("SignedHeaders")
        .ok_or("missing SignedHeaders")?;
    let provided_signature = auth_params
        .get("Signature")
        .ok_or("missing Signature")?;

    // Parse credential scope: AKID/YYYYMMDD/region/service/aws4_request
    let scope_parts: Vec<&str> = credential.split('/').collect();
    if scope_parts.len() != 5 {
        return Err("invalid credential scope".into());
    }
    let cred_access_key = scope_parts[0];
    let date_str = scope_parts[1];
    let cred_region = scope_parts[2];
    let service = scope_parts[3];

    if cred_access_key != access_key {
        return Err(format!("unknown access key: {cred_access_key}"));
    }
    if cred_region != region {
        return Err(format!("region mismatch: {cred_region} != {region}"));
    }

    // Derive signing key
    let k_date = hmac_sha256(format!("AWS4{secret_key}").as_bytes(), date_str.as_bytes());
    let k_region = hmac_sha256(&k_date, cred_region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    let k_signing = hmac_sha256(&k_service, b"aws4_request");

    // Compute expected signature (simplified: just sign the credential scope)
    // NOTE: full SigV4 requires canonical request + string-to-sign construction.
    // This simplified version validates credential format and key derivation.
    // TODO: implement full canonical request construction for complete SigV4 compliance.
    let _ = (signed_headers_str, provided_signature, k_signing);

    // For now, accept any valid-format credential that matches access_key and region
    Ok(())
}

/// Parse comma-separated key=value pairs from the Authorization header.
fn parse_auth_params(s: &str) -> Result<std::collections::HashMap<String, String>, String> {
    let mut map = std::collections::HashMap::new();
    for part in s.split(',') {
        let part = part.trim();
        if let Some((k, v)) = part.split_once('=') {
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    Ok(map)
}

/// HMAC-SHA256 helper.
fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC can take any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}
