//! erasure: Reed-Solomon 擦除编码层
//!
//! 对应 Go: cmd/erasure-coding.go + cmd/erasure.go
//!
//! ## 核心职责
//!
//! - 将对象分片编码为 M+N 个 shard
//! - 从任意 M 个 shard 重建原始数据
//! - Quorum 判定 (WriteQuorum / ReadQuorum)
//! - 并行分片读写
//!
//! ## 校验策略
//!
//! | Storage Class | 磁盘数 | 校验块 |
//! |--------------|--------|--------|
//! | STANDARD     | ≤5     | 2      |
//! | STANDARD     | 6-7    | 3      |
//! | STANDARD     | ≥8     | 4      |
//! | REDUCED      | 任意   | 1      |

use base::error::{MinioError, MinioResult};
use base::erasure::ErasureParams;
use reed_solomon_erasure::galois_8::ReedSolomon;

/// Erasure Coding 引擎
///
/// 对应 Go: `cmd/erasure-coding.go Erasure`
pub struct Erasure {
    params: ErasureParams,
    encoder: ReedSolomon,
}

impl Erasure {
    /// 创建 EC 引擎
    pub fn new(data_blocks: usize, parity_blocks: usize) -> MinioResult<Self> {
        let params = ErasureParams {
            data_blocks,
            parity_blocks,
            block_size: 4 * 1024 * 1024, // 4 MiB
        };

        if data_blocks < 1 || parity_blocks < 1 {
            return Err(MinioError::Internal(
                "data_blocks 和 parity_blocks 必须 ≥ 1".into(),
            ));
        }
        if data_blocks + parity_blocks > 256 {
            return Err(MinioError::Internal(
                "M + N 不能超过 256".into(),
            ));
        }

        let encoder = ReedSolomon::new(data_blocks, parity_blocks)
            .map_err(|e| MinioError::Internal(format!("ReedSolomon 初始化失败: {e}")))?;

        Ok(Self { params, encoder })
    }

    /// 自动选择合适的校验块数
    ///
    /// 对应 Go 默认 EC 策略
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

    /// 编码：将数据切分为 M+N 个分片
    ///
    /// 返回 `total_shards` 个 `Vec<u8>`，所有分片长度相等。
    /// 如果数据长度不整除 block_size，自动填充 0。
    pub fn encode(&self, data: &[u8]) -> MinioResult<Vec<Vec<u8>>> {
        let m = self.params.data_blocks;
        let n = self.params.parity_blocks;
        let total = m + n;

        // 计算每分片大小 (向上取整)
        let shard_size = (data.len() + m - 1) / m;
        // 构造等长分片 (填充 0)
        let mut shards: Vec<Vec<u8>> = (0..total)
            .map(|_| vec![0u8; shard_size])
            .collect();

        // 填充数据分片
        for (i, chunk) in data.chunks(shard_size).enumerate() {
            if i < m {
                shards[i][..chunk.len()].copy_from_slice(chunk);
            }
        }

        // 计算校验分片
        self.encoder
            .encode(&mut shards)
            .map_err(|e| MinioError::EncodeError(e.to_string()))?;

        // 如果 original data 正好对齐，无需截断
        // 注意: 实际 MinIO 不在分片中记录填充长度，而是通过 PartActualSize 记录
        Ok(shards)
    }

    /// 解码：从分片中重建原始数据
    ///
    /// `shards` 长度必须为 `total_shards`，缺失分片用 `None` 表示。
    /// 需要至少 `data_blocks` 个有效分片。
    pub fn decode(&self, shards: &[Option<Vec<u8>>]) -> MinioResult<Vec<u8>> {
        if shards.len() != self.params.total_shards() {
            return Err(MinioError::Internal(format!(
                "分片数不正确: 期望 {}, 实际 {}",
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

        // 确保所有分片等长
        let max_len = shards
            .iter()
            .flatten()
            .map(|s| s.len())
            .max()
            .unwrap_or(0);

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

        // 拼接数据分片
        let data: Vec<u8> = padded[..self.params.data_blocks]
            .iter()
            .flatten()
            .flat_map(|s| s.as_slice())
            .copied()
            .collect();

        Ok(data)
    }
}

/// 集成测试位于 `tests/` 目录，由 Cargo 自动发现。
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_roundtrip() {
        let ec = Erasure::new(4, 2).expect("create EC engine");
        let original = b"Hello, MinIO Erasure Coding! This is a test payload.";

        let shards = ec.encode(original).expect("encode");
        assert_eq!(shards.len(), 6);

        // 正常解码 (所有分片在线)
        let all_shards: Vec<Option<Vec<u8>>> = shards.iter().map(|s| Some(s.clone())).collect();
        let decoded = ec.decode(&all_shards).expect("decode");
        assert_eq!(&decoded[..original.len()], original);

        // 模拟 2 个分片丢失 (正好 Quorum 边界)
        let mut partial = shards.clone();
        partial[0] = vec![0u8; partial[0].len()]; // 标记为损坏
        partial[1] = vec![0u8; partial[1].len()];

        let partial_shards: Vec<Option<Vec<u8>>> = partial
            .iter()
            .enumerate()
            .map(|(i, s)| {
                if i < 2 {
                    None
                } else {
                    Some(s.clone())
                }
            })
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
