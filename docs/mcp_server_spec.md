# MCP Server 需求与设计文档

> minio-rs 的 MCP (Model Context Protocol) Server，提供 S3 服务生命周期管理与性能压测能力

---

## 1. 需求背景

### 1.1 动机

开发与测试过程中需要频繁执行以下操作：

1. 启动 minio-rs S3 服务（指定地址、磁盘路径）
2. 对服务运行 s3perf 性能压测
3. 分析压测结果
4. 停止服务

当前这些操作需要手动执行多条 shell 命令，效率低且难以复用。通过 MCP Server 将这些能力暴露为结构化工具，Claude Code 等 AI 编码助手可直接编排完整的"启动→压测→分析→停止"工作流。

### 1.2 用户故事

- **开发者**：在 Claude Code 中描述压测需求（如"用 4 线程对 mixed 工作负载压测 30 秒"），AI 自动完成全流程
- **CI 集成**：MCP tools 可作为自动化压测流水线的基础组件
- **调试诊断**：快速启动一个临时 minio-rs 实例进行 S3 API 调试

---

## 2. 架构设计

### 2.1 整体架构

```
┌─────────────────┐     stdio (JSON-RPC)     ┌──────────────────┐
│  Claude Code /  │ ◄──────────────────────► │   mcp-server     │
│  MCP Client     │                          │   (本 crate)      │
└─────────────────┘                          ├──────────────────┤
                                             │ ┌─start_server──┐│
                                             │ │ minio-rs       ││
                                             │ │ server::run()  ││
                                             │ └────────────────┘│
                                             │ ┌─run_benchmark─┐│
                                             │ │ s3perf         ││
                                             │ │ run_mixed/get/ ││
                                             │ │ put/delete...  ││
                                             │ └────────────────┘│
                                             └──────────────────┘
```

### 2.2 Crate 位置

```
minio-rs/
├── crates/
│   └── mcp-server/          ← 新增 MCP Server Crate
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs       # 二进制入口（stdio transport）
│           └── lib.rs        # McpServer handler + 4 个 tool 定义
├── s3perf/                   # 已有（库化后可供 mcp-server 引用）
└── src/                      # minio-rs 主 crate
```

### 2.3 依赖关系

```
mcp-server ──→ minio-rs (server lifecycle)
            ──→ s3perf   (benchmark runner)
            ──→ rmcp 1.6 (MCP Rust SDK)
```

### 2.4 MCP 框架选型

