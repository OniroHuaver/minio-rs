# Healing 指标与 Admin API 规格（双通道）

> **Healing** 对外使用两套相互独立的数据通道。累计类指标经 **Metrics V3**（`/minio/metrics/v3/...`）供 Prometheus 抓取；明细与状态机经 **Admin API**。架构细节见 `docs/metrics_architecture_spec.md`。代码路径与 Go 行号仍指向原版，便于对照实现。

---

## 1. 两套数据通道对比

| 维度 | Prometheus Metrics（V3） | Admin Heal API |
|------|-------------------------|----------------|
| **数据来源** | `scannedItemsMap` / `healedItemsMap` / `healFailedItemsMap` 等累计结构 | `currentStatus.Items`（`[]madmin.HealResultItem`） |
| **数据性质** | Counter / Gauge 等**累计**指标，拉取**不消费** | **流式**结果，**消费后清空** |
| **消费方式** | Prometheus 定期 **scrape** `/minio/metrics/v3/...` | 客户端 **poll**（长轮询式多次 POST） |
| **典型用途** | 监控告警、容量趋势 | 控制台 / CLI 实时查看每个对象的 heal 结果 |

设计原则：**监控走 V3 累计指标；交互式排障走 Admin 流**；二者勿混用同一缓冲区，避免 scrape 破坏实时 UI。

---

## 2. Prometheus（累计通道，Metrics V3）

Healing 相关**累计**指标通过 **`/minio/metrics/v3/...`** 下的 **MetricsGroup** 暴露（描述符命名与标签对齐 MinIO V3）；具体子路径以实现阶段注册的 `newMetricGroups()` 为准。

认证由 **`MINIO_PROMETHEUS_AUTH_TYPE`** 控制（默认 JWT，可 `public`），与其它 V3 metrics 相同。

---

## 3. Admin API（流式通道）

### 3.1 手动 Heal 序列：`HealHandler`

- **路由**：`POST /minio/admin/v3/heal/{bucket}/{prefix}`（见 `admin-router.go` 约第 175–178 行）。
- **协议**（基于 `clientToken`）：
  1. **首次**：无 token → 启动 `healSequence` → 返回 `clientToken` + `StartTime`。
  2. **第 N 次**：带 token → `PopHealStatusJSON` → 返回 `currentStatus.Items`（**弹出后清空**）。
  3. **停止**：`forceStop` → `stopHealSequence`，取消 context。

实现参考 `admin-heal-ops.go`（约第 357–405 行）：每次 `PopHealStatusJSON` 后清空 `currentStatus.Items`，形成**显式消费语义**。

### 3.2 后台 Heal 状态：`BackgroundHealStatusHandler`

- **路由**：`POST /minio/admin/v3/background-heal/status`。
- **响应**：`madmin.BgHealState`，包含例如：
  - **ScannedItemsCount**：`bgSeq.getScannedItemsCount()`（各类型扫描累计之和）。
  - **HealDisks**：当前正在 healing 的磁盘列表。
  - **Sets**：每个 erasure set 的磁盘状态。

分布式模式下经 `globalNotificationSys.BackgroundHealStatus()` 向各 peer 收集再合并（见 `admin-handlers.go` 约第 1464–1510 行）。本地状态由 `getLocalBackgroundHealStatus` 构建（`global-heal.go` 约第 77–141 行）。

---

## 4. 背压与客户端责任

`pushHealResultItem` 在 `Items` 积压超过 **`maxUnconsumedHealResultItems`（如 1000）** 时**阻塞** heal goroutine；若超过 **`healUnconsumedTimeout`（如 24h）** 仍未被消费则中止序列（见 `admin-heal-ops.go` 约第 47–59 行）。

**含义**：

- Admin 通道是**有界队列 + 阻塞生产者**；客户端必须持续 poll，heal 才能推进。
- V3 Prometheus 通道无此消费要求；二者职责分离。

---

## 5. madmin 客户端轮询示例

参考 `buildscripts/heal-manual.go`（约第 44–85 行）模式：

1. `Heal(ctx, bucket, "", opts, "", false, false)` — 启动，取 `clientToken`。
2. `Heal(ctx, bucket, "", opts, clientToken, false, false)` — 轮询 `Items`。
3. 直到 `status.Summary == "finished"` 或 `"stopped"`。

minio-rs：Admin 层应保证 **JSON 字段与状态机语义** 与上述兼容，便于现有 `mc` 与脚本复用。

---

## 6. 架构关系小结

```text
                    ┌─────────────────────┐
  heal workers ───► │ 内存状态            │
                    │ scanned/healed/...  │──► GET /minio/metrics/v3/...（累计，不清空）
                    │ currentStatus.Items │──► PopHealStatusJSON（消费即清空）
                    └─────────────────────┘
```

**关键决策**：

1. V3 指标为**累计值**，不因 Admin 消费而重置 → 适合长期趋势与告警。
2. `currentStatus.Items` 为**一次性流** → 适合实时对象级展示。
3. **背压**：`pushHealResultItem` 阻塞生产者，防止无界内存；流式客户端必须跟读。
