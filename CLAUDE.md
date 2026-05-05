# minio-rs 项目规范

## 语言约定

- **代码注解**（`.rs` 文件中的 `//`、`///`、`//!` 注释）：使用英文
- **文档**（`docs/*.md`、`README.md`）：保持中文
- **Cargo.toml 元数据**（`description` 等字段）：使用英文
- 代码标识符（函数名、变量名、类型名）始终使用英文
- 写代码前，先把改动和设计更新到文档中

## 项目结构

```
src/                # 主程序（网络层、启动流程、入口点）
├── main.rs         # 二进制入口
├── lib.rs          # 库入口（供集成测试引用）
├── cmd.rs          # CLI 定义 (clap)
├── server.rs       # 启动流程编排
├── disk.rs         # 磁盘检测
└── banner.rs       # 启动 Banner

base/               # xl.meta 格式、EC 参数、存储常量
storage/            # StorageAPI trait + 本地磁盘实现
erasure/            # Reed-Solomon 编解码
object/             # ObjectAPI trait + 对象操作编排
iam/                # IAM/STS 子系统
grid/               # 分布式 RPC
s3/                 # S3 HTTP API (axum)

tests/              # 集成测试
docs/               # 文档
```