选用 **rmcp 1.6.0**（[rust-sdk](https://github.com/modelcontextprotocol/rust-sdk)），原因：

- MCP 官方 Rust SDK，社区最活跃
- `#[tool]` + `#[tool_router]` 属性宏自动生成 ServerHandler trait
- schemars 集成自动生成 JSON Schema
- stdio transport 原生支持

---

## 3. 前置依赖变更

### 3.1 minio-rs server 支持程序化关闭

**文件**：`src/server/run.rs`

`run()` 函数签名从：

```rust
pub async fn run(config: ServerConfig) -> MinioResult<()>
```

改为：

```rust
pub async fn run(
    config: ServerConfig,
    shutdown: Option<CancellationToken>,
) -> MinioResult<()>
```

当 `shutdown` 为 `Some(token)` 时，`with_graceful_shutdown` 同时监听 OS signal 和 token.cancelled()。

### 3.2 s3perf 库化

**新增** `s3perf/src/lib.rs`，声明所有模块并 re-export 公共 API。

`run_benchmark` 及所有 wrapper 函数（`run_mixed`、`run_get` 等）的返回值从 `anyhow::Result<()>` 改为 `anyhow::Result<Aggregated>`。

所有 `println!` 输出改为 `eprintln!`（避免污染 MCP stdio 传输通道）。

---

## 4. MCP Tools 定义

### 4.1 start_server

启动 minio-rs S3 服务。

| 属性 | 值 |
|------|-----|
| 参数 | `disks: Vec<String>`（必填，至少 1 个磁盘路径） |
| | `address?: String`（默认 `0.0.0.0:9000`） |
| | `console_address?: String` |
| 返回 | `{"status": "started", "address": "0.0.0.0:9000"}` |
| 错误 | 服务已运行 / 磁盘路径为空 |

实现逻辑：

1. 检查是否已有运行中的服务（通过内部状态 `server_task` 判断）
2. 构建 `ServerConfig` → spawn tokio task 调用 `minio_rs::server::run::run(config, Some(token))`
3. 存储 `JoinHandle` + `CancellationToken` + `address` 到 `McpServerInner`
4. 等待 500ms 让服务完成 bind

### 4.2 stop_server

停止运行中的 minio-rs 服务。

| 属性 | 值 |
|------|-----|
| 参数 | 无 |
| 返回 | `{"status": "stopped"}` 或 `{"status": "not_running"}` |

实现逻辑：

1. 取出 `CancellationToken` 和 `JoinHandle`
2. Cancel token → await handle（10 秒超时）
3. 清理内部状态

### 4.3 server_status

查询服务运行状态。

| 属性 | 值 |
|------|-----|
| 参数 | 无 |
| 返回 | `{"running": bool, "address": "0.0.0.0:9000" | null}` |

### 4.4 run_benchmark

对任意 S3 endpoint 运行性能压测。

| 属性 | 值 |
|------|-----|
| 参数 | `endpoint: String`（必填，S3 地址） |
| | `benchmark: String`（必填，类型：mixed/get/put/delete/list/stat） |
| | `access_key?: String`（默认 `minioadmin`） |
| | `secret_key?: String`（默认 `minioadmin`） |
| | `region?: String`（默认 `us-east-1`） |
| | `tls?: bool`（默认 false） |
| | `insecure?: bool`（默认 false） |
| | `bucket?: String`（默认 `s3perf-bench`） |
| | `concurrency?: usize`（默认 4） |
| | `duration?: String`（默认 `30s`） |
| | `obj_size?: String`（默认 `1MiB`） |
| | `objects?: usize`（默认 100） |
| | `get_distrib?: f64`（mixed 用，默认 0.45） |
| | `stat_distrib?: f64`（mixed 用，默认 0.05） |
| | `put_distrib?: f64`（mixed 用，默认 0.25） |
| | `delete_distrib?: f64`（mixed 用，默认 0.25） |
| 返回 | 完整 `Aggregated` JSON（含吞吐量、延迟百分位、分段统计） |

实现逻辑：

1. 从参数构建 `s3perf::S3Config` + `s3perf::BenchConfig`
2. 根据 `benchmark` 类型分发到对应 runner：
   - `mixed` → `run_mixed(&bc, g, s, p, d)`
   - `get` → `run_get(&bc, 1, None, false)`
   - `put` → `run_put(&bc, false, None, false)`
   - `delete` → `run_delete(&bc, 100)`
   - `list` → `run_list(&bc, false)`
   - `stat` → `run_stat(&bc)`
3. 将 `Aggregated` 序列化为 `serde_json::Value` 返回

---

## 5. 内部状态管理

```rust
struct McpServerInner {
    server_task: Mutex<Option<JoinHandle<()>>>,
    server_token: Mutex<Option<CancellationToken>>,
    server_address: Mutex<Option<String>>,
}
```

- `server_task`: 后台 server 的 task handle，用于判断运行状态和等待退出
- `server_token`: 用于触发优雅关闭
- `server_address`: 记录 server 的监听地址，供 `server_status` 返回

所有字段由 `tokio::sync::Mutex` 保护。`McpServer` 通过 `Arc<McpServerInner>` 实现 Clone（rmcp 要求 handler 可 Clone）。

---

## 6. 错误处理

使用 `rmcp::ErrorData` 作为 tool 返回值中的错误类型：

- `ErrorData::invalid_params(msg, None)` — 参数校验失败
- `ErrorData::internal_error(msg, None)` — 运行时错误

rmcp 框架自动将 `ErrorData` 映射为 MCP 协议的标准错误响应。

---

## 7. MCP 注册配置

在 `~/.claude/mcp.json` 中添加：

```json
{
  "mcpServers": {
    "minio-bench": {
      "type": "stdio",
      "command": "cargo",
      "args": ["run", "-p", "mcp-server"],
      "cwd": "/path/to/minio-rs"
    }
  }
}
```

或使用 release build：

```json
{
  "mcpServers": {
    "minio-bench": {
      "type": "stdio",
      "command": "/path/to/minio-rs/target/release/mcp-server"
    }
  }
}
```

---

## 8. 使用示例

### 8.1 启动服务 + 运行压测

```
用户: 启动一个单盘 minio 服务，然后跑 30 秒 mixed 压测

Claude Code (通过 MCP):
  1. start_server { disks: ["/tmp/data"] }
     → {"status": "started", "address": "0.0.0.0:9000"}
  2. run_benchmark { endpoint: "localhost:9000", benchmark: "mixed", duration: "30s" }
     → { mixed_server_stats: { avg_mbps: 123.4, avg_ops: 567.8 }, ... }
  3. [可选] stop_server
```

### 8.2 对外部 S3 服务压测

```
用户: 对 play.min.io 跑一个 GET 压测

Claude Code:
  run_benchmark {
    endpoint: "play.min.io",
    benchmark: "get",
    access_key: "Q3AM3UQ867SPQQA43P2F",
    secret_key: "zuf+tfteSlswRu7BJ86wekitnifILbZam1KYY3TG",
    tls: true,
    duration: "1m",
    concurrency: 16
  }
```

---

## 9. 后续迭代

- [ ] **分布式压测**：通过 s3perf 的 `--remote-hosts` 支持多 agent 协同压测
- [ ] **连续压测**：支持多次运行并自动对比结果（regression detection）
- [ ] **结果持久化**：压测结果写入 InfluxDB + 历史趋势查询
- [ ] **TUI 模式**：通过 MCP resource 暴露实时压测面板
- [ ] **HTTP transport**：支持 HTTP/SSE transport（用于远程 MCP client）
