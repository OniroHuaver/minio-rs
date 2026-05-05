//! Erasure encoding tests — shard layout, size, and data integrity.
//!
//! ## Test coverage (derived from STORAGE_SPEC.md §3, ARCHITECTURE.md §6)
//!
//! - Shard count = M+N across all parity tiers
//! - Shard size uniformity (all equal after zero-pad)
//! - Data-shard content correctness (raw bytes before RS)
//! - Parity shards are non-trivial (≠ all-zero, RS transform applied)
//! - Deterministic encode: same input → identical output
//! - Re-encode stability: multiple encode calls produce identical shards
//! - Small-file inline threshold (≤128 KiB) shard sizes
//! - Large-file boundary (multiple MiB) shard sizes
//! - M+N ≤ 256 limit

use erasure::Erasure;

fn test_bytes(seed: u64, len: usize) -> Vec<u8> {
    let mut v = Vec::with_capacity(len);
    let mut s = seed.wrapping_mul(0x9e3779b97f4a7c15);
    for _ in 0..len {
        s = s.wrapping_add(0x9e3779b97f4a7c15);
        v.push((s.wrapping_mul(s >> 33) >> 24) as u8);
    }
    v
}

// ---------------------------------------------------------------------------
// Shard count
// ---------------------------------------------------------------------------

#[test]
fn shard_count_matches_total() {
    let configs = &[(2, 1), (2, 2), (3, 2), (4, 2), (4, 4), (6, 3), (8, 4), (12, 4)];
    for &(m, n) in configs {
        let ec = Erasure::new(m, n).unwrap_or_else(|e| panic!("new({m},{n}): {e}"));
        let shards = ec.encode(&test_bytes(m as u64, 1024)).expect("encode");
        assert_eq!(shards.len(), m + n, "wrong shard count for {m}+{n}");
    }
}

// ---------------------------------------------------------------------------
// Shard size uniformity
// ---------------------------------------------------------------------------

