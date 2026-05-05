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

## 2. 模块分层架构

单 Crate 架构，所有模块在 `src/` 下通过 `crate::` 引用。无 crate 边界，无工作空间。

```
┌─────────────────────────────────────────────────┐
│             src/server (启动入口)                 │
│  CLI 解析、磁盘检测、EC 池初始化、HTTP Server     │
├─────────────────────────────────────────────────┤
│               src/s3 (HTTP API 层)               │
│  axum Router → S3 路由                          │
│  中间件链: Trace → CORS                         │
├─────────────────────────────────────────────────┤
│             src/object (对象操作编排)              │
│  ObjectAPI trait + ErasureObjects 实现            │
├──────────────────┬──────────────────────────────┤
│   src/erasure    │       src/iam (Phase 3)       │
│  Reed-Solomon    │  用户/策略/组/STS              │
│  编解码+Quorum    │  JWT 签发与验证                │
├──────────────────┴──────────────────────────────┤
│             src/storage (存储抽象层)               │
│  StorageAPI trait + xlStorage 本地磁盘实现        │
├─────────────────────────────────────────────────┤
│              src/base (核心类型)                   │
│  xl.meta 格式 · EC 参数 · 常量 · 错误类型          │
│  SipHash 路由 · 基础类型                          │
└─────────────────────────────────────────────────┘

       src/grid (Phase 2) — 分布式 RPC 通信层
```

### 模块间依赖关系

```
s3 ────────→ object ──→ erasure ──→ storage ──→ base
  │                         │
  └──→ iam                  └──→ storage
```

所有引用统一使用 `crate::` 前缀（如 `use crate::base::error::MinioError`）。

---

## 3. 数据流 (Phase 1 - 单机 PUT 对象)

```
HTTP (axum)
  ├── 解析 S3 Headers (Authorization, Content-Length, x-amz-*)
  ├── 签名验证 → base::auth (Phase 3)
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
