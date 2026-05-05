# minio-rs 项目规范

## 语言约定

- 代码注解（`//`/`///`/`//!`）用英文，文档（`docs/`、`README`）用中文，标识符用英文
- 写代码前，先把改动和设计更新到文档中

## 项目结构

单 Crate 架构，`src/` 下按模块分目录：

```
src/
├── main.rs          # 入口
├── lib.rs           # 库 root
├── server/          # CLI、启动流程、磁盘检测
├── s3/              # axum HTTP API、handler、XML 响应
├── object/          # ObjectAPI trait、对象编排
├── erasure/         # Reed-Solomon 编解码、bitrot 检测
├── storage/         # StorageAPI trait、本地磁盘驱动、xl.meta 读写
├── base/            # 错误类型、常量、xl.meta 格式定义
├── iam/             # IAM/STS（Phase 3，暂为桩）
└── grid/            # 分布式 RPC（Phase 2，暂为桩）
```

依赖方向：`server` → `s3` → `object` → `erasure` → `storage` → `base`
所有模块通过 `crate::` 前缀引用，Cargo.toml 是全局唯一依赖声明。

## 测试组织

| 测试类型 | 位置 | 说明 |
|---------|------|------|
| 单元测试 | `src/{module}/*.rs` 内 `#[cfg(test)]` | 紧邻源码 |
| 模块测试桩 | `src/{module}/tests/*.rs` | 跨文件但仍在模块内，`#[ignore]` 占位 |
| 集成测试 | `tests/*.rs` | 真正跨模块，启动完整 server 进程 |
