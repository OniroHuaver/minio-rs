# Server 启动流程设计文档

> 本文档描述 minio-rs 二进制入口 `minio server` 的启动流程、模块划分、关键设计决策与实现顺序。

---

## 1. 设计目标

- **可验证**：每个启动阶段可以独立测试（磁盘检测、EC 池构建、HTTP serve）。
- **最小依赖**：只引入必要的依赖（clap、axum），不引入大型框架。
- **单机优先**：Phase 1 只解决单机多盘场景，Phase 2 再引入分布式拓扑发现。
- **可观测**：启动全程输出结构化日志，banner 信息清晰可读。

---

## 2. CLI 入口与参数解析

### 2.1 选择 clap 的理由

| 方案 | 评价 |
|------|------|
| **clap (derive)** | Rust 生态事实标准；编译时生成帮助/补全；与 `axum` 无冲突；仅增加 ~0.3s 编译时间 |
| 手写 `std::env::args()` | 可维护性差；无自动帮助；需要手写参数校验；**不推荐** |
| `argh` / `gumdrop` | 更轻量但社区偏小；功能集不够完成（无子命令嵌套） |

**结论**：使用 `clap`（derive API），定义 `server` 子命令。

### 2.2 CLI 结构

```
minio server [OPTIONS] <DISKS>...

Arguments:
  <DISKS>...  磁盘路径列表，至少 3 个（或 1 个，开发模式）

Options:
  -a, --address <ADDR>           HTTP 监听地址 [default: 0.0.0.0:9000]
  -c, --console-address <ADDR>   控制台监听地址 [default: 无，Phase 2]
  -h, --help                     Print help
  -V, --version                  Print version
```

### 2.3 代码位置

```
crates/server/src/
├── cmd.rs          ← CLI 定义（clap derive struct）
├── main.rs         ← 最小入口：初始化 tracing → 解析 CLI → 调用 cmd::run()
└── lib.rs          ← 已有模块骨架
```

`cmd.rs` 核心结构：

```rust
/// minio-rs server — 高性能 S3 兼容对象存储
#[derive(Parser)]
#[command(name = "minio", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start the object storage server
    Server {
        /// HTTP/HTTPS listening address
        #[arg(short, long, default_value = "0.0.0.0:9000")]
        address: String,

        /// Console (web UI) listening address
        #[arg(short = 'C', long = "console-address")]
        console_address: Option<String>,

        /// Disk paths (at least 3 for production, 1 for dev/test)
        #[arg(required = true)]
        disks: Vec<String>,
    },
}
```

### 2.4 设计决策

- 为什么不用 `clap` 的多值参数 + `--address` 直接挂在 `minio` 顶层？因为原版 MinIO 有 `server` / `gateway` / `admin` 等多个子命令，保持 `minio server` 结构为后续扩展留空间。
- `console-address` 标记为 `Option`：Phase 1 不实现控制台，但命令行参数先占位，解析后打印提示信息。
- 磁盘参数设为 `required = true`，clap 会自动输出错误信息。

---

## 3. 模块划分与启动流程

### 3.1 启动阶段总览

```
main()
  │
  ├── (1) 初始化 tracing-subscriber 日志
  │
  ├── (2) 解析 CLI 参数（clap）
  │
  ├── (3) 磁盘检测 — checkDisks()
  │     ├── 遍历磁盘路径
  │     ├── 检查路径存在/可读写
  │     ├── 检查 .minio.sys/format.json
  │     ├── 自动创建 .minio.sys/tmp、.minio.sys/multipart
  │     └── 汇总 DiskInfo 列表
  │
  ├── (4) EC 池初始化 — initErasureSet()
  │     ├── 计算 EC 参数（Erasure::with_default_parity）
  │     ├── 创建 Arc<XlStorage> 实例列表
  │     └── 创建 ErasureSet → ErasureObjects
  │
  ├── (5) 构建 AppState
  │
  ├── (6) 注册 S3 HTTP 路由（axum Router）
  │
  ├── (7) 打印启动 Banner
  │
  └── (8) axum::serve + 信号等待 + 优雅关闭
```

