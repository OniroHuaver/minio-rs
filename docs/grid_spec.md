# Grid 分布式 RPC 框架

> Grid 是内部服务器间高性能 RPC 通信框架，替代旧的 REST 锁调用，基于 WebSocket 长连接 + 消息复用。

## 核心定位

Grid → WebSocket 单连接双向复用，承载锁、存储、集群管理等所有内部 RPC。

## 消息协议

### 消息结构

```
Message {
    mux_id: u64,        // 多路复用 ID，路由请求/响应
    seq: u32,           // 序列号
    deadline_ms: u32,   // 超时（毫秒）
    handler: u8,        // Handler ID
    op: Op,             // 操作码
    flags: Flags,       // 标志位
    payload: Option<Vec<u8>>,  // MessagePack 序列化荷载
}
```

序列化格式：MessagePack（`rmp-serde`），对齐 Go msgp。

### Op 操作码

| Op | 值 | 说明 |
|----|-----|------|
| Connect | 1 | 发起连接 |
| ConnectResponse | 2 | 连接响应 |
| Ping | 3 | 心跳请求 |
| Pong | 4 | 心跳响应 |
| ConnectMux | 5 | 建立多路复用流 |
| MuxConnectError | 6 | 多路复用连接错误 |
| DisconnectClientMux | 7 | 客户端断开流 |
| DisconnectServerMux | 8 | 服务端断开流 |
| MuxClientMsg | 9 | 客户端流消息 |
| MuxServerMsg | 10 | 服务端流消息 |
| UnblockSrvMux | 11 | 解除服务端流阻塞 |
| UnblockClMux | 12 | 解除客户端流阻塞 |
| AckMux | 13 | 流确认 |
| Request | 14 | 单次请求 |
| Response | 15 | 单次响应 |
| Disconnect | 16 | 断开连接 |
| Merged | 17 | 合并消息 |

### Flags 标志位

| Flag | 位 | 说明 |
|------|-----|------|
| CRCXXH3 | 0 | 携带 xxh3 校验和 |
| EOF | 1 | 流 EOF |
| STATELESS | 2 | 无状态消息 |
| PAYLOAD_IS_ERR | 3 | 荷载为错误字符串 |
| PAYLOAD_IS_ZERO | 4 | 荷载为 0 长度切片 |
| SUBROUTE | 5 | 携带子路由 |

## 架构分层

```
┌─────────────────────────────────────────────┐
│  Manager                                    │
│  管理所有目标服务器的 Connection             │
│  注册 Handler → handlers{ single, streams } │
└──────────────┬──────────────────────────────┘
               │
┌──────────────┴──────────────────────────────┐
│  Connection                                 │
│  单条 WebSocket 长连接，双向消息流          │
│  ├─ mux_client (outgoing) → 客户端多路复用  │
│  ├─ mux_server (inStream) → 服务端流处理    │
│  ├─ read_task / write_task 异步任务         │
│  └─ 自动重连、Ping/Pong 心跳               │
└──────────────┬──────────────────────────────┘
               │
┌──────────────┴──────────────────────────────┐
│  消息协议 (message.rs)                      │
│  MessagePack 二进制序列化                    │
│  OpCode: Request/Response/MuxClientMsg/...  │
└─────────────────────────────────────────────┘
```

## Handler 类别

Grid 承载 120+ handler，分四大类：

| 类别 | 示例 Handler | 前缀 |
|------|-------------|------|
| 分布式锁 | HandlerLockLock, HandlerLockRLock, HandlerLockRefresh... | lockR |
| 存储操作 | HandlerReadXL, HandlerDeleteFile, HandlerRenameData... | storageR |
| 集群 Peer | HandlerLoadUser, HandlerServerInfo, HandlerTrace... | peer |
| S3 操作 | HandlerMakeBucket, HandlerHeadBucket... | peerS3 |

## 关键实现细节

### 1. 连接方向决策（shouldConnect）

对每对 host 做哈希比较，确定性地让一方做 client 发起连接：

