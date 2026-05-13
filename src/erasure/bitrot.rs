//! Bitrot silent data corruption detection
//!
//! ## Data flow
//! - Write: `[data] -> hash64 -> [hash(8B LE) || data] -> write to disk`
//! - Read: `[hash(8B LE) || data] -> verify hash == HighwayHash(data) -> Some(data) or None(corrupted)`

use highway::HighwayHash;
use highway::HighwayHasher;

/// Bitrot detector — stateless, pure function
pub struct BitrotDetector;

impl BitrotDetector {
    /// Compute HighwayHash (64-bit) of data
    pub fn hash(data: &[u8]) -> u64 {
        let mut hasher = HighwayHasher::default();
        hasher.append(data);
        hasher.finalize64()
    }

    /// Wrap shard: `[hash(8B LE)] || data`
    pub fn wrap(data: &[u8]) -> Vec<u8> {
        let hash = Self::hash(data);
        let mut wrapped = Vec::with_capacity(8 + data.len());
        wrapped.extend_from_slice(&hash.to_le_bytes());
        wrapped.extend_from_slice(data);
        wrapped
    }

    /// Unwrap and verify: returns `Some(data)` if valid, `None` if corrupted or malformed
    pub fn unwrap(wrapped: &[u8]) -> Option<Vec<u8>> {
        if wrapped.len() < 8 {
            return None;
        }
        let expected = u64::from_le_bytes(wrapped[..8].try_into().unwrap());
        let data = &wrapped[8..];
        if Self::hash(data) == expected {
            Some(data.to_vec())
        } else {
            None
        }
    }

    /// Verify that data matches the expected hash
    pub fn verify(data: &[u8], expected: u64) -> bool {
        Self::hash(data) == expected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wrap_unwrap_roundtrip() {
        let data = b"hello bitrot world!";
        let wrapped = BitrotDetector::wrap(data);
        assert_eq!(wrapped.len(), 8 + data.len());

        let unwrapped = BitrotDetector::unwrap(&wrapped);
        assert_eq!(unwrapped.as_deref(), Some(data.as_slice()));
    }

    #[test]
    fn test_unwrap_corrupted_detected() {
        let data = b"hello bitrot world!";
        let mut wrapped = BitrotDetector::wrap(data);

        // Flip one bit in the data
        wrapped[10] ^= 0x01;
        assert!(BitrotDetector::unwrap(&wrapped).is_none());
    }

    #[test]
    fn test_unwrap_too_short() {
        assert!(BitrotDetector::unwrap(&[1, 2, 3]).is_none());
    }

    #[test]
    fn test_hash_deterministic() {
        let a = BitrotDetector::hash(b"same input");
        let b = BitrotDetector::hash(b"same input");
        assert_eq!(a, b);

        let c = BitrotDetector::hash(b"different input");
        assert_ne!(a, c);
    }
}
