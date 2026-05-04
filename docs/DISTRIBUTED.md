# MinIO 分布式存储与数据处理

> 本文档整合 MinIO 原版文档中核心存储与分布式处理相关文档，从 Rust 重写视角提炼设计规格与实现要点。

---

## 1. 分布式架构设计 (来自 distributed/CONFIG.md)

**YAML 配置启动**

MinIO 支持通过 YAML 配置文件 `minio server --config config.yaml` 替代传统命令行椭圆扩号语法启动，支持异构主机名。当前配置 schema 版本为 `v2`。

```yaml
version: v2
address: ":9000"
rootUser: "minioadmin"
rootPassword: "minioadmin"
console-address: ":9001"
certs-dir: "/home/user/.minio/certs/"
pools:
  - args:
      - "https://server-example-pool1:9000/mnt/disk{1...4}/"
      - "https://server{1...2}-pool1:9000/mnt/disk{1...4}/"
    set-drive-count: 4
options:
  ftp:
    address: ":8021"
    passive-port-range: "30000-40000"
  sftp:
    address: ":8022"
    ssh-private-key: "/home/user/.ssh/id_rsa"
```

**Pool 架构**

- 每个 Pool 至少 2 个节点，每个节点使用相同数量的驱动器
- 不允许混合 `local-path` 和 `distributed-path`
- 支持椭圆扩号语法（`{1...4}`）和 bracket notation（`{a,c,f}`）
- 环境变量优先级 > `config.yaml` > 内部 KV 配置

**规划中的功能**

- 运行时 `Reload()` 配置无需重启
- 单节点逐步扩容（自动创建新 Pool + 退役旧节点）
- 完全异构主机名支持

**Rust 实现要点（Phase 1）**

- `Config` 结构体直接映射 YAML schema，使用 `serde` + `serde_yaml` 反序列化
- `Pool` 结构体包含 `args: Vec<String>` 和可选 `set_drive_count`
- 支持 `Pools` 配置的热加载（`Arc<RwLock<Config>>` + `tokio::watch`）
- 椭圆扩号语法解析器支持 `{m..n}` 和 `{a,b,c}` 展开

---

## 2. 擦除编码详解 (来自 erasure/)

### 2.1 基本原理

MinIO 使用 **Reed-Solomon 纠删码** 将对象分片为数据块（Data Shards）和奇偶校验块（Parity Shards），保护数据免受硬件故障和静默数据损坏（Bit Rot）。默认 N/2 数据 + N/2 奇偶校验配置，最多容忍 N/2 个驱动器同时故障。支持两种 Storage Class：`STANDARD`（默认 N/2 奇偶校验）和 `REDUCED_REDUNDANCY`（默认 2 奇偶校验）。

**Bit Rot 防护**：使用 HighwayHash 校验和检测静默数据损坏。

### 2.2 擦除集 (Erasure Set)

MinIO 将驱动器划分为擦除集，每个集合包含 **2 到 16 个** 驱动器。驱动器总数必须为集合大小的倍数。每个对象写入一个擦除集中。

- 18 个驱动器 → 2 个 9 驱动器集合
- 24 个驱动器 → 2 个 12 驱动器集合
- 分布式模式下，擦除条带大小基于节点亲和性选择

驱动器应大致同大小。

### 2.3 Storage Class 配置

| 驱动器总数 | 数据块 | 奇偶校验块 | 存储放大比 |
|-----------|--------|-----------|-----------|
| 16 | 8 | 8 | 2.00 |
| 16 | 10 | 6 | 1.60 |
| 16 | 12 | 4 | 1.34 |
| 16 | 14 | 2 | 1.14 |

**默认 Parity 配置**：

| 擦除集大小 | 默认 Parity (STANDARD) |
|-----------|----------------------|
| <=5 | EC:2 |
| 6-7 | EC:3 |
| >=8 | EC:4 |

配置通过 `MINIO_STORAGE_CLASS_STANDARD=EC:parity` / `MINIO_STORAGE_CLASS_RRS=EC:parity` 环境变量或 `mc admin config set` 设置。对象通过请求头 `x-amz-storage-class` 指定存储类。

### 2.4 Rust 实现要点（Phase 1 核心）

**Reed-Solomon 实现选择**

- 推荐使用纯 Rust 的 `reed-solomon-erasure` crate（基于 Cauchy / Vandermonde 矩阵），或绑定 `klauspost/reedsolomon`（CGo）。优先纯 Rust 方案确保跨编译和性能
- HighwayHash 替代：使用 `highway-rs` crate 实现 checksum

**数据布局**

