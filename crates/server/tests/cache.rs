//! Cache value tests
//!
//! Tests Cache (lazy evaluation with expiry) functionality.

/// Test Cache::GetWithCtx (context cancel propagation)
#[test]
#[ignore]
fn test_cache_get_with_ctx() {
    // TODO: implement when Cache type available
    //
    // Steps:
    //   1. New[time::Time](), InitOnce(2s, ..., slowCaller)
    //   2. Cancelled ctx -> immediately returns context::Canceled
    //   3. Valid ctx -> t1, t2 should be equal within 2s
    //   4. sleep 3s (past 2s TTL) -> t3 differs from t1
}

/// Test Cache::Get (simplified, no context)
#[test]
#[ignore]
fn test_cache_get() {
    // TODO: implement when Cache available
    //
    // Steps:
    //   1. 2s TTL
    //   2. t1, t2 should be equal
    //   3. sleep 3s -> t3 differs from t1
}
