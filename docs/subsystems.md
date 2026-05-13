# MinIO 子系统规格

> 本文档整合 MinIO 原版文档中 20+ 个子系统的零散文档，从 Rust 重写视角提炼核心规格与实现要点。

---

## 1. 事件通知 (来自 bucket/notifications/)

**功能概述**

MinIO 支持对 bucket 内对象操作发布事件通知。支持的事件类型包括：`s3:ObjectCreated:*`（Put、Post、Copy、CompleteMultipartUpload）、`s3:ObjectRemoved:*`（Delete、DeleteMarkerCreated）、`s3:ObjectAccessed:*`（Get、Head）、以及 Retention/LegalHold 相关事件。还支持复制相关事件（`s3:Replication:*`）、ILM 过渡事件（`s3:ObjectRestore:*`）和全局事件（`s3:BucketCreated`、`s3:BucketRemoved`）。

**支持的 Target 与核心实现**

通知可发布到以下后端：AMQP (RabbitMQ)、MQTT、Elasticsearch、Redis、NATS (含 Streaming)、NSQ、MySQL、PostgreSQL、Apache Kafka、Webhook。每个 Target 支持 `queue_dir` / `queue_limit` 持久化事件存储，在目标离线时缓存、恢复后重放。Elasticsearch 和 Redis 支持 `namespace`（同步当前状态）和 `access`（操作日志）两种格式。

每个 notification target 在 MinIO 中注册为一个 SQS ARN（如 `arn:minio:sqs::1:amqp`），通过 `mc event add` 将 ARN 绑定到 bucket 的特定事件过滤规则（按后缀、前缀过滤）。通知消息体遵循 AWS S3 事件通知 JSON 结构。

**Rust 实现建议（Phase 2+）**

- 定义 `EventType`、`TargetConfig` 枚举和 `NotificationQueue` trait
- 每个 target 实现独立的 connector crate（如 `minio-notify-kafka`），通过 trait object 注册
- 持久化队列可用文件系统 + Tokio `mpsc` channel 实现背压
- 建议 Phase 2 先实现 Webhook 和 Kafka，Phase 3 补充数据库类 target

---

## 2. 生命周期管理 ILM (来自 bucket/lifecycle/)

**功能概述**

MinIO 支持 S3 兼容的 Bucket Lifecycle Configuration 规则，可自动执行对象的过期删除（Expiration）和过渡到冷存储层（Transition）。规则支持按前缀（Prefix）、标签（Tag）过滤，按天数或指定日期触发。扩展功能包括：NoncurrentVersionExpiration（自动清除非当前版本，可选保留最近 N 个版本）、ExpiredObjectAllVersions（过期的对象清除所有版本）、ExpiredObjectDeleteMarker（自动清理孤立的删除标记）。

**Transition 分层设计**

Transition 功能允许将对象从本地 MinIO 集群过渡到远程存储层（GCS、AWS S3、Azure Blob、另一 MinIO 集群）。Tier 通过 `mc admin tier add` 注册。过渡后对象元数据保留在本地，数据存储在远端，路径格式为 `{uuid[0:2]}/{uuid[2:4]}/{uuid}`。由 MinIO Scanner（每分钟、每次扫描 1/16 命名空间）拾取符合条件的对象执行过渡。RestoreObject API 可将过渡对象临时恢复到本地。

内部元数据存储在 `xl.meta` 中，包含 `x-minio-internal-transition-status`、`x-minio-internal-transition-tier`、`x-minio-internal-transitioned-object` 等字段。加密对象在过渡时保持密文不动。

**Rust 实现建议（Phase 2-3）**

- `LifecycleRule` 结构体直接映射 JSON 配置，使用 `serde` 解析
- Scanner 组件使用 Tokio 间隔任务，管理扫描进度与限速
- Transition 后端抽象为 `StorageTier` trait，初始实现本地文件 + S3 兼容
- 注意 NoncurrentVersion 清除的效率：需要按 version 排序、计数

---

## 3. 对象复制 (来自 bucket/replication/)

**功能概述**