- `ErasureSet` 结构体管理一组驱动器，负责对象的 shard 映射
- 写入：对象 → 分片 → 计算奇偶校验 → 并行写入各驱动器
- 读取：从 quorum 驱动器读取 → 校验 → 如有损坏启动重建
- 修复：检测到损坏或驱动器离线时，触发在线重建

**关键抽象**

```rust
pub struct ErasureSet {
    data_shards: usize,
    parity_shards: usize,
    drives: Vec<Arc<dyn Drive>>,
}

pub trait Drive: Send + Sync {
    async fn read(&self, shard: &ShardId) -> Result<Vec<u8>>;
    async fn write(&self, shard: &ShardId, data: &[u8]) -> Result<()>;
    fn status(&self) -> DriveStatus;
}
```

**Phase 1**：实现单节点擦除编码的读写路径，支持 N/2 默认配置
**Phase 2**：Storage Class 自定义、在线修复

---

## 3. S3 Select 引擎 (来自 select/)

### 3.1 功能概述

S3 Select API 允许用户使用 SQL 表达式从对象中检索子集数据，避免全量读取。支持以下输入格式：

- **CSV**：UTF-8 编码，GZIP/BZIP2/ZSTD/LZ4/S2/Snappy 压缩
- **JSON**（行分隔）：同 CSV 支持的所有压缩
- **Parquet**（默认禁用，需 `MINIO_API_SELECT_PARQUET=on`）：列式压缩支持 GZIP/Snappy/LZ4

### 3.2 SQL 支持

完全支持 AWS S3 SELECT SQL 语法：
- 所有操作符（比较、逻辑、算术）
- 聚合函数（COUNT、SUM、AVG、MIN、MAX）
- 条件函数（CASE、COALESCE、NULLIF）
- 类型转换（CAST）
- 日期函数（DATE_ADD、DATE_DIFF、EXTRACT、UTCNOW）
- 字符串函数

**限制**：
- JSON path 表达式（`FROM S3Object[*].path`）未实现
- signed 64-bit 范围外的超大数未支持
- CSV input 记录 >1MiB 拒绝（`OverMaxRecordSize`）
- CSV 字段（含引号）内不能包含换行符
- AWS S3 保留关键字列表未处理

### 3.3 Rust 实现建议（Phase 2-3）

**SQL 解析与执行**

- SQL 解析使用 `sqlparser` crate（支持 PostgreSQL/ANSI 方言子集）
- 执行引擎实现为流式迭代器模式，避免全量加载到内存
- CSV 解析使用 `csv` crate + 零拷贝 deserialize
- JSON 行解析使用 `simd_json` 或 `serde_json` 流式解析
- Parquet 使用 `parquet` crate（Apache Arrow 生态）

**架构分层**

```
SelectRequest (Expression + InputSerialization + OutputSerialization)
    -> SQL Parser (sqlparser)
    -> Logical Plan (Expr -> Filter/Projection/Aggregate)
    -> Physical Plan (ObjectReader -> RowIterator -> Filter -> Project)
    -> Output Serializer (CSV/JSON output format)
```

**性能关注点**

- 使用 `simd` 优化 CSV 字段扫描
- Parquet 默认禁用（与 Go 版一致），需显式开启
- 流式输出：使用 Tokio channel 将处理后的行发送到 HTTP response body

**Phase 2** 实现 CSV 和 JSON 行格式的基础 SQL Select
**Phase 3** 实现 Parquet、JSON path、日期函数等高级功能

---

## 4. Rust 实现路线图

| Phase | 模块 | 里程碑 |
|-------|------|--------|
| **Phase 1** | 擦除编码核心 + YAML 配置 | 单节点单盘能读写文件 |
| **Phase 1.5** | 多盘擦除编码集 | 支持跨多盘 EC 写入读取 |
| **Phase 2** | Storage Class 自定义 + BitRot 校验 | 支持 STANDARD/RRS 配置 |
| **Phase 2** | S3 Select CSV/JSON 基础 SQL | 支持简单 WHERE 投影查询 |
| **Phase 2** | 分布式配置与 Pool 管理 | 支持 config.yaml 多 pool |
| **Phase 3** | 在线修复 (Healing) + Scanner | 自动检测并修复损坏 shard |
| **Phase 3** | S3 Select Parquet + 高级 SQL | 完整 S3 Select 兼容 |
| **Phase 3** | 运行时配置 Reload | 无重启动态扩缩容 |

**架构依赖**

- Phase 1 的擦除编码核心是所有 Phase 的基础，必须先完成
- Storage Class 依赖擦除编码的分片参数化
- S3 Select 依赖文件读取 IO 层，可并行开发
- 在线修复依赖 Scanner，Scanner 依赖擦除编码的校验和能力
