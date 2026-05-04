# minio-rs 架构设计文档

> 对应原 MinIO Go 版本的 `docs/ARCHITECTURE.md` | Rust 重写版

---

## 1. 项目概览

**目标**：用 Rust 重写 MinIO 核心数据路径，深入理解其存储引擎、EC 编码、分布式架构与 IAM 子系统。

**技术栈**：
- 异步运行时：`tokio` (multi-threaded)
- HTTP：`axum` (tower 生态)
- 序列化：`serde` + `rmp-serde` (xl.meta 的 MessagePack 格式)
- 擦除编码：`reed-solomon-erasure` (Reed-Solomon over GF(2^16))

---

## 2. Crate 分层架构

```
┌─────────────────────────────────────────────────┐
│              server (二进制入口)                  │
│  启动流程、信号处理、子系统组装                    │
├─────────────────────────────────────────────────┤
│               s3 (HTTP API 层)                   │
│  axum Router → S3/Admin/STS/KMS 路由             │
│  中间件链: Auth → CORS → Limit → Validate        │
├─────────────────────────────────────────────────┤
│             object (对象操作编排)                  │
│  ObjectAPI trait + erasureObjects 实现            │
│  ← 多池路由 (Phase 2)                             │
├──────────────────┬──────────────────────────────┤
│    erasure       │         iam (Phase 3)         │
│  Reed-Solomon    │  用户/策略/组/STS              │
│  编解码+Quorum    │  JWT 签发与验证                │
├──────────────────┴──────────────────────────────┤
│              storage (存储抽象层)                  │
│  StorageAPI trait                                │
│  ├── xlStorage (本地磁盘)                         │
│  └── storageClient (远程 RPC, Phase 2)            │
├─────────────────────────────────────────────────┤
│              core (核心类型)                       │
│  xl.meta 格式 · EC 参数 · 常量 · 错误类型          │
│  SipHash 路由 · 基础类型                          │
└─────────────────────────────────────────────────┘

         grid (Phase 2) — 分布式 RPC 通信层
         连接各节点间的内部通信
```

### Crate 间依赖关系

```
server ──→ s3, object, erasure, storage, core
s3     ──→ object, iam, core
object ──→ erasure, storage, core
erasure ─→ storage, core
storage ─→ core
iam    ──→ core
grid   ──→ core
```

---

## 3. 数据流 (Phase 1 - 单机 PUT 对象)

```
HTTP (axum)
  ├── 解析 S3 Headers (Authorization, Content-Length, x-amz-*)
  ├── 签名验证 → core::auth (Phase 3)
  └── PutObjectHandler
        │
        ▼
ObjectAPI::put_object(bucket, object, data, metadata)
        │
        ▼
erasureObjects::put_object()
  ├── 计算 EC 参数 (根据 pool/set 的磁盘数)
  ├── 生成 VersionID (UUID v7)
  ├── 构造 xl.meta (版本条目)
  │     ├── 系统元数据 (ModTime, Signature, EC 参数)
  │     ├── 用户元数据 (Content-Type, x-amz-meta-*)
  │     └── Parts 列表 (PartNumber, ETag, Size)
  ├── Erasure::encode(data) → M+N 个 shard
  ├── 并行写入 M+N 个磁盘:
  │     for each disk in erasureSet:
  │       StorageAPI::write_all("bucket/object/uuid/part.N", shard[N])
  │       StorageAPI::write_all("bucket/object/xl.meta", meta_bytes)
  ├── 检查 WriteQuorum
  └── 返回 ObjectInfo { version_id, etag, size }
```

---

## 4. 关键设计决策

| 决策 | Rust 版 | Go 原版 |
|------|---------|---------|
| 全局状态 | `Arc<RwLock<T>>` 或显式注入 | `globalXxx` 包级变量 |
| 错误处理 | `thiserror` 枚举 + `Result` | Go `error` 接口 |
| 接口抽象 | `#[async_trait]` trait | Go `interface` |
| 惰性初始化 | `OnceLock` 或 `LazyLock` | `sync.Once` |
| 并发控制 | `tokio::sync::Semaphore` | `errgroup.Group` |
| 泛型缓存 | 静态分发 (trait bounds) | `cachevalue.Cache[T]` 泛型 |

---

## 5. Phase 1 核心数据结构

### xl.meta 文件格式 (XL V2)

```
Header:  "XL2 " (4B) + Major(2B BE) + Minor(2B BE)  =  8 字节
Body:    MessagePack Array of Version Entries

Version Entry (Type=1, Object):
  VersionID, ModTime, Signature
  ErasureAlgorithm, ErasureM, ErasureN
  ErasureBlockSize, ErasureDist
  MetaSys: [(key, value)]
  MetaUser: [(key, value)]
  Parts: [{Number, ETag, Size, ActualSize, Index}]

Version Entry (Type=2, DeleteMarker):
  VersionID, ModTime, Signature, Flags

Version Entry (Type=3, Legacy):
  占位，直到被覆盖写入时清除
```

### 磁盘目录结构

```
disk/
  .minio.sys/
    config/format.json        ← 磁盘格式化信息
    tmp/                       ← 临时文件（原子 Rename）
    multipart/                 ← Multipart 中间态
  {bucket}/
    {object}/
      xl.meta                  ← MessagePack 二进制版本日志
      xl.meta.bkp              ← 写入前备份
      {version-uuid}/
        part.1                 ← EC 编码数据分片
        part.2
        ...
```

---

## 6. Erasure Coding 参数

| Storage Class | 磁盘数 | Data (M) | Parity (N) |
|--------------|--------|----------|------------|
| STANDARD     | ≤5     | N-2      | 2          |
| STANDARD     | 6-7    | N-3      | 3          |
| STANDARD     | ≥8     | N-4      | 4          |
| REDUCED      | any    | N-1      | 1          |

**Quorum**:
- WriteQuorum = `dataBlocks + 1` (when data > parity)
- WriteQuorum = `dataBlocks` (when data == parity)
- ReadQuorum = `totalDisks - parityBlocks`

---

## 7. 并发模型 (Rust 映射)

| Go 原版 | Rust 替代 | 用途 |
|---------|----------|------|
| `sync.RWMutex` | `tokio::sync::RwLock` | 子系统状态保护 |
| `sync.Once` | `std::sync::OnceLock` | 惰性初始化 |
| `singleflight.Group` | `arc-swap` + 去重 channel | 合并并发缓存填充 |
| `errgroup.Group` | `futures::join_all` + 错误收集 | 并行磁盘操作 |
| `xsync.MapOf[K,V]` | `dashmap::DashMap` | 高并发无锁读 |
| `atomic.Uint64` | `std::sync::atomic::AtomicU64` | 无锁计数器 |