### 3.2 代码组织

```
crates/server/src/
├── main.rs        ← 入口
├── lib.rs         ← 模块声明、AppState、ServerConfig、启动入口
├── cmd.rs         ← clap CLI 定义
├── server.rs      ← Server 结构体 + run() 方法（组装所有步骤）
├── disk.rs        ← 磁盘检测逻辑
└── banner.rs      ← 启动 Banner 输出
```

---

## 4. 日志初始化

### 4.1 配置

```rust
// main.rs
tracing_subscriber::fmt()
    .with_env_filter(
        EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("info"))
    )
    .with_target(true)
    .with_file(true)
    .with_line_number(true)
    .init();
```

### 4.2 设计决策

- 使用 `tracing-subscriber` 的 `fmt` 层 + `EnvFilter`，默认 `info` 级别。
- 环境变量 `RUST_LOG` 可覆盖：`RUST_LOG=debug minio server /disk1 /disk2 /disk3`。
- 启动阶段的关键日志使用 `info!`（正常状态）和 `warn!`（降级状态，如磁盘格式化状态不一致）。
- **不启用 JSON 日志**（Phase 1 不需要生产环境日志管道）。

---

## 5. 磁盘检测

### 5.1 检测步骤

```
for each disk_path in disks:
    1. tokio::fs::metadata(disk_path).await
       - 不存在：创建目录（一次 mkdir_all）
       - 不可读/写：返回错误（致命，启动失败）
    
    2. Check .minio.sys/format.json
       - 存在且有效 → DiskInfo.formatted = true
       - 不存在     → DiskInfo.formatted = false
    
    3. 自动创建 .minio.sys/tmp/ 和 .minio.sys/multipart/
       （MinIO 行为：即使未格式化也创建临时目录）
```

### 5.2 返回值

```rust
/// 磁盘检测结果
pub struct CheckedDisk {
    pub path: PathBuf,
    pub disk_info: DiskInfo,
    pub xl_storage: Arc<XlStorage>,
}
```

### 5.3 设计决策

- **错误即终止**：磁盘路径不存在且自动创建失败 → 直接 `process::exit(1)`。MinIO 原版也是立即退出，不在退化模式下启动。
- **格式化检查不阻断启动**：`format.json` 不存在仅标记 `formatted: false`，启动仍继续。真正的格式化流程属于 Phase 2 的 `HealFormat`。
- **`Arc<XlStorage>` 在此阶段创建**：因为后续 `ErasureSet::new()` 需要 `Vec<Arc<dyn StorageAPI>>`。

---

## 6. EC 池初始化

### 6.1 流程

```rust
fn init_erasure_objects(disks: Vec<Arc<dyn StorageAPI>>) -> MinioResult<Arc<ErasureObjects>> {
    // (1) 参数校验
    let total = disks.len();
    if total < 3 {
        return Err("at least 3 disks required");
    }

    // (2) 创建 ErasureObjects（内部自动计算 parity）
    let objects = ErasureObjects::new(disks)?;
    Ok(Arc::new(objects))
}
```

### 6.2 自动 EC 参数计算

| 磁盘数 | Data | Parity | WriteQuorum | ReadQuorum | 存储效率 |
|--------|------|--------|-------------|------------|----------|
| 3      | 1    | 2      | 2           | 1          | 33%      |
| 4      | 2    | 2      | 3           | 2          | 50%      |
| 5      | 3    | 2      | 4           | 3          | 60%      |
| 6      | 3    | 3      | 3           | 3          | 50%      |
| 7      | 4    | 3      | 5           | 4          | 57%      |
| 8      | 4    | 4      | 4           | 4          | 50%      |
| 12     | 8    | 4      | 9           | 8          | 67%      |
| 16     | 12   | 4      | 13          | 12         | 75%      |