Bucket Replication 将源 bucket 中符合条件的对象同步到目标 bucket（可跨集群）。支持单向（Active-Passive）和双向（Active-Active）复制；支持多目标复制（一个源 bucket 到多个目的 bucket）；支持复制 DeleteMarker 和版本化删除（MinIO 对 S3 V2 配置的扩展）；支持现有对象复制（ExistingObjectReplication）和重同步（Resync）。

**核心设计**

复制依赖版本化（versioning）保证不可变性。对象写入后状态流转：`PENDING` -> `COMPLETED` / `FAILED`。目标端对象标记为 `REPLICA`。复制异步执行（默认），也可配置同步模式（`--sync`）。复制失败由 Scanner（每分钟，每次 1/16 命名空间）自动重试。可配置 `replication_workers`（默认 100）和 `replication_max_lrg_workers`（默认 10）。

Active-Active 模式下，自动故障转移：`GET/HEAD` 时若对象在本地缺失但目标端存在，自动代理请求。复制配置中 `ReplicaModifications` 控制元数据变更是否回写源端。内部元数据存储在 `xl.meta` 的 `x-minio-internal-replication-status` 字段中，以 base64 编码的 ARN:状态 键值对序列表示多目标状态。

**Rust 实现建议（Phase 2-3）**

- 复制队列用 Tokio `mpsc` + 持久化 WAL 实现，确保重启不丢失
- `ReplicationRule` 结构体映射 JSON 配置，支持 `Filter`、`Destination`、`DeleteMarkerReplication`、`DeleteReplication` 等字段
- 复制 worker 池使用 `tokio::sync::Semaphore` 限流
- Scanner 作为低优先级后台任务，不阻塞前台请求
- 代理 (proxy) 转发使用 `reqwest` + 流式传输，避免全量缓冲

---

## 4. 对象锁定与保留 (来自 bucket/retention/)

**功能概述**

MinIO 支持 WORM（Write Once Read Many）模型，通过 Object Lock 实现对象不可变性。需要在创建 bucket 时启用（`mc mb --with-lock`），启用后自动开启版本化且不可关闭。支持两种保留模式：**Governance**（特殊权限可覆盖）和 **Compliance**（任何人不可删除，直到保留期结束）。此外支持 **Legal Hold**（独立于保留期，显式设置/移除）。

**核心机制**

Bucket 级默认保留配置通过 `PutObjectLockConfiguration` API 设置，新对象自动继承。对象级可通过请求头 `x-amz-object-lock-mode` / `x-amz-object-lock-retain-until-date` / `x-amz-object-lock-legal-hold` 覆盖桶默认配置。`PutObjectRetention` API 可在上传后修改保留设置。`MINIO_NTP_SERVER` 环境变量确保保留时间的准确性。

**Rust 实现建议（Phase 2）**

- `ObjectLockConfig` 和 `Retention` 结构体直接映射 S3 XML 规范
- 在 `PutObject` / `DeleteObject` 路径中检查 retention 和 legal hold
- Retention 日期比较依赖可靠的时钟源（NTP）
- 与版本化深度集成：每个版本独立的 retention 元数据

---

## 5. Bucket 配额 (来自 bucket/quota/)

**功能概述**

MinIO 支持为每个 bucket 设置 `Hard` 配额。达到配额上限后，写入操作被拒绝。配额通过 `mc admin bucket quota` 命令管理。

**实现要点**

配额基于 Scanner 收集的 bucket 用量数据判断。硬配额（Hard）在写入时检查当前用量是否已超出限制。Rust 实现中，配额检查可作为 Middleware 层，在 `PutObject` / `CompleteMultipartUpload` 等写操作入口拦截。配额元数据持久化在 bucket 配置中。由于 Scanner 用量的更新有延迟，配额判断接近但不保证精确。

---

## 6. 批量作业 (来自 batch-jobs/)

**功能概述**

MinIO Batch Job 提供了大规模对象管理框架，目前支持 **Replication Job**（批量跨站点复制），后续计划支持从 NAS/HDFS 导入。作业通过 YAML 描述文件定义源、目标、过滤条件，支持重试、实时进度监控和完成通知。

**作业定义核心结构**

