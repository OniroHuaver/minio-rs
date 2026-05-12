//! 随机测试数据生成器 — 无外部依赖，纯内存数据源。
//!
//! 支持三种对象大小策略：
//! 1. 固定大小 (`obj.size`)
//! 2. 随机大小 (`obj.randsize`) — log2 分布
//! 3. 分桶大小 (`obj.size` 格式 `4096:10740,8192:1685,...`)

use rand::Rng;
use std::fmt;
use std::io::{self, Cursor, Read, Seek, SeekFrom};

/// Supertrait combining Read + Seek for trait objects
pub trait ReadSeek: Read + Seek + Send {}
impl<T: Read + Seek + Send> ReadSeek for T {}

// ---------------------------------------------------------------------------
// Crate-level error type (re-exported for convenience)
// ---------------------------------------------------------------------------
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("S3 error: {0}")]
    S3(String),
    #[error("CSV error: {0}")]
    Csv(String),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("Zstd error: {0}")]
    Zstd(String),
    #[error("Benchmark error: {0}")]
    Bench(String),
    #[error("Parse error: {0}")]
    Parse(String),
}

// ---------------------------------------------------------------------------
// LastByte trait — 报告写入数据流最后一个字节的位置
// ---------------------------------------------------------------------------
pub trait LastByte {
    /// 数据的最后一个有效字节偏移量（含）。用于精确计算上传字节数。
    fn last_byte(&self) -> Option<u64>;
}

// ---------------------------------------------------------------------------
// Object — 单个测试对象
// ---------------------------------------------------------------------------
pub struct Object {
    pub reader: Box<dyn ReadSeek>,
    pub name: String,
    pub content_type: String,
    pub prefix: String,
    pub version_id: String,
    pub size: i64,
    pub last_byte: Option<u64>,
}

impl fmt::Debug for Object {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Object")
            .field("name", &self.name)
            .field("content_type", &self.content_type)
            .field("prefix", &self.prefix)
            .field("size", &self.size)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Source trait — 对象工厂
// ---------------------------------------------------------------------------
pub trait Source: Send + Sync {
    fn object(&mut self) -> Object;
    fn prefix(&self) -> &str;
    fn set_prefix(&mut self, prefix: String);
}

// ---------------------------------------------------------------------------
// ObjSize — 对象大小策略
// ---------------------------------------------------------------------------
#[derive(Debug, Clone)]
pub enum ObjSize {
    /// 固定大小
    Fixed(i64),
    /// log2 随机分布，平均大小 ≈ max × 0.179151
    Random { max: i64 },
    /// 分桶: Vec<(size, weight)>
    Bucketed { buckets: Vec<(i64, u64)>, total_weight: u64 },
}

impl ObjSize {
    /// 从 `--obj.size` 参数解析
    pub fn parse(s: &str) -> std::result::Result<Self, String> {
        if let Some(inner) = s.strip_prefix("rand:") {
            let max: i64 = inner
                .parse()
                .map_err(|e| format!("invalid random suffix max bytes: {e}"))?;
            return Ok(Self::Random { max });
        }
        if s.contains(':') {
            let mut buckets = Vec::new();
            let mut total_weight = 0u64;
            for part in s.split(',') {
                let (size_s, weight_s) = part
                    .split_once(':')
                    .ok_or_else(|| format!("invalid bucket spec `{part}`"))?;
                let size: i64 = size_s
                    .parse()
                    .map_err(|e| format!("invalid object size in bucket: {e}"))?;
                let weight: u64 = weight_s
                    .parse()
                    .map_err(|e| format!("invalid bucket weight: {e}"))?;
                total_weight += weight;
                buckets.push((size, weight));
            }
            return Ok(Self::Bucketed {
                buckets,
                total_weight,
            });
        }
        let size: i64 = s
            .parse()
            .map_err(|e| format!("invalid fixed object size: {e}"))?;
        Ok(Self::Fixed(size))
    }

