//! erasure: Reed-Solomon erasure coding layer
//!
//! ## Core responsibilities
//!
//! - Encode objects into M+N shards
//! - Reconstruct original data from any M shards
//! - Quorum determination (WriteQuorum / ReadQuorum)
//! - Parallel shard read/write
//!
//! ## Parity strategy
//!
//! | Storage Class | Disks | Parity blocks |
//! |--------------|-------|---------------|
//! | STANDARD     | <=5   | 2             |
//! | STANDARD     | 6-7   | 3             |
//! | STANDARD     | >=8   | 4             |
//! | REDUCED      | Any   | 1             |

pub mod bitrot;

use crate::base::erasure::ErasureParams;
use crate::base::error::{MinioError, MinioResult};
use reed_solomon_erasure::galois_8::ReedSolomon;

/// Erasure Coding engine
///
pub struct Erasure {
    params: ErasureParams,
    encoder: ReedSolomon,
}

impl Erasure {
    /// Create an EC engine
    pub fn new(data_blocks: usize, parity_blocks: usize) -> MinioResult<Self> {
        let params = ErasureParams {
            data_blocks,
            parity_blocks,
            block_size: 4 * 1024 * 1024, // 4 MiB
        };

        if data_blocks < 1 || parity_blocks < 1 {
            return Err(MinioError::Internal(
                "data_blocks and parity_blocks must be >= 1".into(),
            ));
        }
        if data_blocks + parity_blocks > 256 {
            return Err(MinioError::Internal("M + N must not exceed 256".into()));
        }

        let encoder = ReedSolomon::new(data_blocks, parity_blocks)
            .map_err(|e| MinioError::Internal(format!("ReedSolomon initialization failed: {e}")))?;

        Ok(Self { params, encoder })
    }

    /// Automatically select the appropriate number of parity blocks
    ///
    pub fn with_default_parity(total_disks: usize) -> MinioResult<Self> {
        let parity = match total_disks {
            0..=5 => 2,
            6..=7 => 3,
            _ => 4,
        };
        let data = total_disks - parity;
        Self::new(data, parity)
    }

    pub fn params(&self) -> &ErasureParams {
        &self.params
    }

    /// Encode: split data into M+N shards
    ///
    /// Returns `total_shards` `Vec<u8>` pieces, all with equal length.
    /// If data length is not divisible by block_size, zero-pads automatically.
    pub fn encode(&self, data: &[u8]) -> MinioResult<Vec<Vec<u8>>> {
        let m = self.params.data_blocks;
        let n = self.params.parity_blocks;
        let total = m + n;

        // Calculate shard size (round up)
        let shard_size = (data.len() + m - 1) / m;
        // Build equal-length shards (zero-padded)
        let mut shards: Vec<Vec<u8>> = (0..total).map(|_| vec![0u8; shard_size]).collect();

        // Fill data shards
        for (i, chunk) in data.chunks(shard_size).enumerate() {
            if i < m {
                shards[i][..chunk.len()].copy_from_slice(chunk);
            }
        }

        // Compute parity shards
        self.encoder
            .encode(&mut shards)
            .map_err(|e| MinioError::EncodeError(e.to_string()))?;

        // If original data is already aligned, no truncation needed
        // Note: MinIO doesn't record padding length in shards; it uses PartActualSize instead
        Ok(shards)
    }

    /// Decode: reconstruct original data from shards
    ///
    /// `shards` length must be `total_shards`; missing shards are represented by `None`.
    /// At least `data_blocks` valid shards are required.
    pub fn decode(&self, shards: &[Option<Vec<u8>>]) -> MinioResult<Vec<u8>> {
        if shards.len() != self.params.total_shards() {
            return Err(MinioError::Internal(format!(
                "incorrect shard count: expected {}, got {}",
                self.params.total_shards(),
                shards.len()
            )));
        }

        let present = shards.iter().filter(|s| s.is_some()).count();
        if present < self.params.data_blocks {
            return Err(MinioError::InsufficientReadQuorum {
                required: self.params.data_blocks,
                actual: present,
            });
        }

        // Ensure all shards have equal length
        let max_len = shards.iter().flatten().map(|s| s.len()).max().unwrap_or(0);

        let mut padded: Vec<Option<Vec<u8>>> = shards
            .iter()
            .map(|s| {
                let mut v = s.clone().unwrap_or_default();
                v.resize(max_len, 0);
                if s.is_some() { Some(v) } else { None }
            })
            .collect();

        self.encoder
            .reconstruct(&mut padded)
            .map_err(|e| MinioError::DecodeError(e.to_string()))?;

        // Concatenate data shards
        let data: Vec<u8> = padded[..self.params.data_blocks]
            .iter()
            .flatten()
            .flat_map(|s| s.as_slice())
            .copied()
            .collect();

        Ok(data)
    }
}

/// Integration tests are in `tests/`, auto-discovered by Cargo.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_roundtrip() {
        let ec = Erasure::new(4, 2).expect("create EC engine");
        let original = b"Hello, MinIO Erasure Coding! This is a test payload.";

        let shards = ec.encode(original).expect("encode");
        assert_eq!(shards.len(), 6);

        // Normal decode (all shards available)
        let all_shards: Vec<Option<Vec<u8>>> = shards.iter().map(|s| Some(s.clone())).collect();
        let decoded = ec.decode(&all_shards).expect("decode");
        assert_eq!(&decoded[..original.len()], original);

        // Simulate 2 lost shards (exactly at quorum boundary)
        let mut partial = shards.clone();
        partial[0] = vec![0u8; partial[0].len()]; // Mark as corrupted
        partial[1] = vec![0u8; partial[1].len()];

        let partial_shards: Vec<Option<Vec<u8>>> = partial
            .iter()
            .enumerate()
            .map(|(i, s)| if i < 2 { None } else { Some(s.clone()) })
            .collect();

        let decoded = ec.decode(&partial_shards).expect("decode with 2 missing");
        assert_eq!(&decoded[..original.len()], original);
    }

    #[test]
    fn test_insufficient_quorum() {
        let ec = Erasure::new(4, 2).expect("create EC engine");
        let shards: Vec<Option<Vec<u8>>> = (0..6)
            .map(|i| if i < 2 { Some(vec![0u8; 64]) } else { None })
            .collect();

        let result = ec.decode(&shards);
        assert!(result.is_err());
    }
}
