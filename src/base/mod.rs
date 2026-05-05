//! base: Core types and constants
//!
//! This crate defines the shared foundational types for the MinIO storage system,
//! including core data structures and constant definitions.
//!
//! # Module overview
//!
//! - `format`: Rust struct definitions for the xl.meta on-disk format (MessagePack serialization)
//! - `erasure`: EC encoding parameters (dataBlocks, parityBlocks, blockSize)
//! - `constants`: Global constants (smallFileThreshold, bigFileThreshold, etc.)
//! - `error`: Unified error types
//! - `types`: Basic types (VersionID, ObjectKey, ETag, etc.)

pub mod constants;
pub mod error;
pub mod format;
pub mod types;

// Sub-modules needed in Phase 1, expand as needed
pub mod erasure {
    //! Erasure coding parameter definitions

    /// EC encoding configuration
    #[derive(Debug, Clone)]
    pub struct ErasureParams {
        /// Number of data blocks (M)
        pub data_blocks: usize,
        /// Number of parity blocks (N)
        pub parity_blocks: usize,
        /// EC block size (default 4 MiB)
        pub block_size: i64,
    }

    impl Default for ErasureParams {
        fn default() -> Self {
            Self {
                data_blocks: 0,
                parity_blocks: 0,
                block_size: 4 * 1024 * 1024, // 4 MiB
            }
        }
    }

    impl ErasureParams {
        /// Total number of shards M + N
        pub fn total_shards(&self) -> usize {
            self.data_blocks + self.parity_blocks
        }

        /// Write quorum: dataBlocks + 1 (when data > parity)
        pub fn write_quorum(&self) -> usize {
            if self.data_blocks == self.parity_blocks {
                self.data_blocks
            } else {
                self.data_blocks + 1
            }
        }

        /// Read quorum: totalShards - parityBlocks
        pub fn read_quorum(&self) -> usize {
            self.total_shards() - self.parity_blocks
        }
    }
}

pub mod hashing {
    //! Object routing hash

    /// Maps an object name to a Set index using SipHash-2-4
    ///
    /// This algorithm is deterministic, uniformly distributed, and high-throughput,
    /// used for object routing in distributed mode.
    pub fn sip_hash_mod(key: &[u8], num_sets: usize) -> usize {
        use std::hash::Hasher;
        let mut hasher = siphasher::sip::SipHasher24::new();
        hasher.write(key);
        (hasher.finish() % num_sets as u64) as usize
    }
}