```rust
fn should_connect(local: &str, remote: &str) -> bool {
    let h0 = xxh3_64((local.to_string() + remote).as_bytes());
    let h1 = xxh3_64((remote.to_string() + local).as_bytes());
    h0 < h1  // 双方得出相反结果，避免双向重复连接
}
```

### 2. 消息多路复用 — MuxID

- 每个请求分配唯一 MuxID（AtomicU64 原子递增）
- `Connection.outgoing: RwLock<HashMap<u64, oneshot::Sender>>` — 等待响应的请求
- `Connection.incoming: RwLock<HashMap<u64, mpsc::Sender>>` — 活跃的流
- 所有请求/响应/流消息通过 MuxID 路由

### 3. 单请求模式（Single Payload）

```
Client: conn.request(ctx, handler_id, payload)
  → 分配 mux_id
  → 创建 oneshot channel
  → 存入 outgoing[mux_id]
  → 发送 Op::Request
  → tokio::time::timeout(ctx, oneshot) → 返回 payload

Server: read_task 收到 Op::Request
  → 查找 handler
  → tokio::spawn(handler(payload))
  → 发送 Op::Response(response_payload)
```

默认 1 分钟超时。单请求不支持取消传播。

### 4. 流模式（Streaming）

```
Client: conn.new_stream(ctx, handler_id, payload) → Stream
  → 分配 mux_id
  → 创建 mpsc channel (InCapacity/OutCapacity 限制)
  → 发送 Op::ConnectMux

Server: 收到 Op::ConnectMux
  → 创建 mux_server
  → tokio::spawn(handler(ctx, payload, request_rx, response_tx))
```

支持完全双向通信，带流控（InCapacity/OutCapacity 限制 channel 大小），context 取消传播到服务端。

### 5. 消息合并（write_task）

write_task 从 out_queue 取消息时：
- 如果队列中还有更多消息，yield 后继续收集（最多 50 条）
- 合并为一个 Op::Merged 消息发送，减少 WebSocket 帧开销
- 仅在连接状态为 StateConnected 时才实际写入

### 6. 连接生命周期

```
StateUnconnected → StateConnecting → StateConnected
                    ↑                    ↓ (断连)
                    └── StateConnectionError ←┘
                         ↓ (重连成功)
                         StateConnected
```

- 每 15s 客户端 Ping，每 10s 连接级 Ping
- 超 30s 没收到 Pong → 断开重连
- 重连时清空所有进行中的 muxClient/muxServer，通知调用方 ErrDisconnected
- 2s 拨号超时 + 随机退避重试

### 7. 类型安全的 Handler

使用 trait object 注册 handler。后续可扩展为泛型封装 `SingleHandler<Req, Resp>` 和 `StreamHandler<Payload, Req, Resp>`，自动处理 MessagePack 序列化/反序列化。

```rust
// 注册
manager.register_single_handler(HANDLER_ECHO, |payload| {
    Box::pin(async move {
        Ok(payload) // echo
    })
});

// 调用
let response = conn.request(ctx, HANDLER_ECHO, &payload).await?;
```

## Rust 实现模块

```
src/grid/
├── mod.rs              # pub re-exports, module docs
├── connection.rs       # Connection: WS lifecycle, read/write/ping tasks
├── connection_state.rs # ConnectionState enum
├── error.rs            # GridError, GridResult, RemoteErr
├── handler.rs          # SingleHandler trait + HandlerRegistry
├── manager.rs          # Manager: Connection pool, handler registration
├── message.rs          # Message, Op, Flags, HandlerId
├── msg_types.rs        # ConnectReq, ConnectResp, MSS, Bytes, test types
├── tests/
│   ├── mod.rs          # Unit/integration tests (simulated pairs, serialization)
│   ├── harness.rs      # [Phase 2] TestGrid multi-node sandbox
│   ├── fault.rs        # [Phase 2] Fault injection (DebugMsg)
│   └── benchmark.rs    # [Phase 2] Performance benchmarks
└── debug.rs            # [Phase 2] DebugMsg enum + Connection::debug_msg
```

## 测试架构

Grid 测试分三层：

### Layer 1: 消息与连接单元测试（已有）

