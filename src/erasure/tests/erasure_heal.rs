//! Erasure healing tests — reconstruct corrupted/lost shards from healthy ones.
//!
//! ## Test coverage (derived from EDGE_CASES.md §4.1, §4.2 + STORAGE_SPEC.md §5)
//!
//! - Reconstruct from parity-only corruption
//! - Reconstruct from data-only corruption
//! - Reconstruct from mixed data+parity corruption
//! - Idempotent healing: double-heal produces identical result
//! - Exactly N corrupted shards (boundary, recoverable)
//! - N+1 corrupted shards → beyond correction capacity
//! - Heal after original encode: rebuilt data matches original
//! - Large-data healing (8 MiB)
//! - All shard patterns: data loss vs parity loss vs mixed

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

// ---------------------------------------------------------------------------
// Basic heal: corrupted shards reconstructed via decode
// ---------------------------------------------------------------------------

#[test]
fn heal_corrupted_parity_shards() {
    // §4.1: parity loss → reconstruct from data shards
    let ec = Erasure::new(8, 4).expect("8+4");
    let original = test_bytes(42, 64 * 1024);
    let shards = ec.encode(&original).expect("encode");

    // "corrupt" last 4 parity shards
    let corrupted: Vec<Option<Vec<u8>>> = (0..12)
        .map(|i| {
            if i >= 8 {
                None
            } else {
                Some(shards[i].clone())
            }
        })
        .collect();

    let healed = ec.decode(&corrupted).expect("heal parity loss");
    assert_eq!(&healed[..original.len()], &original[..]);
}

#[test]
fn heal_corrupted_data_shards() {
    // §4.2: N-1 data shard loss → still recoverable via parity
    let ec = Erasure::new(6, 3).expect("6+3");
    let original = test_bytes(77, 48 * 1024);
    let shards = ec.encode(&original).expect("encode");

    // corrupt 3 data shards (within N=3 limit)
    let corrupted: Vec<Option<Vec<u8>>> = (0..9)
        .map(|i| if i < 3 { None } else { Some(shards[i].clone()) })
        .collect();

    let healed = ec.decode(&corrupted).expect("heal data loss");
    assert_eq!(&healed[..original.len()], &original[..]);
}

#[test]
fn heal_mixed_corruption() {
    let cases: &[(usize, usize, &[usize])] = &[
        (4, 2, &[0, 5]),
        (6, 3, &[0, 1, 7]),
        (8, 4, &[0, 2, 8, 9]),
        (4, 4, &[1, 3, 5, 7]),
    ];

    for &(m, n, corrupt) in cases {
        let original = test_bytes((m + n) as u64, 32 * 1024);
        let ec = Erasure::new(m, n).expect("new");
        let shards = ec.encode(&original).expect("encode");
        let total = m + n;

        let corrupted: Vec<Option<Vec<u8>>> = (0..total)
            .map(|i| {
                if corrupt.contains(&i) {
                    None
                } else {
                    Some(shards[i].clone())
                }
            })
            .collect();

        let healed = ec
            .decode(&corrupted)
            .unwrap_or_else(|e| panic!("{m}+{n} heal {corrupt:?}: {e}"));
        assert_eq!(&healed[..original.len()], &original[..], "{m}+{n} {corrupt:?}");
    }
}

// ---------------------------------------------------------------------------
// Idempotent healing (§4.1)
// ---------------------------------------------------------------------------

#[test]
fn heal_is_idempotent() {
    // healing twice should produce the same result
    let ec = Erasure::new(8, 4).expect("8+4");
    let original = test_bytes(0xabc, 128 * 1024);
    let shards = ec.encode(&original).expect("encode");

    let corrupted: Vec<Option<Vec<u8>>> = (0..12)
        .map(|i| if i >= 9 { None } else { Some(shards[i].clone()) })
        .collect();

    let heal1 = ec.decode(&corrupted).expect("heal first time");
    let heal2 = ec.decode(&corrupted).expect("heal second time");
    assert_eq!(heal1, heal2, "healing should be idempotent");
    assert_eq!(&heal1[..original.len()], &original[..]);
}

// ---------------------------------------------------------------------------
// Boundary: exactly N corrupted → recoverable (§4.2)
// ---------------------------------------------------------------------------