逻辑在 `Erasure::with_default_parity()` 中：
- `<=5 磁盘` → parity=2
- `6-7 磁盘` → parity=3
- `>=8 磁盘` → parity=4

### 6.3 设计决策

- **单 Set**：Phase 1 所有磁盘属于同一个 `ErasureSet`。多 Set 路由（SipHash 分片）在 Phase 2 实现。
- **不支持混合磁盘数**：所有磁盘路径必须构成一个同构 EC 组。不同类型的盘（如不同容量）仅以最小容量为准。
- **`ErasureObjects::new()` vs `with_params()`**：Phase 1 默认使用 `new()` 自动选择 parity。仅在需要测试特定 EC 配置时使用 `with_params()`。

---

## 7. AppState 定义

```rust
/// Server 运行时共享状态
pub struct AppState {
    /// 对象存储实例（线程安全、Arc 共享）
    pub objects: Arc<ErasureObjects>,
}
```

### 7.1 为什么只放 `Arc<ErasureObjects>`？

- `ErasureObjects` 实现了 `ObjectAPI` trait，是 S3 路由层的唯一依赖。
- 后续扩展（IAM、配置管理）通过 `AppState` 加字段即可，不需要改路由签名。
- `Arc<dyn ObjectAPI>` 的 trait object 也可以，但 Phase 1 使用具体类型减少动态分派开销。

### 7.2 生命周期

`AppState` 在 `main()` 中创建，通过 `axum::Router::with_state()` 注入到所有 S3 handler 中。程序退出时自然释放。

---

## 8. HTTP Server 启动

### 8.1 代码骨架

```rust
pub async fn run(config: ServerConfig) -> MinioResult<()> {
    // 1. 磁盘检测
    let checked_disks = check_disks(&config.disks).await?;
    let disks: Vec<Arc<dyn StorageAPI>> = checked_disks
        .into_iter()
        .map(|d| d.xl_storage as Arc<dyn StorageAPI>)
        .collect();

    // 2. EC 池初始化
    let objects = init_erasure_objects(disks)?;

    // 3. 构建 AppState
    let state = Arc::new(AppState { objects });

    // 4. 构建 axum Router
    let app = s3::router(state.clone());

    // 5. 创建 TCP listener
    let listener = TcpListener::bind(&config.address).await?;

    // 6. 打印 banner
    print_banner(&config, state.clone());

    // 7. 启动 HTTP server + 信号等待
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}
```

### 8.2 优雅关闭

```rust
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("shutdown signal received, initiating graceful shutdown...");
}
```

### 8.3 设计决策

- **使用 `axum::serve`** 而非手动 `hyper::server`：axum 0.7 集成了 `serve()` 函数，直接接受 `TcpListener` + `Router`，与 graceful shutdown 配合良好。
- **`TcpListener` 由 `tokio::net` 创建** 而非 `axum::serve` 隐式绑定：Phase 2 可以在 bind 后、serve 前执行额外初始化（如 unix socket、reuse port）。
- **SIGTERM + SIGINT 双信号**：确保在容器环境（SIGTERM）和终端环境（Ctrl+C / SIGINT）下都能正常关闭。

---

## 9. 启动 Banner

### 9.1 输出示例

```
┌────────────────────────────────────────────────────────────┐
│                    minio-rs  v0.1.0                        │
│                                                            │
│  Endpoint:  http://192.168.1.100:9000                      │
│  Console:   (disabled in Phase 1)                          │
│                                                            │
│  Disks:                                                     │
│    /data/disk1    up  500 GB   formatted: yes               │
│    /data/disk2    up  500 GB   formatted: yes               │
│    /data/disk3    up  500 GB   formatted: yes               │
│    /data/disk4    up  500 GB   formatted: yes               │
│                                                            │
│  Erasure Configuration:                                    │
│    Drives:  4                                              │
│    Data:    2                                              │
│    Parity:  2                                              │
│    EC Block Size:  4 MiB                                   │
│    Write Quorum:   3 / 4                                   │
│    Read Quorum:    2 / 4                                   │
│    Storage Usage:  50.0%  (usable / total)                 │
│                                                            │
│  Status:  http://192.168.1.100:9000/minio/health/live      │
│                                                            │
└────────────────────────────────────────────────────────────┘
```

