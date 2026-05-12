//! Bucket-level metacache tests.
//!
//! Tests bucketMetacache findCache method performance.


/// Benchmark: bucketMetacache.findCache performance.
///
/// Warm up 50000 cache entries (100 distinct paths, 500 each),
/// then measure findCache performance.
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
