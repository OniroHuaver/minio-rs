//! 缓存值测试
//!
//! 对应 Go: internal/cachevalue/cache_test.go
//!
//! 测试 Cache (带过期时间的惰性求值缓存) 功能。

/// 测试 Cache.GetWithCtx (context cancel 传播)
///
/// Go: TestCacheCtx
#[test]
#[ignore]
fn test_cache_get_with_ctx() {
    // TODO: implement when Cache type available
    //
    // Go 逻辑:
    //   1. New[time.Time](), InitOnce(2s, ..., slowCaller)
    //   2. 已 cancel 的 ctx → 立即返回 context.Canceled
    //   3. 有效 ctx → t1, t2 在 2s 内应相等
    //   4. sleep 3s (超 2s TTL) → t3 与 t1 不等
}

/// 测试 Cache.Get (简化版, 无 context)
///
/// Go: TestCache
#[test]
#[ignore]
fn test_cache_get() {
    // TODO: implement when Cache available
    //
    // Go 逻辑:
    //   1. 2s TTL
    //   2. t1, t2 应相等
    //   3. sleep 3s → t3 与 t1 不等
}
