//! KMS tests: KMS handlers, DEK encode/decode, SecretKey encrypt/decrypt

// ---- kms-handlers ----

/// Verifies KMS CreateKey handler.
///
/// Covers: no policy (deny), allow policy (success), resource mismatch (deny).
#[test]
#[ignore]
fn test_kms_handlers_create_key() {
    // 4 cases: no policy->403; no resource restriction->200; resource match->200; resource mismatch->403
    // TODO: implement when KMS handler subsystem is available
}

/// Verifies KMS KeyStatus handler.
#[test]
#[ignore]
fn test_kms_handlers_key_status() {
    // 7 cases: root, no policy, no resource restriction, resource match, resource mismatch
    // TODO: implement when KMS handler subsystem is available
}

/// Verifies KMS APIs/Version/Metrics/Status handlers.
#[test]
#[ignore]
fn test_kms_handlers_apis() {
    // ~12 cases covering Version/APIs/Metrics/Status for root, no policy, with policy
    // TODO: implement when KMS handler subsystem is available
}

/// Verifies KMS ListKeys handler.
#[test]
#[ignore]
fn test_kms_handlers_list_keys() {
    // ~8 cases covering pattern filter, resource restriction, Deny policy
    // TODO: implement when KMS handler subsystem is available
}

/// Verifies KMS Admin API handler.
#[test]
#[ignore]
fn test_kms_handler_admin_api() {
    // ~9 cases covering Admin KMS API: CreateKey/Status/KeyStatus
    //   Admin actions ignore Resources
    // TODO: implement when KMS handler subsystem is available
}

/// Verifies KMS handler behavior when not configured or with invalid credentials.
#[test]
#[ignore]
fn test_kms_handler_not_configured_or_invalid_creds() {
    // KMS not configured -> 501 Not Implemented
    //   KMS configured but invalid credentials -> 403 Forbidden
    // TODO: implement when KMS handler subsystem is available
}

// ---- internal/kms/config ----

/// Verifies KMS config presence check `IsPresent()`.
#[test]
#[ignore]
fn test_kms_is_present() {
    // GlobalKMS != nil -> true; nil -> false
    // TODO: implement when KMS config is available
}

// ---- internal/kms/dek ----

/// Verifies DEK encode/decode round-trip.
#[test]
#[ignore]
fn test_encode_decode_dek() {
    // DEK{Version, Key, SealedKey} -> Encode -> Decode -> equal
    // TODO: implement when DEK type is available
}

// ---- internal/kms/secret-key ----

/// Verifies single key encrypt/decrypt round-trip.
#[test]
#[ignore]
fn test_single_key_roundtrip() {
    // SecretKey -> Encrypt -> Decrypt -> original
    // TODO: implement when SecretKey KMS is available
}

/// Verifies key decryption `DecryptKey()`.
#[test]
#[ignore]
fn test_decrypt_key() {
    // multi-key decryption attempt
    // TODO: implement when SecretKey KMS is available
}
