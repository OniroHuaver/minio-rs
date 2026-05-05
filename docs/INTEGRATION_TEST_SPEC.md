# 集成测试方案设计文档

> minio-rs Phase 1 集成测试方案
> 目标：通过端到端测试验证单机模式下 PUT/GET/DELETE/LIST 对象操作的正确性

---

## 1. 测试环境搭建

### 1.1 启动 minio-rs server

**方案：子进程管理（推荐）**

测试进程通过 `std::process::Command` 启动 `cargo build` 产出的 `minio` 二进制，将临时目录作为数据盘传入。

```
临时目录布局:
/tmp/minio-test-XXXXXX/
  └── data/          ← 传给 minio server 作为数据盘
```

启动流程：
1. 使用 `tempfile::TempDir` 创建临时目录
2. 构建 `Command: ./target/debug/minio server <temp_dir>/data --address 127.0.0.1:0`
3. 等待 server 端口就绪（TcpStream connect 轮询, 30s 超时）
4. 测试全部结束后，Drop `TempDir` 触发自动清理，同时 Kill 子进程

### 1.2 等待服务就绪

| 方式 | 实现 | 推荐度 |
|------|------|--------|
| 端口监听检测 | `TcpStream::connect(addr)` 轮询直到成功 | 首选 |
| stdout 日志匹配 | 匹配 `"listening on"` 或 `"ready"` | 兜底 |

超时：30 秒，每 200ms 轮询一次。

### 1.3 测试结束清理

| 资源 | 清理方式 |
|------|----------|
| server 进程 | Kill 子进程（Drop Child） |
| 临时数据目录 | Drop TempDir 自动删除 |

---

## 2. 测试客户端选择

### 推荐方案：三阶段分层测试

```
              ┌──────────────────┐
              │   端到端测试 (mc)  │  ← 验证完整业务流程
              ├──────────────────┤
              │   API 级测试      │  ← 验证 HTTP 路由 + S3 协议
              │  (reqwest)       │
              ├──────────────────┤
              │   单元级测试       │  ← 验证核心逻辑
              │  (crate test)    │
              └──────────────────┘
```

**Phase 1 以 API 级测试（reqwest）为主，mc 端到端验证为辅。**

理由：无需安装外部依赖、可精确构造边界条件、与 CI 天然集成。

---

## 3. 测试用例清单

### 3.1 Bucket 操作

| ID | 名称 | 步骤 | 预期 |
|----|------|------|------|
| B-01 | CreateBucket | `PUT /{bucket}` | 返回 200 |
| B-02 | CreateBucket 重复 | 两次 `PUT /{bucket}` | 第二次 409 |
| B-03 | DeleteBucket | PUT → DELETE → HEAD | DELETE 204, HEAD 404 |
| B-04 | ListBuckets | 创建 3 个 bucket → ListBuckets | 列表包含全部 |
| B-05 | Bucket 不存在 | HEAD 不存在桶 | 404 NoSuchBucket |

### 3.2 Object PUT

| ID | 名称 | 步骤 | 预期 |
|----|------|------|------|
| P-01 | PutObject 小文件 | PUT <128KiB 数据 | 200, ETag 正确 |
| P-02 | PutObject 大文件 | PUT >128KiB 数据 | 200, 分片写入 |
| P-03 | PutObject 空对象 | 0 字节 | 200 |
| P-04 | PutObject 覆盖 | PUT 同 key 两次 | 第二次成功，GET 新数据 |
| P-05 | PutObject 含元数据 | `x-amz-meta-color: red` | GET 返回元数据 |

### 3.3 Object GET

| ID | 名称 | 步骤 | 预期 |
|----|------|------|------|
| G-01 | GetObject 全量 | PUT → GET | 数据一致 |
| G-02 | GetObject Range | `Range: bytes=0-99` | 返回指定字节 |
| G-03 | GetObject 不存在 | GET 不存在的 key | 404 NoSuchKey |

### 3.4 HeadObject / DeleteObject

| ID | 名称 | 步骤 | 预期 |
|----|------|------|------|
| H-01 | HeadObject 存在 | PUT → HEAD | 200 + 元数据 headers |
| H-02 | HeadObject 不存在 | HEAD 不存在 key | 404 |
| D-01 | DeleteObject | PUT → DELETE → GET | DELETE 204, GET 404 |
| D-02 | DeleteObject 不存在 | DELETE 不存在 key | 204 (幂等) |

### 3.5 ListObjectsV2

| ID | 名称 | 步骤 | 预期 |
|----|------|------|------|
| L-01 | 空桶 List | 空桶 ListObjectsV2 | 空列表 |
| L-02 | 多对象 | 写 5 个 → List | 返回 5 个 |
| L-03 | prefix 过滤 | `prefix=a/` | 只返回匹配项 |
| L-04 | max-keys | max-keys=3 | 返回 3 个 + IsTruncated |

### 3.6 错误场景

| ID | 名称 | 预期 |
|----|------|------|
| E-01 | 对象不存在 | 404 + NoSuchKey XML |
| E-02 | Bucket 不存在 | 404 + NoSuchBucket XML |

---

## 4. 环境变量

| 环境变量 | 值 | 说明 |
|----------|-----|------|
| `MINIO_ROOT_USER` | `minioadmin` | 管理员 Access Key |
| `MINIO_ROOT_PASSWORD` | `minioadmin` | 管理员 Secret Key |

---

## 5. Rust 集成测试

### 5.1 文件位置

```
minio-rs/
  tests/
    integration.rs          ← 主测试文件
    common/
      mod.rs                ← 公共 helper
      server_process.rs     ← server 进程管理
      s3_client.rs          ← S3 HTTP 请求构造
```

### 5.2 核心设计

**TestServer**: 封装子进程生命周期
```rust
pub struct TestServer {
    child: Option<Child>,
    pub addr: String,
    _data_dir: TempDir,
}
// start(): 启动子进程 + 等待端口就绪
// Drop: kill 进程 + 清理临时目录
```

**S3Client**: 封装 HTTP 请求构造
```rust
pub struct S3Client {
    client: Client,       // reqwest::Client
    endpoint: String,
}
// put_object(), get_object(), delete_object(), head_object(), list_objects_v2()
```

### 5.3 依赖

```toml
[dev-dependencies]
reqwest = { version = "0.12", features = ["json"] }
tempfile = "3"
```

### 5.4 运行方式

```bash
cargo build --bin minio
cargo test --test integration -- --nocapture
cargo test --test integration test_small_file_put_get -- --nocapture
```

---

## 6. 验证标准

```bash
# 1. 启动
./target/debug/minio server /tmp/data --address 127.0.0.1:9000 &

# 2. mc 验证
mc alias set local http://localhost:9000 minioadmin minioadmin
mc mb local/testbucket
echo "Hello, World!" > hello.txt
mc cp hello.txt local/testbucket/
mc cat local/testbucket/hello.txt
mc rm local/testbucket/hello.txt
```

### 测试通过标准

| 条件 | 要求 |
|------|------|
| Rust 集成测试 | 所有 `tests/integration.rs` 测试通过 |
| mc 端到端 | shell 命令序列全部成功 |
| 错误场景 | 返回预期 HTTP 状态码和 S3 错误码 |
| 清理 | 测试结束后无残留进程/文件 |

---

*文档版本：v1.0*
*关联任务：Phase 1.7 集成测试*
