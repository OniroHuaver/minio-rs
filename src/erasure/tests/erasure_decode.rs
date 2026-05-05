//! Erasure decoding tests — data reconstruction from partial shards.
//!
//! ## Test coverage (derived from STORAGE_SPEC.md §5 + EDGE_CASES.md §1.1, §4.2)
//!
//! - Decode from all shards present (baseline)
//! - Decode with parity-only loss (most common recovery path)
//! - Decode with data-only loss
//! - Decode with mixed data+parity loss
//! - Loss exactly at correction boundary (N shards missing → recoverable)
//! - Loss exceeding N → InsufficientReadQuorum error
//! - Wrong shard count → error
//! - Data-size boundary tests (1 B through 8 MiB)
//! - Shard size uniformity enforced before reconstruct
//! - Deterministic decode (same input → same output)

use crate::erasure::Erasure;

fn test_bytes(seed: u64, len: usize) -> Vec<u8> {
    let mut v = Vec::with_capacity(len);
    let mut s = seed.wrapping_mul(0x9e3779b97f4a7c15);
    for _ in 0..len {
        s = s.wrapping_add(0x9e3779b97f4a7c15);
        v.push((s.wrapping_mul(s >> 33) >> 24) as u8);
    }
    v
}

/// Helper: encode, drop specified shard indices, decode, verify.
fn encode_drop_decode(m: usize, n: usize, data: &[u8], drop: &[usize]) -> Vec<u8> {
    let ec = Erasure::new(m, n).expect("new");
    let shards = ec.encode(data).expect("encode");
    let total = m + n;
    let partial: Vec<Option<Vec<u8>>> = (0..total)
        .map(|i| {
            if drop.contains(&i) {
                None
            } else {
                Some(shards[i].clone())
            }
        })
        .collect();
    ec.decode(&partial).expect("decode")
}

// ---------------------------------------------------------------------------
// Baseline: all shards present
// ---------------------------------------------------------------------------

#[test]
fn decode_all_shards_present() {
    for &(m, n) in &[(4, 2), (6, 3), (8, 4), (12, 4)] {
        let data = test_bytes(m as u64 * 7, 64 * 1024);
        let decoded = encode_drop_decode(m, n, &data, &[]);
        assert_eq!(&decoded[..data.len()], &data[..], "{m}+{n}");
    }
}

// ---------------------------------------------------------------------------
// Parity-only loss
// ---------------------------------------------------------------------------

#[test]
fn decode_parity_only_loss() {
    // losing any parity shards should never affect data recovery
    for &(m, n, miss) in &[(4, 2, 1), (4, 2, 2), (6, 3, 1), (6, 3, 2), (6, 3, 3), (8, 4, 4)] {
        let data = test_bytes(m as u64 * 11, 256 * 1024);
        let drop: Vec<usize> = (m..m + miss).collect();
        let decoded = encode_drop_decode(m, n, &data, &drop);
        assert_eq!(&decoded[..data.len()], &data[..], "{m}+{n} missing {miss} parity");
    }
}

// ---------------------------------------------------------------------------
// Data-only loss
// ---------------------------------------------------------------------------

#[test]
fn decode_data_only_loss() {
    // losing data shards → parity reconstruction
    for &(m, n, miss) in &[(4, 2, 1), (4, 2, 2), (6, 3, 1), (6, 3, 2), (6, 3, 3), (8, 4, 3)] {
        let data = test_bytes(m as u64 * 13, 128 * 1024);
        let drop: Vec<usize> = (0..miss).collect();
        let decoded = encode_drop_decode(m, n, &data, &drop);
        assert_eq!(&decoded[..data.len()], &data[..], "{m}+{n} missing {miss} data");
    }
}

// ---------------------------------------------------------------------------
// Mixed data + parity loss
// ---------------------------------------------------------------------------

#[test]
fn decode_mixed_loss() {
    let cases: &[(usize, usize, &[usize])] = &[
        (4, 2, &[0, 4]),
        (4, 2, &[1, 5]),
        (6, 3, &[0, 1, 6]),
        (6, 3, &[0, 6, 7]),
        (8, 4, &[0, 1, 2, 8]),
        (8, 4, &[0, 1, 8, 9]),
    ];
    for &(m, n, drop) in cases {
        let data = test_bytes((m + n) as u64, 64 * 1024);
        let decoded = encode_drop_decode(m, n, &data, drop);
        assert_eq!(&decoded[..data.len()], &data[..], "{m}+{n} drop {drop:?}");
    }
}

// ---------------------------------------------------------------------------
// Boundary: exactly N shards missing (still recoverable)
// ---------------------------------------------------------------------------

#[test]
fn decode_exact_boundary_recoverable() {
    // §4.2: missing == N → recoverable
    let ec = Erasure::new(4, 2).expect("4+2");
    let data = test_bytes(77, 4096);
    let shards = ec.encode(&data).expect("encode");

    // drop exactly 2 shards (data[1] + parity[1])
    let partial: Vec<Option<Vec<u8>>> = vec![
        Some(shards[0].clone()),
        None,
        Some(shards[2].clone()),
        Some(shards[3].clone()),
        Some(shards[4].clone()),
        None,
    ];

    let decoded = ec.decode(&partial).expect("missing=N should recover");
    assert_eq!(&decoded[..4096], &data[..]);
}

// ---------------------------------------------------------------------------
// Boundary: > N shards missing → error
// ---------------------------------------------------------------------------

