# Prometheus Metrics 架构规格（Metrics V3）

> 本文描述 **Metrics V3**（`/minio/metrics/v3/...`）的采集、分组缓存与 Prometheus 输出，作为 minio-rs 对齐基准。代码注释与标识符用英文；本规格用中文书写。

---

## 1. 总体流水线

**数据采集 → 分组缓存 → Prometheus 文本格式输出**

- **采集**：从全局子系统、HTTP 包装器、环形时间窗等读取原子计数或快照。
- **分组**：按 **V3 `collectorPath`**（URL 路径后缀）划分；各 group 可对昂贵计算做短期缓存（对标原版 `cachevalue.Cache` 用法）。
- **输出**：编码为 Prometheus exposition 格式（对标 `expfmt`）。

参考实现文件（Go）：`cmd/metrics-v3.go`、`cmd/metrics-v3-types.go`、`cmd/metrics-v3-handler.go`。认证与入口路由可对标 `cmd/metrics-router.go` 中与 V3 相关的注册逻辑。

---

## 2. HTTP 端点与认证

| 路径前缀 | 说明 |
|----------|------|
| `/minio/metrics/v3` | 根：可聚合全部已注册 V3 子路径（行为对标原版 handler） |
| `/minio/metrics/v3/{path...}` | 子路径：路径即指标命名空间 |

环境变量 **`MINIO_PROMETHEUS_AUTH_TYPE`**：

- **`jwt`（默认）**：Bearer Token + IAM `PrometheusAdminAction`（对标 `mc admin prometheus generate`）。
- **`public`**：不对 metrics 端点做认证。

minio-rs：**默认 JWT**，与 Admin/IAM 阶段集成后再开放 `public`。

---

## 3. V3 数据模型

类型定义见 `metrics-v3-types.go`。

### 3.1 `MetricsGroup` 与路径前缀

- **`collectorPath.metricPrefix()`**：URL 路径 → 合法 Prometheus 指标前缀（例如 `/cluster/usage` → `minio_cluster_usage_*`）。
- **`MetricsLoaderFn` / `BucketMetricsLoaderFn`** + **`JoinLoaders`**：组合多个 loader。
- **`newMetricGroups()`**：每个 group 独立 **sub-registry**，形成 **`collectorPath` → `Gatherer`** 映射（`metrics-v3.go` 约第 398–487 行）。

### 3.2 指标描述符

`MetricDescriptor`：名称、类型、Help、标签（见 `metrics-v3-types.go` 约第 107–116 行）。

| 类型名 | Prometheus 映射 | 典型用途 |
|--------|-------------------|----------|
| CounterMT | CounterValue | 单调递增：请求总数、字节累计 |
| GaugeMT | GaugeValue | 可升可降：容量、队列深度 |
| HistogramMT | CounterValue（分桶） | 分布：TTFB、延迟 |

### 3.3 动态桶过滤

查询参数 **`?bucket=a,b`**：只返回与指定桶相关的指标，降低大规模部署响应体积。

### 3.4 层级聚合

请求 **`/minio/metrics/v3/cluster`** 等父路径时，聚合所有 **`isDescendantOf("/cluster")`** 的子路径指标（`metrics-v3-handler.go` 约第 186–209 行）。

### 3.5 `?list` 元数据

`listMetrics()`（约第 98–160 行）：列出某路径下指标的名称、类型、Help、标签；支持 Markdown 表格或 JSON（`Accept` / 查询参数按原版行为对齐）。

### 3.6 路由分发

`ServeHTTP` 解析路径段与 `bucket` 查询串（`metrics-v3-handler.go` 约第 221–251 行）。

---

## 4. 请求级与时间窗采集（与端点版本无关）

HTTP 指标依赖包装 **`http.ResponseWriter`** 的 **ResponseRecorder**（`metrics.go` 等，约第 33–57 行），记录 **StatusCode**、**TTFB**、**bytesWritten**，汇总到 **`http-stats.go`** 的 **HTTPStats**（**atomic** 并发安全），按 API 操作类型聚合。

滑动窗口类指标使用**环形缓冲区**（如 `last-minute.go` 中 60×1s 桶、`ReplicationLastHour` 等），通过 **`forwardTo()`** 丢弃过期桶。

---

## 5. V3 路径清单（`metrics-v3.go` 约第 36–63 行）

实现时保持路径与语义与 MinIO 一致，便于 `mc` / Grafana 复用。

| 路径后缀 | 内容 |
|----------|------|
| `/api/requests` | HTTP 请求、TTFB 分布 |
| `/bucket/api` | 按 bucket 的 API 流量 |
| `/bucket/replication` | 按 bucket 的复制状态 |
| `/system/drive` | 磁盘用量、iostat |
| `/system/memory` | 内存 |
| `/system/cpu` | CPU 负载 |
| `/system/process` | 进程级 |
| `/system/network/internode` | 节点间网络 |
| `/cluster/health` | 节点/磁盘在线、容量健康 |
| `/cluster/usage/objects` | 对象数/大小分布 |
| `/cluster/usage/buckets` | bucket 级用量 |
| `/cluster/erasure-set` | 纠删集健康 |
| `/cluster/iam` | IAM 同步 |
| `/cluster/config` | 存储类等配置 |
| `/ilm` | 生命周期任务 |
| `/scanner` | 扫描器进度 |
| `/audit` | 审计队列 |
| `/logger/webhook` | Webhook 日志队列 |
| `/replication` | 集群复制队列 |
| `/notification` | 事件通知 |

**Healing 累计类指标**：归入 V3 中与 MinIO 一致的 **MetricsGroup**（与集群健康、扫描、擦除集等分组对齐）。与 Admin 流式通道的分工见 `docs/heal_metrics_spec.md`。

---

## 6. minio-rs 实现建议（摘要）

| 主题 | 建议 |
|------|------|
| Crate | `prometheus-client` 或 `prometheus` + axum `Router` |
| 路由 | 注册 `/minio/metrics/v3/*`；父路径聚合、`?list`、`?bucket=` 与原版语义一致 |
| 缓存 | 按 group 配置 TTL，避免 `Collect` 热路径重算 |
| 认证 | `MINIO_PROMETHEUS_AUTH_TYPE`，默认非 `public` |
| Healing | 累计 → V3；对象级流式 → Admin API |

---

## 7. 相关文档

- `docs/heal_metrics_spec.md` — Healing 双通道（V3 累计 vs Admin Heal API）
- `docs/api_reference.md` — §6 Metrics API
- `docs/subsystems.md` — §13 指标与监控概览