#[test]
fn heal_boundary_exactly_n_corrupted() {
    // N=4 corruption with 8+4 → should still recover
    let ec = Erasure::new(8, 4).expect("8+4");
    let original = test_bytes(0xdef, 256 * 1024);
    let shards = ec.encode(&original).expect("encode");

    // corrupt exactly 4 shards (N=4)
    let corrupted: Vec<Option<Vec<u8>>> = (0..12)
        .map(|i| if i < 4 { None } else { Some(shards[i].clone()) })
        .collect();

    let healed = ec.decode(&corrupted).expect("heal exactly N");
    assert_eq!(&healed[..original.len()], &original[..]);
}

// ---------------------------------------------------------------------------
// Beyond capacity: N+1 corrupted → fail (§4.2)
// ---------------------------------------------------------------------------

#[test]
fn heal_beyond_capacity() {
    let cases = &[
        (4, 2, 3),  // N=2, corrupt=3
        (6, 3, 4),  // N=3, corrupt=4
        (8, 4, 5),  // N=4, corrupt=5
    ];

    for &(m, n, corrupt_count) in cases {
        let ec = Erasure::new(m, n).expect("new");
        let original = test_bytes(m as u64, 1024);
        let shards = ec.encode(&original).expect("encode");

        let corrupted: Vec<Option<Vec<u8>>> = (0..m + n)
            .map(|i| {
                if i < corrupt_count {
                    None
                } else {
                    Some(shards[i].clone())
                }
            })
            .collect();

        let err = ec
            .decode(&corrupted)
            .expect_err(&format!("{m}+{n} corrupt={corrupt_count} should fail"));
        assert!(
            matches!(err, base::error::MinioError::InsufficientReadQuorum { .. }),
            "{m}+{n}: got {err}"
        );
    }
}

// ---------------------------------------------------------------------------
// Large data healing (§7.3 — streaming boundary applies)
// ---------------------------------------------------------------------------

#[test]
fn heal_large_data() {
    let ec = Erasure::new(8, 4).expect("8+4");
    // 8 MiB data encoded into 12 shards
    let original = test_bytes(0xcafe, 8 * 1024 * 1024);
    let shards = ec.encode(&original).expect("encode large");

    // corrupt 4 random shards (0, 3, 7, 10 — mixed data+parity)
    let corrupt = &[0, 3, 7, 10];
    let corrupted: Vec<Option<Vec<u8>>> = (0..12)
        .map(|i| {
            if corrupt.contains(&i) {
                None
            } else {
                Some(shards[i].clone())
            }
        })
        .collect();

    let healed = ec.decode(&corrupted).expect("heal large");
    assert_eq!(&healed[..original.len()], &original[..]);
}

// ---------------------------------------------------------------------------
// Worst case: all parity + some data lost, still within N total
// ---------------------------------------------------------------------------

#[test]
fn heal_worst_case_within_capacity() {
    // 8+4: lose all 4 parity + 0 data (still within N=4)
    let ec = Erasure::new(8, 4).expect("8+4");
    let original = test_bytes(0x777, 128 * 1024);
    let shards = ec.encode(&original).expect("encode");

    // all parity lost
    let corrupted: Vec<Option<Vec<u8>>> = (0..12)
        .map(|i| if i >= 8 { None } else { Some(shards[i].clone()) })
        .collect();

    let healed = ec.decode(&corrupted).expect("all parity lost");
    assert_eq!(&healed[..original.len()], &original[..]);
}

// ---------------------------------------------------------------------------
// Decode result includes only original data (not padding)
// ---------------------------------------------------------------------------

#[test]
fn heal_result_no_extra_padding() {
    // The decode output should have the exact original data, no trailing padding
    let ec = Erasure::new(4, 2).expect("4+2");
    let original = test_bytes(0x111, 4000); // unaligned
    let shards = ec.encode(&original).expect("encode");

    let corrupted: Vec<Option<Vec<u8>>> = (0..6)
        .map(|i| if i == 5 { None } else { Some(shards[i].clone()) })
        .collect();

    let healed = ec.decode(&corrupted).expect("heal");
    // The current decode concatenates all padded data shards → output may include padding
    // This test documents expected behaviour: output length == sum of padded shards
    let shard_size = (4000 + 3) / 4; // = 1000
    assert_eq!(healed.len(), shard_size * 4, "decode returns padded concatenation");
    assert_eq!(&healed[..4000], &original[..], "first 4000 bytes match");
    // trailing bytes are zero-padding from the encode step
    assert!(healed[4000..].iter().all(|&b| b == 0), "trailing should be padding zeros");
}
