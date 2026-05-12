//! Credentials tests: access/secret key validation, creation, comparison, expiration

/// Verifies `ExpToInt64()` converts expiration time to int64 epoch.
#[test]
#[ignore]
fn test_exp_to_int64() {
    // time.Time -> int64 epoch
    // TODO: implement when credentials expiration is available
}

/// Verifies `IsAccessKeyValid()` access key validation.
#[test]
#[ignore]
fn test_is_access_key_valid() {
    // empty/length/charset checks
    // TODO: implement when credentials validation is available
}

/// Verifies `IsSecretKeyValid()` secret key validation.
#[test]
#[ignore]
fn test_is_secret_key_valid() {
    // empty/length checks
    // TODO: implement when credentials validation is available
}

/// Verifies `GetNewCredentials()` generates new credentials.
#[test]
#[ignore]
fn test_get_new_credentials() {
    // random access/secret key generation
    // TODO: implement when credentials generation is available
}

/// Verifies `CreateCredentials()` with specified access/secret key.
#[test]
#[ignore]
fn test_create_credentials() {
    // create credentials with given access/secret key
    // TODO: implement when credentials creation is available
}

/// Verifies `Credentials.Equal()` field-by-field comparison.
#[test]
#[ignore]
fn test_credentials_equal() {
    // field-by-field comparison
    // TODO: implement when Credentials type is available
}