```yaml
replicate:
  apiVersion: v1
  source:
    type: "minio"
    bucket: BUCKET
    prefix: PREFIX
  target:
    type: "minio"
    bucket: BUCKET
  flags:
    filter:
      newerThan: "7d"
      olderThan: "7d"
    notify:
      endpoint: "https://notify.endpoint"
    retry:
      attempts: 10
      delay: "500ms"
```

`mc batch` 命令管理作业生命周期：`generate` / `start` / `list` / `status` / `describe`。作业运行状态包括实时吞吐量、已传输数据量、已处理对象数。

**Rust 实现建议（Phase 2）**

- YAML 解析使用 `serde_yaml`，`BatchJob` 是顶层枚举（`Replicate`、`CopyFromNAS` 等变体）
- 作业执行引擎使用 Tokio 任务，支持暂停/取消和断点续传
- 进度通过 `mpsc` 上报给 API 层

---

## 7. 压缩 (来自 compression/)

**功能概述**

MinIO 支持流式压缩，在对象写入磁盘前以 streaming 方式压缩。使用 [`klauspost/compress/s2`](https://github.com/klauspost/compress/tree/master/s2) 算法，专门针对机器生成内容优化。写入吞吐量 >= 500MB/s/核心，解压 >= 1GB/s。机械硬盘场景下，压缩减少了磁盘 I/O，可提升整体吞吐量。

**配置与排除规则**

通过 `compression` 配置子系统设置，支持按扩展名和 MIME 类型白名单。默认包含 `.txt .log .csv .json .tar .xml .bin` 和 `text/*` `application/json` `application/xml`。已压缩格式（`.gz .bz2 .rar .zip .7z .xz .mp4 .mkv .mov` 和 `video/*` `audio/*` 等）自动排除。加密 + 压缩默认禁用（CRIME 攻击风险），需显式开启 `allow_encryption=on`。

**Rust 实现建议（Phase 1）**

- 使用 `snap`、`zstd` 或纯 Rust 的 `flate2` crate
- 压缩作为 `ObjectLayer` 中的透明 wrapper：写入时 compress，读取时 decompress
- 检查 `Content-Encoding` 和扩展名决定是否压缩
- `CompressionConfig` 作为全局配置结构体

---

## 8. 配置子系统 (来自 config/)

**功能概述**

MinIO 配置子系统提供统一的分层配置管理。配置存储在后端擦除编码的数据中，支持通过 `mc admin config` 命令交互。若配置了 KMS，配置和 IAM 数据会被加密。配置格式为子系统/键值对。主要子系统包括：`api`（API 限流、复制 worker 数等）、`heal`（修复频率、bitrot 扫描）、`scanner`（扫描延迟、周期）、`site`（站点标签、区域）、`storage_class`（EC 奇偶校验级别）、`etcd`（联合 IAM/Bucket DNS）、notify_* 系列（通知后端）、压缩等。

TLS 证书目录默认为 `${HOME}/.minio/certs`。支持 YAML 配置文件（`minio server --config config.yaml`）作为命令行参数的替代。

**动态子系统**

`api`、`heal`、`scanner` 三个子系统支持运行时热更新（无需重启）。环境变量的优先级高于 `config.yaml`，`config.yaml` 高于内部 KV 配置。

**Rust 实现建议（Phase 1）**

- `Config` 结构体使用 `serde` 反序列化，支持 YAML / 环境变量 / KV 三种来源
- 实现 `ConfigProvider` trait，合并多来源配置
- 热更新子系统使用 `tokio::watch` channel 传播配置变更
- 配置变更需要版本化支持（当前有 `version: v2`）

---

## 9. S3 扩展 (来自 extensions/)

### 9.1 Fan-Out 上传

MinIO 实现 PostUpload API 的扩展，支持从单个数据流并发写入多个对象（fan-out）。主要场景为 TSB（Time Shift Buffer）分发。请求中添加 `x-minio-fanout-list` form-field 启用。SDK 层提供 `PutObjectFanOut()` 高级 API。每个目标对象上传完成后独立可用。

### 9.2 S3 ZIP 访问

MinIO 实现 S3 扩展，允许将 ZIP 文件作为虚拟目录，直接 List/Stat/Get ZIP 内部文件。请求头 `x-minio-extract: true` 启用。路径格式：`bucket/archive.zip/inner/file.txt`。

限制：只支持读操作（HeadObject、GetObject、ListObjectsV2）；Range 请求不支持；ZIP 目录必须在文件末尾 100MB 以内；单个 ZIP 最多支持 100,000 个文件（推荐上限）。所有属性（修改时间、标签等）绑定到 ZIP 文件整体，复制也是整体复制。

**Rust 实现建议（Phase 2）**

- Fan-Out：在 PutObject 路径中解析 `x-minio-fanout-list`，生成多个并发写入任务
- S3 ZIP：使用 `zip` crate 解析 ZIP 中央目录，按路径匹配返回对应 entry 的 reader

---

## 10. Lambda 通知 (来自 lambda/)

**功能概述**

Object Lambda 允许在 GetObject 返回数据前，通过外部 Lambda 函数对数据进行实时转换。适用于数据脱敏、格式转换、数据富化等场景。Lambda 函数是一个标准的 HTTP Webhook Endpoint，MinIO 在 GET 请求时调用该函数完成数据转换后返回给客户端。

**调用流程**

1. 客户端发起带 `lambdaArn` 参数的 Presigned GET 请求
2. MinIO 构造事件上下文 JSON，包含 `inputS3Url`（预签名 URL）、`outputRoute` 和 `outputToken`
3. 调用 Lambda Webhook Endpoint
4. Lambda 从 `inputS3Url` 获取原始对象，处理后将结果通过响应头 `x-amz-request-route` + `x-amz-request-token` 返回
5. MinIO 验证 token 后将转换后数据返回给客户端

支持静态 Token 认证和 mTLS 认证。Lambda 目标通过环境变量 `MINIO_LAMBDA_WEBHOOK_ENABLE_<name>` 配置。ARN 格式：`arn:minio:s3-object-lambda::<name>:webhook`。

**Rust 实现建议（Phase 2）**

- Lambda 目标注册到 `LambdaTargetRegistry`，每个 target 是 `Arc<dyn LambdaHandler>`
- 调用使用 `reqwest` 异步 HTTP 客户端
- `inputS3Url` 的生成需要预签名 URL 能力

---

## 11. 审计日志 (来自 auditlog/)

**功能概述**

MinIO 支持将审计日志发送到 HTTP Webhook 或 Kafka 目标。审计日志包含每次 API 调用的完整上下文：请求/响应头、用户信息、对象路径、状态码、耗时（纳秒级）、传输字节数、以及擦除编码场景下的 pool/set/drives 位置信息。

**日志格式**

```json
{
  "version": "1",
  "deploymentid": "90e81272-...",
  "time": "2024-05-09T07:38:10.449688982Z",
  "api": {
    "name": "PutObject",
    "bucket": "testbucket",
    "object": "hosts",
    "status": "OK",
    "statusCode": 200,
    "timeToResponseInNS": "13309747"
  },
  "remotehost": "127.0.0.1",
  "tags": {
    "objectLocation": {
      "poolId": 1,
      "setId": 1,
      "drives": ["/mnt/data1", "/mnt/data2"]
    }
  }
}
```

`auditlog-echo` 是官方提供的调试工具，在控制台实时展示审计日志。审计 Webhook Target 和审计 Kafka Target 都通过 `mc admin config set` 配置。

---

## 12. 日志系统 (来自 logging/)

**功能概述**

MinIO 支持两种日志输出目标：**console**（始终启用，不可关闭）和 **HTTP webhook**（默认关闭）。HTTP 日志以 JSON 格式发送到配置的 webhook endpoint。通过 `logger_webhook` 配置子系统管理。

审计日志（Audit Log）和普通日志分离。审计日志有独立的 Target 配置（`audit_webhook` / `audit_kafka`），内容更详细。普通日志聚焦于系统运行信息。

**Rust 实现建议（Phase 1）**

- 使用 `tracing` crate 作为内部日志框架，支持结构化日志
- Console logger 默认 subscriber；Webhook logger 作为 `tracing-subscriber` 的自定义 layer
- 审计日志单独 channel，与请求处理路径解耦

---

## 13. 指标与监控 (来自 metrics/)

**功能概述**

指标通过 **Metrics V3** 暴露：`/minio/metrics/v3/<category>`，按路径组织类别，例如 `/api/requests`、`/bucket/api`、`/audit`、`/cluster/*`、`/ilm`、`/logger/webhook`、`/notification`、`/replication`、`/scanner`、`/system/*`（drive/cpu/memory/network/process）。

**关键指标类别**

- **请求指标**：拒绝数、inflight 数、错误数（4xx/5xx）、TTFB 分布、流量（收发字节）
- **集群指标**：擦除集健康度、读写 quorum、驱动机在线/离线/修复中、IAM 同步状态
- **系统指标**：每块盘的 IOPS、延迟、利用率；CPU 负载/iowait；内存使用；进程级 goroutine 数、文件描述符
- **ILM/复制指标**：队列深度、活跃 worker 数、传输速率、未完成任务

**Rust 实现建议（Phase 1-2）**

- 使用 `prometheus` crate（`prometheus-client`）暴露 metrics
- `/minio/metrics/v3/*` 在 Axum router 中按 prefix group 注册
- 系统指标采集使用 `sysinfo` crate
- 擦除集指标从 `ErasureSet` 内部状态聚合

---

## 14. 速率限制 (来自 throttle/)

**功能概述**

MinIO 支持对 API 层进行限流：限制集群级别并发请求数，控制请求在队列中的最大等待时间。通过 `api` 子系统的 `requests_max` 参数配置。

默认值为 `auto`（根据可用 RAM 自动计算）。示例：为机械硬盘部署限制 1600 并发请求，将高并发 I/O 转换为顺序 I/O，提升响应可预测性。

```sh
mc admin config set myminio/ api requests_max=1600
```

`requests_max=0` 表示自动计算。

**Rust 实现建议（Phase 1）**

- 使用 `tokio::sync::Semaphore` 实现并发请求限制
- 在 HTTP 中间件层获取 permit，请求完成后释放
- 队列等待超时通过 `tokio::time::timeout` 控制
- 注意 Semaphore 是公平的，避免请求饥饿

---

## 15. TLS 配置 (来自 tls/)

**功能概述**

MinIO 支持标准 HTTPS/TLS，证书从 `${HOME}/.minio/certs/` 目录加载。支持 PEM 格式（不支持 PFX）。支持多证书（SNI）、自签名证书、Let's Encrypt 自动续期、以及第三方 CA 信任链。

**证书目录结构**

```
~/.minio/certs/
  ├─ CAs/          # 第三方 CA 证书
  ├─ private.key   # 服务端私钥
  └─ public.crt    # 服务端公钥证书
```

密码保护的私钥通过 `MINIO_CERT_PASSWD` 环境变量提供。支持使用 `certgen` 工具快速生成 SAN 证书。MinIO 还支持 mTLS（客户端证书认证）。

**Rust 实现建议（Phase 1）**

- 使用 `rustls` + `tokio-rustls` 处理 TLS
- 证书热重载使用 `inotify` / `kqueue` 监控文件变化
- 支持 SNI 多证书（`rustls::ServerConfig` 的 `CertifiedKey` 路由）
- CAs 目录作为 `rustls::RootCertStore` 加载

---

## 16. 性能调优 (来自 tuning/)

**功能概述**

MinIO 提供 Linux `tuned` 性能调优配置，通过 `tuned-adm profile minio` 激活。`tuned.conf` 配置包含针对 MinIO 存储工作负载优化的内核参数（I/O 调度器、网络缓冲区、虚拟内存设置等）。

**Rust 实现建议**

- 在文档中提供 Linux `tuned` 配置参考
- 某些调优参数可作为 MinIO Rust 版本启动时的建议（如 `net.core.rmem_max`、`vm.dirty_ratio`）
- 系统级调优在容器外部署时相关，Kubernetes 部署建议使用 `sysctl` init container

---

## 17. 多租户 (来自 multi-tenancy/)

**功能概述**

MinIO 支持三种多租户部署模式：**单机多租户**（单机多端口，各租户独立进程）、**分布式多租户**（多节点上运行多组分布式 MinIO 实例）、**云规模多租户**（Kubernetes Operator）。每个租户有自己的端口、配置、数据目录和 root 凭证。

**示例（分布式多租户）**

```sh
export MINIO_ROOT_USER=<TENANT1_ACCESS_KEY>
export MINIO_ROOT_PASSWORD=<TENANT1_SECRET_KEY>
minio server --address :9001 http://192.168.10.1{1...4}/data/tenant1

export MINIO_ROOT_USER=<TENANT2_ACCESS_KEY>
export MINIO_ROOT_PASSWORD=<TENANT2_SECRET_KEY>
minio server --address :9002 http://192.168.10.1{1...4}/data/tenant2
```

**Rust 实现建议**

- Rust 版 MinIO 的默认模型是多租户友好：每个实例管理一组池和凭证
- 多租户更多是运维层面的隔离（进程/容器/K8s namespace）
- 租户间的 IAM 策略、密钥完全隔离，不共享状态

---

## 18. 多用户 (来自 multi-user/)

**功能概述**

MinIO 支持 IAM 用户和组的创建、管理和权限控制。用户关联策略（Policy），策略基于 AWS IAM 策略语法（JSON）。支持组级策略和策略变量（`${aws:username}`、`${jwt:*}`、`${ldap:*}`）。

**策略变量**

- `aws:username` / `aws:groups`：当前用户/组
- `jwt:*`：OpenID Connect JWT claims（sub、iss、aud、preferred_username 等）
- `ldap:username` / `ldap:groups`：LDAP/AD 用户属性
- `aws:CurrentTime`、`aws:SourceIp`、`aws:SecureTransport`：请求上下文变量

**用户操作命令**

```sh
mc admin user add myminio newuser newuser123
mc admin policy attach myminio getonly --user=newuser
mc admin user disable myminio newuser
mc admin user remove myminio newuser
```

**Rust 实现建议（Phase 1-2）**

- IAM 策略引擎核心使用开源 `cedar-policy` 或自定义实现 AWS IAM 策略语法解析
- 用户/组/策略存储在 `IAMSys` 中，支持 etcd（联合）和后端存储两种模式
- 策略变量在鉴权时动态求值

---

## 19. 联邦模式 (来自 federation/)

**功能概述**

MinIO 联邦模式允许将多个集群联合为一个逻辑命名空间，通过 DNS 风格 bucket 查找实现。依赖 etcd 存储 bucket DNS SRV 记录和 IAM 数据。CoreDNS 可选用于 DNS 解析。

已标记为 **deprecated**，不推荐在新部署中使用。当前更推荐使用 Bucket Replication 实现跨集群数据同步。

**核心配置**

```sh
export MINIO_ETCD_ENDPOINTS="http://remote-etcd1:2379,http://remote-etcd2:4001"
export MINIO_DOMAIN=domain.com
export MINIO_PUBLIC_IPS=44.35.2.1,44.35.2.2
minio server http://rack{1...4}.host{1...4}.domain.com/mnt/export{1...32}
```

**Rust 实现建议**

- 联邦模式为已弃用功能，Rust 版本不强制实现
- 如需兼容，Phase 3 可考虑实现 etcd-backed bucket DNS lookup
- 建议新架构使用全局 bucket namespace + 跨集群复制作为替代方案

---

## 20. FTP 接口 (来自 ftp/)

**功能概述**

MinIO 原生支持 FTP/FTPS/SFTP 协议，允许标准 FTP 客户端直接上传和下载文件。支持的操作：`get`、`put`、`ls`、`mkdir`、`rmdir`、`delete`。不支持 `append` 和 `rename`。

**实现特点**

- 所有 IAM 用户均可通过 FTP 登录（旋转凭证除外）
- 权限受 IAM 策略约束
- 版本化 bucket 上仅操作最新版本
- SSE 加密和复制功能不受影响
- 通过 `--ftp` 和 `--sftp` 命令行参数配置

**SFTP 高级选项**

支持自定义算法：`pub-key-algos`、`kex-algos`、`cipher-algos`、`mac-algos`。支持证书认证（TrustedUserCAKeys）。FTP 默认不安全，可通过 `--ftp="tls-private-key=..." --ftp="tls-public-cert=..."` 启用 TLS。

**Rust 实现建议（Phase 2-3）**

- FTPS 使用 `rustls` 加密通道
- SFTP 使用 `ssh2` crate 或纯 Rust 的 `russh`
- FTP/SFTP 作为可选的 frontend 模块，与 S3 共享底层 `ObjectLayer`

---

## 21. chroot 隔离 (来自 chroot/)

**功能概述**

chroot 提供用户级别的文件系统 namespace 隔离。MinIO 标准二进制可部署在 chroot jail 中运行。构建后复制二进制到 chroot 目录，`mount --bind /proc` 后通过 `chroot` 命令启动。

```sh
sudo chroot --userspec username:group /mnt/export/${USER} /bin/minio server /data
```

**Rust 实现建议**

- Rust 二进制为静态编译（musl target），天然适合 chroot 部署
- chroot 隔离是运维层面配置，建议在部署文档中说明
- 容器化部署（Docker/K8s）是更推荐的隔离方式

---

## 22. Console / Web 管理控制台 (来自 console/)

**功能概述**

MinIO 提供基于 Web 的管理控制台（Console），支持可视化管理 Bucket、对象、用户、策略、配置、监控等。Console 在 Go 原版中是一个独立进程（默认监听 `:13333`），MinIO Server 在 `:9000` 检测到浏览器请求后 307 重定向到 Console 端口。

**核心架构**

- **前端**：React (TypeScript) SPA，编译后通过 `//go:embed` 嵌入 Console 二进制
- **后端**：REST API（`/api/v1/*`），通过 STS 临时凭证代理 S3 操作
- **认证**：表单登录 + OAuth2/IDP 登录 → STS AssumeRole → AES-GCM 加密的 session cookie
- **中间件链**：Gzip → 审计日志 → 文件服务（API/WS/SPA 路由分发）→ 上下文注入 → 认证 → 安全头
- **WebSocket**：`/ws` 提供实时对象浏览、监控指标、日志流

**API 模块**

| 模块 | 端点 |
|------|------|
| 认证 | `/api/v1/login`, `/logout`, `/session` |
| 用户/组/策略 | `/api/v1/users`, `/groups`, `/policies` |
| Bucket | `/api/v1/buckets/*` (CRUD + 版本控制 + 加密 + 复制 + 生命周期 + 事件) |
| 对象 | `/api/v1/buckets/{name}/objects/*` (浏览/上下传/批量删除/分享/元数据) |
| 配置 | `/api/v1/configs/*` (CRUD + 导入/导出 + 重置) |
| 管理 | `/api/v1/service/restart`, `/profiling/*`, `/admin/info`, `/nodes` |
| 站点复制 | `/api/v1/admin/site-replication` |
| Tier/KMS/IDP | `/api/v1/admin/tiers`, `/kms/*`, `/idp/*` |

**Rust 实现建议（Phase 3）**

详见 [`docs/console_spec.md`](console_spec.md)。
- Phase 3 采用内嵌模式（Console 路由挂载到 `:9000` 的 axum Router），降低部署复杂度
- 认证从 AES-GCM 加密 token 改为 JWT，复用 S3 层的 `jsonwebtoken` 依赖
- 前端 Phase 3 初期用最小 HTML 页面，后续嵌入上游 React SPA
- Console API handler 放在 `src/console/` 模块，与现有 S3 路由合并

---

## 23. Rust 实现路线图

| Phase | 子系统 | 说明 |
|-------|--------|------|
| Phase 1 | TLS、日志、配置、指标、压缩、速率限制 | 基础设施层，http server 启动即可集成 |
| Phase 1-2 | 多用户（IAM） | 核心鉴权，影响所有 API 请求 |
| Phase 2 | 事件通知（Webhook/Kafka）、ILM、Bucket 配额、批量作业、Lambda、S3 扩展 | 与 bucket 操作深度集成 |
| Phase 2-3 | 对象复制、生命周期 Transition | 高阶功能，依赖 Scanner 和版本化 |
| Phase 2-3 | FTP/SFTP | 可选协议 frontend |
| Phase 3 | 联邦模式（可选，已弃用） | 向后兼容 |

**跨阶段依赖**

- Scanner 组件（ILM Transition、复制重试、配额更新）在 Phase 2 实现
- 版本化是复制和 ILM 非当前版本管理的前提
- IAM 策略引擎是 Lambda、FTP、多用户的基础
