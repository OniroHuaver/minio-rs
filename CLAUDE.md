# minio-rs 项目规范

## 语言约定

- **代码注解**（`.rs` 文件中的 `//`、`///`、`//!` 注释）：使用英文
- **文档**（`docs/*.md`、`README.md`）：保持中文
- **Cargo.toml 元数据**（`description` 等字段）：使用英文
- 代码标识符（函数名、变量名、类型名）始终使用英文
- 写代码前，先把改动和设计更新到文档中

## 项目结构

单 Crate 架构，所有代码在 `src/` 下按模块分目录：

```
minio-rs/
├── Cargo.toml                # 全局唯一依赖声明
├── README.md
│
├── docs/                     # 文档（snake_case 命名）
│   ├── architecture.md
│   ├── api_reference.md
│   ├── storage_spec.md
│   ├── s3_spec.md
│   ├── server_spec.md
│   └── ...
│
├── tests/                    # 跨模块集成测试
│   ├── common/               # 测试辅助
│   └── integration.rs
│
└── src/                      # 纯净代码区
    ├── main.rs               # 极薄启动层
    ├── lib.rs                # 唯一库入口
    │
    ├── server/               # 启动流程
    │   ├── mod.rs
    │   ├── cmd.rs            # CLI 定义 (clap)
    │   ├── run.rs            # 启动流程编排
    │   ├── disk.rs           # 磁盘检测
    │   └── banner.rs         # 启动 Banner
    │
    ├── base/                 # 核心类型、常量、错误、xl.meta 格式
    │   ├── mod.rs
    │   ├── error.rs          # MinioError 统一错误
    │   ├── format.rs         # xl.meta 二进制格式
    │   ├── constants.rs
    │   └── types.rs
    │
    ├── storage/              # 存储抽象层
    │   ├── mod.rs            # StorageAPI trait + DiskInfo
    │   ├── xl_storage.rs     # 本地磁盘驱动
    │   ├── format.rs         # xl.meta 读写工具
    │   └── tests/            # 模块内测试桩 (#[ignore])
    │
    ├── erasure/              # 纠删码引擎
    │   ├── mod.rs            # Erasure (encode/decode)
    │   ├── bitrot.rs         # 静默损坏检测
    │   └── tests/            # 模块内测试桩 (#[ignore])
    │
    ├── object/               # 对象操作编排层
    │   ├── mod.rs            # ObjectAPI trait + ErasureObjects
    │   ├── object_api.rs
    │   ├── erasure_objects.rs
    │   ├── set.rs            # ErasureSet 并行分片 I/O
    │   └── tests/            # 模块内测试桩 (#[ignore])
    │
    ├── s3/                   # S3 HTTP API 层
    │   ├── mod.rs
    │   ├── router.rs         # axum Router
    │   ├── state.rs          # AppState
    │   ├── error.rs          # S3 错误映射
    │   ├── request.rs
    │   ├── response.rs       # XML 响应构建
    │   ├── handlers/         # S3 handler 实现
    │   └── tests/            # 模块内测试桩 (#[ignore])
    │
    ├── iam/                  # 认证授权（Phase 3）
    │   ├── mod.rs
    │   └── tests/            # 模块内测试桩 (#[ignore])
    │
    └── grid/                 # 分布式 RPC（Phase 2）
        ├── mod.rs
        └── tests/            # 模块内测试桩 (#[ignore])
```

## 模块依赖

```
s3 ────────→ object ──→ erasure ──→ storage ──→ base
  │                         │
  └──→ iam                  └──→ storage
```

所有模块通过 `crate::` 前缀引用，无 crate 边界。Cargo.toml 是全局唯一的依赖声明。

## 测试组织

| 测试类型 | 位置 | 说明 |
|---------|------|------|
| 单元测试 | `src/{module}/*.rs` 内 `#[cfg(test)]` | 紧邻源码 |
| 模块测试桩 | `src/{module}/tests/*.rs` | 跨文件但仍在模块内，`#[ignore]` 占位 |
| 集成测试 | `tests/*.rs` | 真正跨模块，启动完整 server 进程 |
