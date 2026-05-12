# minio-rs

MinIO 对象存储的 Rust 学习版重写。

## 目标

通过 Rust 逐层重写 MinIO 核心数据路径，深入理解：
- **XL Storage Format V2** — 磁盘格式与 MessagePack 版本日志
- **Erasure Coding** — Reed-Solomon 编解码与 Quorum 判定
- **分布式架构** — 无共享节点通信、分布式锁、多池路由
- **S3 API** — AWS SigV4 签名、IAM 策略评估、STS 临时凭证

## 结构

单 Crate，`src/` 下按模块分层：`server` → `s3` → `object` → `erasure` → `storage` → `base`。文档在 `docs/`（snake_case），集成测试在 `tests/`。

## 进度

| Phase | 内容 | 状态 |
|-------|------|------|
| 0     | 项目骨架 + 文档整合 | ✅ 已完成 |
| 1     | 单机核心存储引擎 | 🔴 待开始 |
| 2     | 分布式模式 | 🔴 待开始 |
| 3     | IAM + STS | 🔴 待开始 |
| 4     | 高级特性 | 🔴 待开始 |

## 快速开始

```bash
cargo build
cargo run
cargo test
```

## 参考

- [MinIO 原版架构文档](../minio/docs/ARCHITECTURE.md)
- [MinIO 原版存储/IAM spec](../minio/docs/STORAGE_IAM_SPEC.md)
- [AWS S3 API Reference](https://docs.aws.amazon.com/AmazonS3/latest/API/)
