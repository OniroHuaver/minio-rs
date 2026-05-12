//! JWT tests: token generation, parsing, authentication, standard/map claims

/// Verifies `metricsRequestAuthenticate()` for web request authentication.
///
/// Covers: valid token, missing Authorization header, invalid token.
#[test]
#[ignore]
fn test_web_request_authenticate() {
    // setup FS + config -> generate token
    //   valid token -> nil; no header -> errNoAuthToken; invalid -> errAuthentication
    // TODO: implement when metricsRequestAuthenticate equivalent is available
}

/// Verifies JWT standard claims parsing (Benchmark wrapper).
#[test]
#[ignore]
fn test_parse_jwt_standard_claims() {
    // authenticateNode -> ParseWithStandardClaims
    // TODO: implement when JWT subsystem is available
}

/// Verifies JWT Map claims parsing (Benchmark wrapper).
#[test]
#[ignore]
fn test_parse_jwt_map_claims() {
    // authenticateNode -> ParseWithClaims
    // TODO: implement when JWT subsystem is available
}

/// Verifies authenticateNode benchmark (wrapper).
#[test]
#[ignore]
fn test_authenticate_node_benchmark() {
    // auth token generation (cached + uncached)
    // TODO: implement when JWT subsystem is available
}

// ---- internal/jwt/parser ----

/// Verifies JWT parser parsing.
#[test]
#[ignore]
fn test_parser_parse() {
    // jwt.Parser.Parse() various token scenarios
    // TODO: implement when JWT parser is available
}