#[test]
fn all_shards_equal_size() {
    // STORAGE_SPEC.md §3.2: each shard = shard_size bytes, last data shard zero-padded
    let configs = &[(4, 2), (8, 4), (3, 3)];
    for &(m, n) in configs {
        let ec = Erasure::new(m, n).expect("new");
        for &data_len in &[1usize, 63, 64, 255, 256, 1023, 1024, 4097, 65537] {
            let shards = ec
                .encode(&test_bytes(data_len as u64, data_len))
                .unwrap_or_else(|e| panic!("encode {data_len}: {e}"));
            let expected = (data_len + m - 1) / m;
            for (i, s) in shards.iter().enumerate() {
                assert_eq!(
                    s.len(),
                    expected,
                    "{m}+{n} len={data_len}: shard[{i}] size mismatch"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Data-shard content
// ---------------------------------------------------------------------------

#[test]
fn data_shards_contain_original_bytes() {
    // first M shards must contain the original data (zero-padded to shard_size)
    let ec = Erasure::new(4, 2).expect("4+2");
    let original = test_bytes(42, 4000);
    let shards = ec.encode(&original).expect("encode");
    let shard_size = (4000 + 3) / 4; // = 1000

    let mut reconstructed = Vec::new();
    for i in 0..4 {
        reconstructed.extend_from_slice(&shards[i]);
    }
    assert_eq!(&reconstructed[..4000], &original[..]);
    // trailing zeros in last shard
    assert!(reconstructed[4000..].iter().all(|&b| b == 0));
    assert_eq!(reconstructed.len(), 4 * shard_size);
}

// ---------------------------------------------------------------------------
// Parity shards are non-trivial
// ---------------------------------------------------------------------------

#[test]
fn parity_shards_are_non_zero() {
    let ec = Erasure::new(4, 2).expect("4+2");
    let data = test_bytes(1, 4096);
    let shards = ec.encode(&data).expect("encode");

    // parity shards (index M..M+N-1) should not be all-zero with real data
    for i in 4..6 {
        let is_zero = shards[i].iter().all(|&b| b == 0);
        assert!(!is_zero, "parity shard[{i}] is unexpectedly all-zero");
    }
}

// ---------------------------------------------------------------------------
// Deterministic encoding
// ---------------------------------------------------------------------------

#[test]
fn encode_is_deterministic() {
    let ec = Erasure::new(8, 4).expect("8+4");
    let data = test_bytes(0xcafe, 128 * 1024); // 128 KiB

    let a = ec.encode(&data).expect("first");
    let b = ec.encode(&data).expect("second");

    assert_eq!(a.len(), b.len());
    for i in 0..a.len() {
        assert_eq!(a[i], b[i], "shard[{i}] differs between encodes");
    }
}

// ---------------------------------------------------------------------------
// Re-encode with varying sizes
// ---------------------------------------------------------------------------

#[test]
fn encode_various_data_sizes() {
    // covers inline (≤128 KiB), mid-range, and large (multi-MiB) sizes
    // STORAGE_SPEC.md §3.3, §7.2, §7.3
    let sizes = &[
        1usize,                                   // 1 B
        128,                                       // 128 B
        1024,                                      // 1 KiB
        65536,                                     // 64 KiB
        128 * 1024 - 1,                            // just under 128 KiB
        128 * 1024,                                // exact 128 KiB threshold
        128 * 1024 + 1,                            // just over threshold
        1024 * 1024,                               // 1 MiB
        4 * 1024 * 1024,                           // 4 MiB (default block size)
        4 * 1024 * 1024 + 1,                       // just over block size
        8 * 1024 * 1024,                           // 8 MiB
    ];

    let ec = Erasure::new(8, 4).expect("8+4");
    for &size in sizes {
        let data = test_bytes(size as u64 + 1, size);
        let shards = ec.encode(&data).expect("encode");
        assert_eq!(shards.len(), 12, "shard count for size={size}");

        // full roundtrip verification
        let all: Vec<_> = shards.iter().map(|s| Some(s.clone())).collect();
        let decoded = ec.decode(&all).expect("decode");
        assert_eq!(&decoded[..size], &data[..], "roundtrip size={size}");
    }
}

// ---------------------------------------------------------------------------
// Max shards limit
// ---------------------------------------------------------------------------

#[test]
fn max_total_shards_limit() {
    // M+N ≤ 256 per the native ReedSolomon limit
    assert!(Erasure::new(200, 100).is_err(), "should reject M+N > 256");
    assert!(Erasure::new(128, 128).is_ok(), "256 should be allowed");
    assert!(Erasure::new(255, 1).is_ok(), "256 should be allowed");
    assert!(Erasure::new(200, 57).is_err(), "257 should be rejected");
}

// ---------------------------------------------------------------------------
// with_default_parity — full coverage
// ---------------------------------------------------------------------------

#[test]
fn default_parity_all_valid_tiers() {
    // Verify every valid total_disk count gets the right parity per ARCHITECTURE.md §6
    fn expected(total: usize) -> (usize, usize) {
        let parity = match total {
            0..=5 => 2,
            6..=7 => 3,
            _ => 4,
        };
        (total - parity, parity)
    }

    for total in 3..=32 {
        let ec = Erasure::with_default_parity(total)
            .unwrap_or_else(|e| panic!("with_default_parity({total}): {e}"));
        let (exp_data, exp_parity) = expected(total);
        assert_eq!(ec.params().data_blocks, exp_data, "data for total={total}");
        assert_eq!(ec.params().parity_blocks, exp_parity, "parity for total={total}");
        assert_eq!(ec.params().total_shards(), total);
    }
}

// ---------------------------------------------------------------------------
// Shard size edge cases — unaligned data to block boundaries
// ---------------------------------------------------------------------------

#[test]
fn shard_size_unaligned_vs_block_size() {
    // DEFAULT_BLOCK_SIZE = 4 MiB (§6 of STORAGE_SPEC.md), but shard calculation
    // uses ceil(data_len / M) not block size. Verify independence.
    let ec = Erasure::new(4, 2).expect("4+2");
    let data = test_bytes(7, 4 * 1024 * 1024 - 1); // 4 MiB - 1
    let shards = ec.encode(&data).expect("encode");
    let expected_shard = (4 * 1024 * 1024 - 1 + 3) / 4; // = 1 MiB
    for (i, s) in shards.iter().enumerate() {
        assert_eq!(s.len(), expected_shard, "shard[{i}] size");
    }
}
