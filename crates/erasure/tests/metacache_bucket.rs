//! Bucket-level metacache tests.
//!
//! 对应 Go: `cmd/metacache-bucket_test.go`
//!
//! 测试 bucketMetacache 的 findCache 方法性能。

use minio_erasure::*;

/// 基准测试: bucketMetacache.findCache 性能。
///
/// Go 源: `Benchmark_bucketMetacache_findCache`
///
/// 预热 50000 个缓存条目 (100 个不同路径，各 500 次)，
/// 然后测量 findCache 的性能。
#[test]
#[ignore]
fn benchmark_bucket_metacache_find_cache() {
    // TODO: implement when bucketMetacache::find_cache with proper caching is available
    /*
    let mut bm = BucketMetacache::new("", false);
    const ELEMENTS: usize = 50000;
    const PATHS: usize = 100;

    // Pre-populate cache
    for i in 0..ELEMENTS {
        let path = format!("prefix/{}", i % PATHS);
        bm.find_cache(ListPathOptions {
            id: Uuid::new_v4(),
            base_dir: path,
            create: true,
            ..Default::default()
        });
    }

    // Benchmark find_cache
    let start = std::time::Instant::now();
    for i in 0..ELEMENTS {
        let path = format!("prefix/{}", i % PATHS);
        bm.find_cache(ListPathOptions {
            id: Uuid::new_v4(),
            base_dir: path,
            create: true,
            ..Default::default()
        });
    }
    let elapsed = start.elapsed();
    eprintln!("find_cache x {} took {:?}", ELEMENTS, elapsed);
    */
}
