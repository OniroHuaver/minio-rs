//! Metacache stream reader/writer tests.
//!
//! Tests metacacheReader and metacacheWriter streaming read/write operations.
//!
//! Test data comes from `testdata/metacache.s2`, containing file and directory listings
//! from the Go standard library `compress/` directory.


/// Helper function to load metacache sample data.
///
/// Reads sample data from testdata/metacache.s2.
/// In Rust, this needs a test data file in the corresponding format.
#[test]
#[ignore]
fn test_load_metacache_sample() {
    // TODO: implement when metacacheReader for s2-compressed stream is available
    /*
    let data = std::fs::read("testdata/metacache.s2").expect("read test data");
    let reader = MetacacheReader::new(&data[..]);
    // verify loading succeeds
    */
}

/// Tests metacacheReader.readNames().
///
/// Read all entry names, verify they match loadMetacacheSampleNames.
#[test]
#[ignore]
fn test_metacache_reader_read_names() {
    // TODO: implement when metacacheReader::read_names is available
}

/// Tests metacacheReader.readN().
///
/// Verify:
/// - readN(-1) reads all entries
/// - readN(0) reads 0 entries
/// - readN(5) reads the first 5 entries
/// - Sequential reads return subsequent entries
#[test]
#[ignore]
fn test_metacache_reader_read_n() {
    // TODO: implement when metacacheReader::read_n is available
}

/// Tests metacacheReader.readN() without directories.
///
/// Verify:
/// - readN(-1, no_dirs=true) returns only file entries
/// - Count and content verification
#[test]
#[ignore]
fn test_metacache_reader_read_n_dirs() {
    // TODO: implement for readN with directory filtering
}

/// Tests metacacheReader.readN() with prefix filtering.
///
/// Verify:
/// - Filter "src/compress/bzip2/" -> returns all entries under that prefix
/// - Filter "src/nonexist" -> returns empty
/// - Filter "src/a" -> returns empty (no match)
/// - Filter "src/compress/zlib/e" -> returns example_test.go
#[test]
#[ignore]
fn test_metacache_reader_read_n_prefix() {
    // TODO: implement for readN with prefix filtering
}

/// Tests metacacheReader.readFn().
///
/// Use a callback to iterate all entries, verify names match expected values.
#[test]
#[ignore]
fn test_metacache_reader_read_fn() {
    // TODO: implement when metacacheReader::read_fn is available
}

/// Tests metacacheReader.readAll().
///
/// Read all entries asynchronously via channel, verify names and order.
#[test]
#[ignore]
fn test_metacache_reader_read_all() {
    // TODO: implement when metacacheReader::read_all with channel is available
}

/// Tests metacacheReader.forwardTo().
///
/// Verify:
/// - forwardTo("src/compress/zlib/reader_test.go") returns only subsequent entries
/// - Partial prefix also locates correctly
#[test]
#[ignore]
fn test_metacache_reader_forward_to() {
    // TODO: implement when metacacheReader::forward_to is available
}

/// Tests metacacheReader.next().
///
/// Read entries one by one, verify names and order.
#[test]
#[ignore]
fn test_metacache_reader_next() {
    // TODO: implement when metacacheReader::next is available
}

/// Tests metacacheReader.peek().
///
/// Verify peek does not consume the entry, subsequent next returns the same entry.
#[test]
#[ignore]
fn test_metacache_reader_peek() {
    // TODO: implement when metacacheReader::peek is available
}

/// Tests metacacheWriter + metacacheReader full stream roundtrip.
///
/// Write sample data to a buffer via writer,
/// then read back via reader and verify names match.
#[test]
#[ignore]
fn test_new_metacache_stream() {
    // TODO: implement when metacacheWriter and metacacheReader stream roundtrip is available
}

/// Tests metacacheReader.skip().
///
/// Read 5 entries, skip 5, read 5 more,
/// verify fetched entries are correct (starting from the 10th).
/// Skipping beyond range should return EOF.
#[test]
#[ignore]
fn test_metacache_reader_skip() {
    // TODO: implement when metacacheReader::skip is available
}