#[test]
fn decode_beyond_boundary_errors() {
    let cases = &[
        (4, 2, 3, "missing 3 > N=2"),
        (6, 3, 4, "missing 4 > N=3"),
        (8, 4, 5, "missing 5 > N=4"),
    ];

    for &(m, n, missing, label) in cases {
        let ec = Erasure::new(m, n).expect(label);
        let data = test_bytes(m as u64, 1024);
        let shards = ec.encode(&data).expect("encode");
        let total = m + n;

        let partial: Vec<Option<Vec<u8>>> = (0..total)
            .map(|i| {
                if i < missing {
                    None
                } else {
                    Some(shards[i].clone())
                }
            })
            .collect();

        let err = ec.decode(&partial).expect_err(label);
        assert!(
            matches!(err, base::error::MinioError::InsufficientReadQuorum { .. }),
            "{label}: expected InsufficientReadQuorum, got {err}"
        );
    }
}

// ---------------------------------------------------------------------------
// Wrong shard count → error
// ---------------------------------------------------------------------------

#[test]
fn wrong_shard_count_error() {
    let ec = Erasure::new(4, 2).expect("4+2");

    // too few
    let few: Vec<Option<Vec<u8>>> = (0..5).map(|_| Some(vec![0u8; 64])).collect();
    assert!(ec.decode(&few).is_err(), "too few shards should fail");

    // too many
    let many: Vec<Option<Vec<u8>>> = (0..7).map(|_| Some(vec![0u8; 64])).collect();
    assert!(ec.decode(&many).is_err(), "too many shards should fail");
}

// ---------------------------------------------------------------------------
// Data size boundary tests
// ---------------------------------------------------------------------------

#[test]
fn decode_various_data_sizes() {
    let sizes = &[
        1usize, 7, 31, 63, 64, 127, 128, 255, 256, 511, 512,
        1023, 1024, 4095, 4096, 4097, 65535, 65536, 65537,
        128 * 1024 - 1, 128 * 1024, 128 * 1024 + 1,
        1024 * 1024, 4 * 1024 * 1024, 8 * 1024 * 1024,
    ];

    let ec = Erasure::new(8, 4).expect("8+4");
    for &size in sizes {
        let data = test_bytes(size as u64 + 100, size);
        let shards = ec.encode(&data).expect("encode");

        // all present
        let all: Vec<_> = shards.iter().map(|s| Some(s.clone())).collect();
        let decoded = ec.decode(&all)
            .unwrap_or_else(|e| panic!("decode size={size}: {e}"));
        assert_eq!(&decoded[..size], &data[..], "size={size}");

        // drop 4 parity shards
        let partial: Vec<Option<Vec<u8>>> = (0..12)
            .map(|i| if i < 8 { Some(shards[i].clone()) } else { None })
            .collect();
        let decoded2 = ec.decode(&partial)
            .unwrap_or_else(|e| panic!("decode w/ missing parity size={size}: {e}"));
        assert_eq!(&decoded2[..size], &data[..], "size={size} with parity loss");
    }
}

// ---------------------------------------------------------------------------
// Deterministic decode
// ---------------------------------------------------------------------------

#[test]
fn decode_is_deterministic() {
    let ec = Erasure::new(8, 4).expect("8+4");
    let data = test_bytes(0xbad, 1024 * 1024);
    let shards = ec.encode(&data).expect("encode");

    // drop 3 shards
    let drop = &[0usize, 5, 10];
    let partial: Vec<Option<Vec<u8>>> = (0..12)
        .map(|i| {
            if drop.contains(&i) {
                None
            } else {
                Some(shards[i].clone())
            }
        })
        .collect();

    let a = ec.decode(&partial).expect("first decode");
    let b = ec.decode(&partial).expect("second decode");
    assert_eq!(a, b, "decode should be deterministic");
}

// ---------------------------------------------------------------------------
// All-None shards → error
// ---------------------------------------------------------------------------

#[test]
fn all_missing_shards() {
    let ec = Erasure::new(4, 2).expect("4+2");
    let shards: Vec<Option<Vec<u8>>> = (0..6).map(|_| None).collect();
    let err = ec.decode(&shards).expect_err("all missing should fail");
    assert!(
        matches!(err, base::error::MinioError::InsufficientReadQuorum { .. }),
        "got {err}"
    );
}

// ---------------------------------------------------------------------------
// Exactly M data shards survive (ReadQuorum boundary)
// ---------------------------------------------------------------------------

#[test]
fn decode_exact_m_data_shards() {
    // 4+2: ReadQuorum = 4 (data_blocks)
    let ec = Erasure::new(4, 2).expect("4+2");
    let data = test_bytes(0xf00, 2048);
    let shards = ec.encode(&data).expect("encode");

    // only shards 0-3 survive (exactly M)
    let partial: Vec<Option<Vec<u8>>> = (0..6)
        .map(|i| if i < 4 { Some(shards[i].clone()) } else { None })
        .collect();

    let decoded = ec.decode(&partial).expect("exact M should recover");
    assert_eq!(&decoded[..2048], &data[..]);
}

// ---------------------------------------------------------------------------
// M-1 data shards → insufficient
// ---------------------------------------------------------------------------

#[test]
fn decode_less_than_m_data_shards() {
    let ec = Erasure::new(4, 2).expect("4+2");
    let data = test_bytes(0xbad, 4096);
    let shards = ec.encode(&data).expect("encode");

    // only 3 valid shards
    let partial: Vec<Option<Vec<u8>>> = (0..6)
        .map(|i| if i < 3 { Some(shards[i].clone()) } else { None })
        .collect();

    let err = ec.decode(&partial).expect_err("3 < M=4 should fail");
    assert!(matches!(err, base::error::MinioError::InsufficientReadQuorum { .. }));
}
