//! base: 核心类型与常量
//!
//! 本 crate 定义 MinIO 存储系统的共享基础类型，对应原 Go 项目中
//! `cmd/` 下的核心数据结构与常量定义。
//!
//! # 模块概览
//!
//! - `format`: xl.meta 磁盘格式的 Rust 结构体定义 (MessagePack 序列化)
//! - `erasure`: EC 编码参数 (dataBlocks, parityBlocks, blockSize)
//! - `constants`: 全局常量 (smallFileThreshold, bigFileThreshold 等)
//! - `error`: 统一错误类型
//! - `types`: 基础类型 (VersionID, ObjectKey, ETag 等)

pub mod constants;
pub mod error;
pub mod format;
pub mod types;

// Phase 1 需要的子模块，后续按需展开
pub mod erasure {
    //! Erasure Coding 参数定义
    //!
    //! 对应 Go: cmd/erasure-coding.go

    /// EC 编码配置
    #[derive(Debug, Clone)]
    pub struct ErasureParams {
        /// 数据块数量 (M)
        pub data_blocks: usize,
        /// 校验块数量 (N)
        pub parity_blocks: usize,
        /// EC 块大小 (默认 4 MiB)
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
        /// 总分片数 M + N
        pub fn total_shards(&self) -> usize {
            self.data_blocks + self.parity_blocks
        }

        /// 写入 Quorum: dataBlocks + 1 (data > parity 时)
        pub fn write_quorum(&self) -> usize {
            if self.data_blocks == self.parity_blocks {
                self.data_blocks
            } else {
                self.data_blocks + 1
            }
        }

        /// 读取 Quorum: totalShards - parityBlocks
        pub fn read_quorum(&self) -> usize {
            self.total_shards() - self.parity_blocks
        }
    }
}

pub mod hashing {
    //! 对象路由哈希
    //!
    //! 对应 Go: cmd/utils.go sipHashMod

    /// 使用 SipHash-2-4 将对象名映射到 Set 索引
    ///
    /// 对应 Go: `sipHashMod(objectName, numSets)`
    /// 该算法确定性、均匀分布、高吞吐，用于分布式模式下的对象路由。
    pub fn sip_hash_mod(key: &[u8], num_sets: usize) -> usize {
        use std::hash::Hasher;
        let mut hasher = siphasher::sip::SipHasher24::new();
        hasher.write(key);
        (hasher.finish() % num_sets as u64) as usize
    }
}