    /// 生成一个对象大小
    pub fn gen(&self, rng: &mut impl Rng) -> i64 {
        match self {
            Self::Fixed(s) => *s,
            Self::Random { max } => {
                // log2 分布：min=1, max=max, 平均 ≈ max * 0.179151
                let bits = (max + 1).ilog2();
                let v: u64 = rng.gen_range(0..(1u64 << bits));
                (v + 1) as i64
            }
            Self::Bucketed {
                buckets,
                total_weight,
            } => {
                let mut w: u64 = rng.gen_range(0..*total_weight);
                for (size, weight) in buckets {
                    if w < *weight {
                        return *size;
                    }
                    w -= *weight;
                }
                buckets.last().map(|(s, _)| *s).unwrap_or(1024)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 随机数据 Reader（基于种子生成伪随机字节，可 Seek）
// ---------------------------------------------------------------------------
pub struct RandomReader {
    size: i64,
    pos: u64,
    seed: u64,
}

impl RandomReader {
    pub fn new(size: i64, seed: u64) -> Self {
        Self { size, pos: 0, seed }
    }

    fn fill_at(&self, offset: u64, buf: &mut [u8]) {
        // 使用简单的 xorshift + offset 生成确定性伪随机数据
        let mut state = self.seed.wrapping_mul(6364136223846793005).wrapping_add(offset);
        for byte in buf.iter_mut() {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *byte = (state >> 8) as u8;
        }
    }
}

impl Read for RandomReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let remaining = (self.size as u64).saturating_sub(self.pos);
        let n = (buf.len() as u64).min(remaining) as usize;
        if n == 0 {
            return Ok(0);
        }
        self.fill_at(self.pos, &mut buf[..n]);
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for RandomReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let new_pos = match pos {
            SeekFrom::Start(p) => p as i64,
            SeekFrom::End(p) => self.size + p,
            SeekFrom::Current(p) => self.pos as i64 + p,
        };
        if new_pos < 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "negative seek offset"));
        }
        self.pos = new_pos as u64;
        Ok(self.pos)
    }
}

impl LastByte for RandomReader {
    fn last_byte(&self) -> Option<u64> {
        if self.size > 0 {
            Some(self.size as u64 - 1)
        } else {
            None
        }
    }
}

impl LastByte for Cursor<Vec<u8>> {
    fn last_byte(&self) -> Option<u64> {
        let len = self.get_ref().len();
        if len > 0 {
            Some(len as u64 - 1)
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// 默认 Source 实现
// ---------------------------------------------------------------------------
pub struct DefaultSource {
    prefix: String,
    obj_size: ObjSize,
    counter: u64,
    seed: u64,
    rng: rand::rngs::SmallRng,
}

impl DefaultSource {
    pub fn new(prefix: String, obj_size: ObjSize, seed: u64) -> Self {
        use rand::SeedableRng;
        Self {
            prefix,
            obj_size,
            counter: 0,
            seed,
            rng: rand::rngs::SmallRng::seed_from_u64(seed),
        }
    }
}

impl Source for DefaultSource {
    fn object(&mut self) -> Object {
        let size = self.obj_size.gen(&mut self.rng);
        let obj_seed = self.seed.wrapping_add(self.counter);
        let name = format!("{}.{:016x}.data", self.prefix, self.counter);
        self.counter += 1;

        let reader = RandomReader::new(size, obj_seed);

        Object {
            reader: Box::new(reader),
            name,
            content_type: "application/octet-stream".into(),
            prefix: self.prefix.clone(),
            version_id: String::new(),
            size,
            last_byte: if size > 0 { Some(size as u64 - 1) } else { None },
        }
    }

    fn prefix(&self) -> &str {
        &self.prefix
    }

    fn set_prefix(&mut self, prefix: String) {
        self.prefix = prefix;
    }
}

// ---------------------------------------------------------------------------
// 多前缀 Source — 每个线程使用独立前缀
// ---------------------------------------------------------------------------
pub struct MultiPrefixSource {
    sources: Vec<DefaultSource>,
    next: usize,
}

impl MultiPrefixSource {
    pub fn new(base_prefix: String, obj_size: ObjSize, count: usize, seed: u64) -> Self {
        let sources: Vec<_> = (0..count)
            .map(|i| {
                let p = if count > 1 {
                    format!("{base_prefix}-{i}")
                } else {
                    base_prefix.clone()
                };
                DefaultSource::new(p, obj_size.clone(), seed.wrapping_add(i as u64))
            })
            .collect();
        Self { sources, next: 0 }
    }
}

impl Source for MultiPrefixSource {
    fn object(&mut self) -> Object {
        let idx = self.next;
        self.next = (self.next + 1) % self.sources.len();
        self.sources[idx].object()
    }

    fn prefix(&self) -> &str {
        self.sources[0].prefix()
    }

    fn set_prefix(&mut self, _prefix: String) {
        // no-op: multi-prefix 模式不支持运行时改前缀
    }
}