基于 `simulated_pair()` 的内存管道测试，验证：
- 消息 MessagePack 序列化/反序列化
- 单请求 echo / 错误传播 / handler not found / 超时
- should_connect 拓扑对称性/幂等性
- 边界情况（孤儿 response、未知 op）

不涉及网络。通过 `conn_a.out_queue` → `conn_b.dispatch()` 管道模拟双向通信。

### Layer 2: DebugMsg 故障注入层（待实现）

为 Connection 添加 `debug_msg(&self, msg: DebugMsg)` 测试后门（仅在 `#[cfg(test)]` 下编译）：

```rust
// src/grid/debug.rs
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugMsg {
    Shutdown,
    KillInbound,              // 关闭读取方向
    KillOutbound,             // 关闭写入方向
    WaitForExit,              // 等待后台任务退出
    SetConnPingDuration(u64), // 修改连接级 ping 间隔
    SetClientPingDuration(u64), // 修改客户端 ping 间隔
    AddToDeadline(u64),       // 模拟调度延迟
    IsOutgoingClosed,         // 查询出站通道状态
    BlockInboundMessages(bool), // 阻塞/放行入站消息
}
```

影响范围：
- `Connection` 新增 `pub(crate) fn debug_msg(&self, msg: DebugMsg)` 方法
- `ConnectionInner` 新增 debug 控制字段（如 `block_inbound: AtomicBool`）
- 可直接在现有 simulated_pair 上验证故障注入行为

### Layer 3: TestGrid 多节点 WebSocket 沙盒（待实现）

启动真实 HTTP/WS 服务进程组，验证完整建连流程：

```
Manager ──WebSocket──→ TcpListener ──WebSocket──→ Manager
```

关键要素：
- `ManagerOptions` 扩展：支持 `hosts` 列表、`auth_fn` 回调（mock auth / debug token）
- 动态端口分配：`TcpListener::bind("127.0.0.1:0")`
- 连接屏障（barrier）：`tokio::sync::Notify` 等待所有 listener 就绪后统一建联
- `wait_all_connect()` 等待全互联完成
- `Manager::serve(listener)` 启动 HTTP 服务器（axum + tungstenite upgrade）

Barrier 时序：
1. 所有 `TcpListener::bind` 完成
2. 所有 HTTP server `tokio::spawn` 启动
3. `notify_waiters()` 释放 → 所有 Manager 开始互连

### 实现优先级

| 优先级 | 任务 | 新增文件 | 改动文件 |
|--------|------|---------|---------|
| P0 | DebugMsg 枚举 + Connection::debug_msg | `src/grid/debug.rs` | `connection.rs`, `mod.rs` |
| P1 | ManagerOptions 扩展（hosts 列表、auth 回调） | - | `manager.rs` |
| P2 | TestGrid 沙盒（端口分配、WS 服务器、barrier） | `src/grid/tests/harness.rs` | - |
| P3 | 故障注入测试用例 | `src/grid/tests/fault.rs` | - |
| P4 | 填充 benchmarks（bench_grid_requests 等） | - | `tests/benchmark.rs` |

## 设计决策

| 决策 | 选择 | 理由 |
|------|------|------|
| Message payload | `Option<Vec<u8>>`（无 lifetime） | rmp-serde 反序列化总是 owned，Cow 无实际收益 |
| Handler 注册 | Trait object (`Arc<dyn HandlerTrait>`) | 灵活、对齐代码库风格、支持动态注册 |
| MuxID 路由 | `oneshot` per request | 零锁竞争、自然超时、锁粒度在单连接 |
| 传输 | `tokio-tungstenite 0.24` | 已在 s3perf 验证可用 |
| 序列化 | `rmp-serde` (MessagePack) | 对齐 Go msgp，已有依赖 |
| xxh3 哈希 | `xxhash-rust` | `shouldConnect()` 需要 |
| Buffer 池 | 暂不做 | 网络 I/O 为瓶颈，先 Benchmark 再优化 |

## 依赖

```toml
tokio-tungstenite = "0.24"
futures-util = "0.3"
xxhash-rust = { version = "0.8", features = ["xxh3"] }
axum = { version = "0.7", features = ["ws"] }
```
