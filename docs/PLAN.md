# minio-rs 分阶段实施计划

> 对应原 MinIO Go 版本理解 → Rust 逐步重写

---

## 总体策略

沿数据"热路径"逐步推进 — 先让一个对象能写下去、读出来，再逐步加上分布式、认证、高级特性。

每阶段产出**可运行二进制 + 集成测试**。

---

## Phase 1: 单机核心存储引擎

**目标**：单机模式下 PUT/GET/DELETE 对象，理解磁盘格式和 EC 编码。

### 子任务

- [ ] **1.1** `base`: xl.meta 格式读/写工具 — 能解析磁盘上的 `xl.meta` 文件
- [ ] **1.2** `storage`: xlStorage 本地磁盘驱动实现 — 文件 IO、原子 rename、目录管理
- [ ] **1.3** `erasure`: Reed-Solomon 编解码 + Quorum 判定 + 分片并行读写
- [ ] **1.4** `object`: erasureObjects — PUT/GET/DELETE 完整调用链
- [ ] **1.5** `s3`: axum HTTP 路由 — PutObject / GetObject / DeleteObject / ListObjectsV2
- [ ] **1.6** `server`: 启动流程 — 磁盘检测、EC 池初始化、HTTP Server 监听
- [ ] **1.7** 集成测试 — 用 `awscli` 或 `mc` 客户端做端到端验证

### 验证标准

```bash
# 启动 minio-rs server
./target/debug/minio server /tmp/data

# 用 mc 客户端操作
mc alias set local http://localhost:9000 minioadmin minioadmin
mc mb local/testbucket
mc cp hello.txt local/testbucket/
mc cat local/testbucket/hello.txt
mc rm local/testbucket/hello.txt
```

---

## Phase 2: 分布式模式

**目标**：多节点通信、分布式锁、多池路由。

### 子任务

- [ ] **2.1** `grid`: gRPC 节点间 RPC (远程磁盘读/写)
- [ ] **2.2** `storage`: storageRESTClient — 通过 RPC 访问远程磁盘
- [ ] **2.3** 分布式锁 (dsync) — 写操作互斥
- [ ] **2.4** `object`: ServerPool 路由 — SipHash 选 Set + 多池选择
- [ ] **2.5** 集群自举 — 节点发现、盘符协商、格式化
- [ ] **2.6** 集成测试 — 多节点部署验证

### 验证标准

```bash
# 4节点 + 每节点4盘 = 16盘集群
minio server \
  http://node{1..4}/data{1..4}
```

---

## Phase 3: IAM + STS + Bucket 元数据

**目标**：多租户认证授权、S3 签名验证。

### 子任务

- [ ] **3.1** `iam`: IAM Store — 内部用户/策略/组的 CRUD 持久化
- [ ] **3.2** `iam`: STS — AssumeRole 系列 (WebIdentity / LDAP / Certificate)
- [ ] **3.3** `iam`: 策略评估引擎 (类 AWS IAM Policy)
- [ ] **3.4** `s3`: 认证中间件 — AWS SigV4 签名验证
- [ ] **3.5** Bucket 元数据 — lifecycle、encryption、versioning 配置
- [ ] **3.6** 集成测试 — 多用户 / 策略边界 / STS 凭证

### 验证标准

```bash
# 创建用户并测试策略隔离
mc admin user add local user1 password1
mc admin policy attach local readwrite --user user1
mc cp secret.txt local/bucket/ --access-key user1 --secret-key password1
```

---

## Phase 4: 高级特性（按兴趣选做）

- [ ] **4.1** 事件通知 — Webhook / Kafka / AMQP
- [ ] **4.2** 站点复制 — Bucket + IAM 跨集群同步
- [ ] **4.3** ILM 生命周期 — 对象自动过期/分层
- [ ] **4.4** Batch Jobs — 批量操作
- [ ] **4.5** S3 Select — SQL 查询引擎 (可选，复杂度极高)

---

## 进度追踪

| Phase | 状态 | 开始 | 完成 | 备注 |
|-------|------|------|------|------|
| 1     | 🔴 待开始 | - | - | 核心存储 |
| 2     | 🔴 待开始 | - | - | 分布式 |
| 3     | 🔴 待开始 | - | - | IAM |
| 4     | 🔴 待开始 | - | - | 高级特性 |

---

## 关键风险

| 风险 | 影响 | 缓解 |
|------|------|------|
| xl.meta 格式细节理解偏差 | 数据写坏/读不出 | 先写独立解析工具验证 |
| EC 分片对齐与填充逻辑 | 往返编解码错误 | TDD: 先写 roundtrip 测试 |
| 分布式锁正确性 | 数据竞争/脑裂 | Phase 2 先不做自动解配 |
| 性能与 Go 版差距大 | 学习价值下降 | 不作为 Phase 1 目标 |

---

## 参考资源

- MinIO 原版 `docs/ARCHITECTURE.md`
- MinIO 原版 `docs/STORAGE_IAM_SPEC.md`
- MinIO 原版 `cmd/xl-storage-format-v2.go`
- [Reed-Solomon Erasure Coding (klauspost/reedsolomon)](https://github.com/klauspost/reedsolomon)
- [AWS S3 API Reference](https://docs.aws.amazon.com/AmazonS3/latest/API/)
