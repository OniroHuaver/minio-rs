# minio-rs

MinIO 对象存储的 Rust 学习版重写。

## 目标

通过用 Rust 逐层重写 MinIO 核心数据路径，深入理解：
- **XL Storage Format V2** — 磁盘格式与 MessagePack 版本日志
- **Erasure Coding** — Reed-Solomon 编解码与 Quorum 判定
- **分布式架构** — 无共享节点通信 (gRPC)、分布式锁、多池路由
- **S3 API** — AWS SigV4 签名、IAM 策略评估、STS 临时凭证

## 项目结构

```
minio-rs/
├── crates/
│   ├── base/         # xl.meta 格式、EC 参数、存储常量
│   ├── storage/      # StorageAPI trait + 本地磁盘实现
│   ├── erasure/      # Reed-Solomon 编解码
│   ├── object/       # ObjectAPI trait + 对象操作编排
│   ├── iam/          # IAM/STS 子系统 (Phase 3)
│   ├── grid/         # 分布式 RPC (Phase 2)
│   ├── s3/           # S3 HTTP API (axum)
│   └── server/       # 二进制入口
├── docs/
│   ├── ARCHITECTURE.md   # 架构设计
│   ├── PLAN.md           # 分阶段实施计划
│   └── STORAGE_SPEC.md   # 存储格式规格
└── tests/
```

## 进度

| Phase | 内容 | 状态 |
|-------|------|------|
| 0     | 项目骨架 + 文档 | ✅ 已完成 |
| 1     | 单机核心存储引擎 | 🔴 待开始 |
| 2     | 分布式模式 | 🔴 待开始 |
| 3     | IAM + STS | 🔴 待开始 |
| 4     | 高级特性 | 🔴 待开始 |

## 快速开始

```bash
# 构建
cargo build

# 运行
cargo run -p server

# 测试
cargo test --workspace
```

## 参考

- [MinIO 原版架构文档](../minio/docs/ARCHITECTURE.md)
- [MinIO 存储格式 spec](../minio/docs/STORAGE_IAM_SPEC.md)
- [AWS S3 API Reference](https://docs.aws.amazon.com/AmazonS3/latest/API/)