### 9.2 设计决策

- 使用纯文本图形框，不依赖第三方库（如 `termion` / `crossterm`）。
- 自动检测本机 IP：通过 `local-ip-address` crate 获取第一个非回环 IPv4 地址。如果获取失败则 fallback 到 `0.0.0.0`。
- Banner 使用 `tracing::info!` 输出而非 `println!`，保证日志格式统一。
- 每个磁盘一行，显示 `up/down` 状态、总容量、格式化状态。

---

## 10. 主函数 (main.rs) 代码骨架

```rust
use clap::Parser;
use tracing_subscriber::EnvFilter;

mod cmd;

#[tokio::main]
async fn main() {
    // Init logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(true)
        .with_file(true)
        .with_line_number(true)
        .init();

    // Parse CLI
    let cli = cmd::Cli::parse();
    let cmd::Commands::Server { address, console_address, disks } = cli.command;

    // Build config
    let config = ServerConfig {
        address,
        console_address,
        disks,
    };

    // Run server
    if let Err(e) = server::run(config).await {
        tracing::error!("server exited with error: {e}");
        std::process::exit(1);
    }
}
```

---

## 11. 依赖追加

需要在 `crates/server/Cargo.toml` 的 `[dependencies]` 段新增：

```toml
clap = { version = "4", features = ["derive"] }
local-ip-address = "0.6"           # 用于 banner 中自动检测本机 IP
```

---

## 12. Phase 2 扩展点

| 模块 | 预留接口 | 未来演进 |
|------|----------|----------|
| `cmd.rs` | `Commands` enum 预留其他子命令 | `gateway`、`admin`、`update` |
| `disk.rs` | `CheckDiskResult` 预留 `healing` 字段 | `HealFormat` 格式化流程、磁盘替换 |
| `server.rs` | `ErasureObjects` 单 set → 多 set | 多池路由、Set 拓扑发现 |
| `banner.rs` | Console 地址字段预留 | 输出 Console URL 和 admin 凭据 |
| `lib.rs` | `endpoint`/`layout`/`net` 模块骨架 | 分布式启动的 endpoint 解析 |

---

## 13. 实现顺序（推荐）

| 步骤 | 文件 | 内容 | 可独立测试？ |
|------|------|------|-------------|
| 1 | `cmd.rs` | clap CLI 定义 | 是（单元测试） |
| 2 | `disk.rs` | `check_disks()` 函数 | 是（临时目录集成测试） |
| 3 | `server.rs` | 组装流程：检测 + EC 池 + AppState | 部分（需要真实磁盘） |
| 4 | `banner.rs` | 启动 Banner 输出 | 是（纯格式化逻辑） |
| 5 | `main.rs` | `#[tokio::main]` 入口 | 否（集成测试） |
| 6 | `lib.rs` | 声明 `cmd`/`server`/`disk`/`banner` 模块 | - |

每个步骤可以单独提交，步骤 5 是最后粘合。

---

## 14. 完整文件清单（新增）

```
crates/server/src/
├── main.rs       ← 重写：入口 + tracing + CLI + 调用 server::run()
├── lib.rs        ← 追加模块声明 + AppState + ServerConfig + run() 签名
├── cmd.rs        ← 新建：clap CLI derive struct
├── server.rs     ← 新建：ServerConfig + run() + shutdown_signal()
├── disk.rs       ← 新建：check_disks() + CheckedDisk
└── banner.rs     ← 新建：print_banner()
```
