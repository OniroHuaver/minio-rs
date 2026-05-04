# MinIO API 接口文档 (Rust 重写版)

> 基于原 MinIO Go `master` 分支 (2026-05) 翻译整理
> Rust 重写项目: minio-rs | Crate 结构参考 `./ARCHITECTURE.md`
> API 前缀：S3 `/`, Admin `/minio/admin/v3/`
> 参考实现：原版 `cmd/api-router.go:255`, `cmd/admin-router.go:138`

---

## 目录

1. [S3 API](#1-s3-api)
2. [Admin API](#2-admin-api)
3. [STS API](#3-sts-api)
4. [KMS API](#4-kms-api)
5. [Health API](#5-health-api)
6. [Metrics API](#6-metrics-api)
7. [认证体系](#7-认证体系)
8. [错误码枚举](#8-错误码枚举)

---

## 1. S3 API

所有路由遵循 AWS S3 REST API 规范，支持**路径风格** (`/{bucket}/{object}`) 和**虚拟主机风格** (`{bucket}.domain/...`)。Rust 路由使用 `axum::Router` 注册。

### 1.1 Object 操作

| 方法 | 路径 | 条件 | Go 处理器 | 说明 |
|------|------|------|-----------|------|
| `HEAD` | `/{object}` | - | `HeadObjectHandler` | 获取对象元数据 |
| `GET` | `/{object}` | `?attributes` | `GetObjectAttributesHandler` | 获取对象属性 |
| `GET` | `/{object}` | - | `GetObjectHandler` | 下载对象 |
| `PUT` | `/{object}` | - | `PutObjectHandler` | 上传对象(单PUT) |
| `PUT` | `/{object}` | `x-amz-copy-source` | `CopyObjectHandler` | 复制对象 |
| `DELETE` | `/{object}` | - | `DeleteObjectHandler` | 删除对象 |
| `GET` | `/{object}` | `?tagging` | `GetObjectTaggingHandler` | 获取对象标签 |
| `PUT` | `/{object}` | `?tagging` | `PutObjectTaggingHandler` | 设置对象标签 |
| `DELETE` | `/{object}` | `?tagging` | `DeleteObjectTaggingHandler` | 删除对象标签 |
| `GET` | `/{object}` | `?retention` | `GetObjectRetentionHandler` | 获取保留设置 |
| `PUT` | `/{object}` | `?retention` | `PutObjectRetentionHandler` | 设置保留 |
| `GET` | `/{object}` | `?legal-hold` | `GetObjectLegalHoldHandler` | 获取依法保留 |
| `PUT` | `/{object}` | `?legal-hold` | `PutObjectLegalHoldHandler` | 设置依法保留 |
| `GET` | `/{object}` | `?lambdaArn=` | `GetObjectLambdaHandler` | Lambda转换获取 |
| `POST` | `/{object}` | `?select&select-type=2` | `SelectObjectContentHandler` | SQL选择查询 |
| `POST` | `/{object}` | `?restore` | `PostRestoreObjectHandler` | 从归档层恢复 |
| `PUT` | `/{object}` | `x-amz-snowball-extract: true` | `PutObjectExtractHandler` | 上传并自动解压Zip |

> **Rust 实现注解**: Object 操作集中在 `s3::handlers::object` 模块。Phase 1 实现核心 4 个操作 (HEAD/GET/PUT/DELETE)，见 `./PLAN.md` Phase 1.5。CopyObject 和 DeleteMultipleObjects 作为 Phase 1 延伸。Tagging/Retention/LegalHold/Lambda/Select/Restore/Extract 归入 Phase 2-4。Rust handler 函数签名示例:
>
> ```rust
> // s3/src/handlers/object.rs
> pub async fn get_object(
>     State(state): State<AppState>,
>     Path((bucket, key)): Path<(String, String)>,
>     headers: HeaderMap,
>     query: Query<GetObjectParams>,
> ) -> Result<Response<Body>, S3Error> { ... }
> ```

---

### 1.2 Multipart Upload

| 方法 | 路径 | 条件 | Go 处理器 | 说明 |
|------|------|------|-----------|------|
| `POST` | `/{object}` | `?uploads` | `NewMultipartUploadHandler` | 初始化分片上传 |
| `PUT` | `/{object}` | `?partNumber=&uploadId=` | `PutObjectPartHandler` | 上传分片 |
| `PUT` | `/{object}` | `?partNumber=&uploadId=` + `x-amz-copy-source` | `CopyObjectPartHandler` | 复制分片 |
| `POST` | `/{object}` | `?uploadId=` | `CompleteMultipartUploadHandler` | 完成分片上传 |
| `DELETE` | `/{object}` | `?uploadId=` | `AbortMultipartUploadHandler` | 中止分片上传 |
| `GET` | `/{object}` | `?uploadId=` | `ListObjectPartsHandler` | 列出已上传分片 |

> **Rust 实现注解**: Multipart 全部归入 Phase 2。在 Rust 中，分片中间态存储在 `base::types::MultipartUpload` 结构中，分片数据暂存于 `.minio.sys/tmp/multipart/`。CompleteMultipartUpload 的 XML 解析使用 `quick-xml`。Phase 2 前单 PUT 最大 5 GiB，超限返回 `ErrEntityTooLarge`。

---

### 1.3 Bucket 操作

| 方法 | 路径 | 条件 | Go 处理器 | 说明 |
|------|------|------|-----------|------|
| `PUT` | `/{bucket}` | - | `PutBucketHandler` | 创建Bucket |
| `HEAD` | `/{bucket}` | - | `HeadBucketHandler` | 检查Bucket存在 |
| `DELETE` | `/{bucket}` | - | `DeleteBucketHandler` | 删除Bucket |
| `GET` | `/{bucket}` | `?location` | `GetBucketLocationHandler` | 获取位置 |
| `GET` | `/{bucket}` | `?policy` | `GetBucketPolicyHandler` | 获取Bucket策略 |
| `PUT` | `/{bucket}` | `?policy` | `PutBucketPolicyHandler` | 设置Bucket策略 |
| `DELETE` | `/{bucket}` | `?policy` | `DeleteBucketPolicyHandler` | 删除Bucket策略 |
| `GET` | `/{bucket}` | `?lifecycle` | `GetBucketLifecycleHandler` | 获取生命周期 |
| `PUT` | `/{bucket}` | `?lifecycle` | `PutBucketLifecycleHandler` | 设置生命周期 |
| `DELETE` | `/{bucket}` | `?lifecycle` | `DeleteBucketLifecycleHandler` | 删除生命周期 |
| `GET` | `/{bucket}` | `?encryption` | `GetBucketEncryptionHandler` | 获取加密配置 |
| `PUT` | `/{bucket}` | `?encryption` | `PutBucketEncryptionHandler` | 设置加密配置 |
| `DELETE` | `/{bucket}` | `?encryption` | `DeleteBucketEncryptionHandler` | 删除加密配置 |
| `GET` | `/{bucket}` | `?replication` | `GetBucketReplicationConfigHandler` | 获取复制配置 |
| `PUT` | `/{bucket}` | `?replication` | `PutBucketReplicationConfigHandler` | 设置复制配置 |
| `DELETE` | `/{bucket}` | `?replication` | `DeleteBucketReplicationConfigHandler` | 删除复制配置 |
| `GET` | `/{bucket}` | `?versioning` | `GetBucketVersioningHandler` | 获取版本控制状态 |
| `PUT` | `/{bucket}` | `?versioning` | `PutBucketVersioningHandler` | 设置版本控制 |
| `GET` | `/{bucket}` | `?object-lock` | `GetBucketObjectLockConfigHandler` | 获取对象锁配置 |
| `PUT` | `/{bucket}` | `?object-lock` | `PutBucketObjectLockConfigHandler` | 设置对象锁 |
| `GET` | `/{bucket}` | `?notification` | `GetBucketNotificationHandler` | 获取通知配置 |
| `PUT` | `/{bucket}` | `?notification` | `PutBucketNotificationHandler` | 设置通知配置 |
| `GET` | `/{bucket}` | `?tagging` | `GetBucketTaggingHandler` | 获取Bucket标签 |
| `PUT` | `/{bucket}` | `?tagging` | `PutBucketTaggingHandler` | 设置Bucket标签 |
| `DELETE` | `/{bucket}` | `?tagging` | `DeleteBucketTaggingHandler` | 删除Bucket标签 |
| `GET` | `/{bucket}` | `?events=` | `ListenNotificationHandler` | 监听事件通知(SSE) |

> **Rust 实现注解**: Bucket 操作分布在 `s3::handlers::bucket` 模块。Phase 1 仅实现 PUT/HEAD/DELETE Bucket (创建/检查/删除) 和 GET Location。Policy/Lifecycle/Encryption/Replication/Versioning/ObjectLock/Notification/Tagging 归入 Phase 2-3。Bucket 元数据持久化为 `base::types::BucketMetadata`，使用 MessagePack 编码存入 `.minio.sys/buckets/`。ListenNotification (SSE) 使用 `tokio::sync::broadcast` 实现。

---

### 1.4 List 操作

| 方法 | 路径 | 条件 | Go 处理器 | 说明 |
|------|------|------|-----------|------|
| `GET` | `/{bucket}` | - | `ListObjectsV1Handler` | 列出对象 V1 |
| `GET` | `/{bucket}` | `?list-type=2` | `ListObjectsV2Handler` | 列出对象 V2 |
| `GET` | `/{bucket}` | `?list-type=2&metadata=true` | `ListObjectsV2MHandler` | 列出对象V2+元数据 |
| `GET` | `/{bucket}` | `?versions` | `ListObjectVersionsHandler` | 列出对象版本 |
| `GET` | `/{bucket}` | `?versions&metadata=true` | `ListObjectVersionsMHandler` | 列出版本+元数据 |
| `GET` | `/{bucket}` | `?uploads` | `ListMultipartUploadsHandler` | 列出正在进行的Multipart |
| `GET` | `/` | - | `ListBucketsHandler` | 列出所有Bucket |
| `POST` | `/{bucket}` | `?delete` | `DeleteMultipleObjectsHandler` | 批量删除 |

> **Rust 实现注解**: Phase 1 实现 ListObjectsV2 (最常用) 和 ListBuckets。ListObjectsV1/V2M/DeleteMultipleObjects 为 Phase 1 延伸。Versions/Multipart 列出归入 Phase 2。Rust 实现使用 `tokio::fs::read_dir` 遍历目录并按 continuation-token 分页，响应序列化为 `quick-xml`。

---

### 1.5 MinIO 扩展

| 方法 | 路径 | 条件 | Go 处理器 | 说明 |
|------|------|------|-----------|------|
| `GET` | `/{bucket}` | `?replication-metrics` | `GetBucketReplicationMetricsHandler` | 复制指标 |
| `GET` | `/{bucket}` | `?replication-metrics=2` | `GetBucketReplicationMetricsV2Handler` | 复制指标V2 |
| `GET` | `/{bucket}` | `?replication-check` | `ValidateBucketReplicationCredsHandler` | 验证复制凭证 |
| `PUT` | `/{bucket}` | `?replication-reset` | `ResetBucketReplicationStartHandler` | 启动复制重置 |
| `GET` | `/{bucket}` | `?replication-reset-status` | `ResetBucketReplicationStatusHandler` | 复制重置状态 |

> **Rust 实现注解**: MinIO 扩展 API 全部归入 Phase 4。不阻塞核心数据路径。

---

### 1.6 Dummy/AWS 兼容 (返回空/默认值)

| API | 说明 |
|-----|------|
| `?acl` (GET/PUT/DELETE) | ACL — 返回标准响应 (不存储) |
| `?cors` (GET) | CORS — 返回空配置 |
| `?website` (GET) | 网站配置 — 返回标准响应 |
| `?accelerate` (GET) | 加速配置 — 返回标准响应 |
| `?requestPayment` (GET) | 请求付费 — 返回标准响应 |
| `?logging` (GET) | 日志 — 返回标准响应 |
| `?policyStatus` (GET) | 策略状态 — 返回是否公开 |

> **Rust 实现注解**: Dummy API **Phase 1 即可实现**，返回固定 XML 响应体即可。这些 API 用于 AWS SDK 兼容性探测，不涉及实际逻辑。

---

### 1.7 不支持的操作

以下操作返回 `ErrNotImplemented` (HTTP 501)：

`?torrent`, `?inventory`, `?metrics` (bucket-level), `?website` (PUT/DELETE), `?logging` (PUT/DELETE), `?accelerate` (PUT/DELETE), `?requestPayment` (PUT/DELETE), `?acl` (PUT/DELETE/HEAD on bucket), `?publicAccessBlock`, `?ownershipControls`, `?intelligent-tiering`, `?analytics`, `x-amz-write-offset-bytes` (AppendObject)

> **Rust 实现注解**: 这些操作在 Rust 版中也返回 `ErrNotImplemented`，路由匹配到兜底 handler 即可。

---

## 2. Admin API

前缀：`/minio/admin/v3/`，注册路径参考原版 `cmd/admin-router.go:138`。Admin API 仅支持 SigV4 认证（不支持 Presigned/JWT/Anonymous），详细认证见第 7 节。

### 2.1 服务管理

| 方法 | 路径 | Go 处理器 | 说明 |
|------|------|-----------|------|
| `POST` | `/service?action={action}` | `ServiceHandler` | 重启/停止服务(旧) |
| `POST` | `/service?action={action}&type=2` | `ServiceV2Handler` | 重启/停止服务(新) |
| `POST` | `/update?updateURL={url}` | `ServerUpdateHandler` | 更新服务(旧) |
| `POST` | `/update?updateURL={url}&type=2` | `ServerUpdateV2Handler` | 更新服务(新) |
| `GET` | `/info` | `ServerInfoHandler` | 服务器信息 |
| `GET` | `/obdinfo` | `HealthInfoHandler` | 健康诊断信息 |

### 2.2 存储管理

| 方法 | 路径 | Go 处理器 | 说明 |
|------|------|-----------|------|
| `GET` | `/storageinfo` | `StorageInfoHandler` | 存储信息 |
| `GET` | `/datausageinfo` | `DataUsageInfoHandler` | 数据使用信息 |
| `GET/POST` | `/inspect-data` | `InspectDataHandler` | 检查磁盘原始数据 |

### 2.3 修复 (Healing)

| 方法 | 路径 | Go 处理器 | 说明 |
|------|------|-----------|------|
| `POST` | `/heal/` | `HealHandler` | 修复处理 |
| `POST` | `/heal/{bucket}` | `HealHandler` | 修复Bucket |
| `POST` | `/heal/{bucket}/{prefix}` | `HealHandler` | 修复匹配前缀的对象 |
| `POST` | `/background-heal/status` | `BackgroundHealStatusHandler` | 后台修复状态 |

### 2.4 池管理

| 方法 | 路径 | Go 处理器 | 说明 |
|------|------|-----------|------|
| `GET` | `/pools/list` | `ListPools` | 列出池 |
| `GET` | `/pools/status?pool={n}` | `StatusPool` | 池状态 |
| `POST` | `/pools/decommission?pool={n}` | `StartDecommission` | 开始解配 |
| `POST` | `/pools/cancel?pool={n}` | `CancelDecommission` | 取消解配 |
| `POST` | `/rebalance/start` | `RebalanceStart` | 开始再平衡 |
| `GET` | `/rebalance/status` | `RebalanceStatus` | 再平衡状态 |
| `POST` | `/rebalance/stop` | `RebalanceStop` | 停止再平衡 |

### 2.5 IAM — 用户

| 方法 | 路径 | Go 处理器 | 说明 |
|------|------|-----------|------|
| `GET` | `/accountinfo` | `AccountInfoHandler` | 当前账户信息 |
| `PUT` | `/add-user?accessKey={k}` | `AddUser` | 添加用户 |
| `PUT` | `/set-user-status?accessKey={k}&status={s}` | `SetUserStatus` | 启用/禁用用户 |
| `DELETE` | `/remove-user?accessKey={k}` | `RemoveUser` | 删除用户 |
| `GET` | `/list-users` | `ListUsers` | 列出用户 |
| `GET` | `/list-users?bucket={b}` | `ListBucketUsers` | 列出Bucket关联用户 |
| `GET` | `/user-info?accessKey={k}` | `GetUserInfo` | 用户详情 |

### 2.6 IAM — 组

| 方法 | 路径 | Go 处理器 | 说明 |
|------|------|-----------|------|
| `PUT` | `/update-group-members` | `UpdateGroupMembers` | 更新组成员 |
| `GET` | `/group?group={n}` | `GetGroup` | 组信息 |
| `GET` | `/groups` | `ListGroups` | 列出所有组 |
| `PUT` | `/set-group-status?group={n}&status={s}` | `SetGroupStatus` | 启用/禁用组 |

### 2.7 IAM — 策略

| 方法 | 路径 | Go 处理器 | 说明 |
|------|------|-----------|------|
| `PUT` | `/add-canned-policy?name={n}` | `AddCannedPolicy` | 添加预制策略 |
| `GET` | `/info-canned-policy?name={n}` | `InfoCannedPolicy` | 策略详情 |
| `GET` | `/list-canned-policies` | `ListCannedPolicies` | 列出所有策略 |
| `GET` | `/list-canned-policies?bucket={b}` | `ListBucketPolicies` | Bucket关联策略 |
| `DELETE` | `/remove-canned-policy?name={n}` | `RemoveCannedPolicy` | 删除策略 |
| `PUT` | `/set-user-or-group-policy` | `SetPolicyForUserOrGroup` | 关联策略到用户/组 |
| `POST` | `/idp/builtin/policy/{op}` | `AttachDetachPolicyBuiltin` | 内置IDP策略操作 |

### 2.8 IAM — 服务账户

| 方法 | 路径 | Go 处理器 | 说明 |
|------|------|-----------|------|
| `PUT` | `/add-service-account` | `AddServiceAccount` | 创建服务账户 |
| `POST` | `/update-service-account?accessKey={k}` | `UpdateServiceAccount` | 更新服务账户 |
| `GET` | `/info-service-account?accessKey={k}` | `InfoServiceAccount` | 服务账户详情 |
| `GET` | `/list-service-accounts` | `ListServiceAccounts` | 列出服务账户 |
| `DELETE` | `/delete-service-account?accessKey={k}` | `DeleteServiceAccount` | 删除服务账户 |

### 2.9 IAM — IDP 配置

| 方法 | 路径 | Go 处理器 | 说明 |
|------|------|-----------|------|
| `PUT` | `/idp-config/{type}/{name}` | `AddIdentityProviderCfg` | 添加IDP配置 |
| `POST` | `/idp-config/{type}/{name}` | `UpdateIdentityProviderCfg` | 更新IDP配置 |
| `GET` | `/idp-config/{type}` | `ListIdentityProviderCfg` | 列出IDP配置 |
| `GET` | `/idp-config/{type}/{name}` | `GetIdentityProviderCfg` | 获取IDP配置 |
| `DELETE` | `/idp-config/{type}/{name}` | `DeleteIdentityProviderCfg` | 删除IDP配置 |

### 2.10 IAM — LDAP 专用

| 方法 | 路径 | Go 处理器 | 说明 |
|------|------|-----------|------|
| `PUT` | `/idp/ldap/add-service-account` | `AddServiceAccountLDAP` | LDAP服务账户 |
| `GET` | `/idp/ldap/list-access-keys` | `ListAccessKeysLDAP` | LDAP访问密钥 |
| `GET` | `/idp/ldap/list-access-keys-bulk` | `ListAccessKeysLDAPBulk` | LDAP访问密钥(批量) |
| `GET` | `/idp/ldap/policy-entities` | `ListLDAPPolicyMappingEntities` | LDAP策略映射 |
| `POST` | `/idp/ldap/policy/{op}` | `AttachDetachPolicyLDAP` | LDAP策略关联 |

### 2.11 IAM — 其他

| 方法 | 路径 | Go 处理器 | 说明 |
|------|------|-----------|------|
| `GET` | `/temporary-account-info` | `TemporaryAccountInfo` | 临时账户信息 |
| `GET` | `/list-access-keys-bulk` | `ListAccessKeysBulk` | 访问密钥(批量) |
| `GET` | `/info-access-key` | `InfoAccessKey` | 访问密钥详情 |
| `GET` | `/export-iam` | `ExportIAM` | 导出IAM (ZIP) |
| `PUT` | `/import-iam` | `ImportIAM` | 导入IAM |
| `PUT` | `/import-iam-v2` | `ImportIAMV2` | 导入IAM V2 |
| `POST` | `/revoke-tokens/{provider}` | `RevokeTokens` | 撤销STS令牌 |

### 2.12 配置 (KV)

| 方法 | 路径 | Go 处理器 | 说明 |
|------|------|-----------|------|
| `GET` | `/get-config-kv?key={k}` | `GetConfigKVHandler` | 获取配置 |
| `PUT` | `/set-config-kv` | `SetConfigKVHandler` | 设置配置 |
| `DELETE` | `/del-config-kv` | `DelConfigKVHandler` | 删除配置 |
| `GET` | `/help-config-kv` | `HelpConfigKVHandler` | 配置帮助 |
| `GET` | `/list-config-history-kv` | `ListConfigHistoryKVHandler` | 配置历史 |
| `PUT` | `/restore-config-history-kv` | `RestoreConfigHistoryKVHandler` | 恢复配置 |
| `DELETE` | `/clear-config-history-kv` | `ClearConfigHistoryKVHandler` | 清空历史 |
| `GET` | `/config` | `GetConfigHandler` | 获取全量配置 |
| `PUT` | `/config` | `SetConfigHandler` | 设置全量配置 |

### 2.13 Bucket 管理 (Admin 级)

| 方法 | 路径 | Go 处理器 | 说明 |
|------|------|-----------|------|
| `GET` | `/get-bucket-quota?bucket={b}` | `GetBucketQuotaConfigHandler` | 获取配额 |
| `PUT` | `/set-bucket-quota?bucket={b}` | `PutBucketQuotaConfigHandler` | 设置配额 |
| `GET` | `/export-bucket-metadata` | `ExportBucketMetadataHandler` | 导出Bucket元数据 |
| `PUT` | `/import-bucket-metadata` | `ImportBucketMetadataHandler` | 导入Bucket元数据 |

### 2.14 复制目标与站点复制

| 方法 | 路径 | Go 处理器 | 说明 |
|------|------|-----------|------|
| `GET` | `/list-remote-targets` | `ListRemoteTargetsHandler` | 列远程目标 |
| `PUT` | `/set-remote-target` | `SetRemoteTargetHandler` | 设置远程目标 |
| `DELETE` | `/remove-remote-target` | `RemoveRemoteTargetHandler` | 删除远程目标 |
| `POST` | `/replication/diff` | `ReplicationDiffHandler` | 复制差异 |
| `GET` | `/replication/mrf` | `ReplicationMRFHandler` | MRF状态 |
| `PUT` | `/site-replication/add` | `SiteReplicationAdd` | 添加站点复制 |
| `PUT` | `/site-replication/remove` | `SiteReplicationRemove` | 移除站点 |
| `GET` | `/site-replication/info` | `SiteReplicationInfo` | 站点复制信息 |
| `GET` | `/site-replication/status` | `SiteReplicationStatus` | 站点复制状态 |
| `PUT` | `/site-replication/edit` | `SiteReplicationEdit` | 编辑站点复制 |

### 2.15 批处理任务

| 方法 | 路径 | Go 处理器 | 说明 |
|------|------|-----------|------|
| `POST` | `/start-job` | `StartBatchJob` | 启动作业 |
| `GET` | `/list-jobs` | `ListBatchJobs` | 列出作业 |
| `GET` | `/status-job` | `BatchJobStatus` | 作业状态 |
| `GET` | `/describe-job` | `DescribeBatchJob` | 作业详情 |
| `DELETE` | `/cancel-job` | `CancelBatchJob` | 取消作业 |

### 2.16 分层存储

| 方法 | 路径 | Go 处理器 | 说明 |
|------|------|-----------|------|
| `PUT` | `/tier` | `AddTierHandler` | 添加分层 |
| `POST` | `/tier/{tier}` | `EditTierHandler` | 编辑分层 |
| `GET` | `/tier` | `ListTierHandler` | 列出分层 |
| `DELETE` | `/tier/{tier}` | `RemoveTierHandler` | 删除分层 |
| `GET` | `/tier/{tier}` | `VerifyTierHandler` | 验证分层 |
| `GET` | `/tier-stats` | `TierStatsHandler` | 分层统计 |

### 2.17 KMS (Admin 级)

| 方法 | 路径 | Go 处理器 | 说明 |
|------|------|-----------|------|
| `POST` | `/kms/status` | `KMSStatusHandler` | KMS状态 |
| `POST` | `/kms/key/create?key-id={id}` | `KMSCreateKeyHandler` | 创建密钥 |
| `GET` | `/kms/key/status` | `KMSKeyStatusHandler` | 密钥状态 |

### 2.18 性能测试

| 方法 | 路径 | Go 处理器 | 说明 |
|------|------|-----------|------|
| `POST` | `/speedtest` | `ObjectSpeedTestHandler` | 对象速度测试 |
| `POST` | `/speedtest/drive` | `DriveSpeedtestHandler` | 磁盘速度测试 |
| `POST` | `/speedtest/net` | `NetperfHandler` | 网络速度测试 |
| `POST` | `/speedtest/site` | `SitePerfHandler` | 站点性能测试 |

### 2.19 调试/追踪

| 方法 | 路径 | Go 处理器 | 说明 |
|------|------|-----------|------|
| `GET` | `/trace` | `TraceHandler` | HTTP追踪 (SSE) |
| `GET` | `/log` | `ConsoleLogHandler` | 控制台日志流 |
| `POST` | `/profile` | `ProfileHandler` | CPU/Mem分析 |
| `GET` | `/top/locks` | `TopLocksHandler` | 锁诊断 |
| `POST` | `/force-unlock` | `ForceUnlockHandler` | 强制解锁 |

> **Rust 实现注解**: Admin API 集中在 `s3::handlers::admin` 模块。按 Phase 划分：
> - **Phase 2**: 存储信息 (`/storageinfo`, `/datausageinfo`)、修复 (`/heal/`)、池管理 (`/pools/`)、调试追踪 (`/trace`, `/log`, `/profile`)、速度测试 (`/speedtest`)
> - **Phase 3**: 全部 IAM 端点 (用户/组/策略/服务账户/IDP/LDAP)、配置 KV、Bucket 管理 (配额)、KMS Admin
> - **Phase 4**: 复制目标与站点复制、批处理任务、分层存储、再平衡
>
> Admin API 路由前缀使用常量 `ADMIN_API_PREFIX = "/minio/admin/v3"`，Rust handler 示例:
> ```rust
> // s3/src/handlers/admin.rs
> pub async fn storage_info(
>     State(state): State<AppState>,
>     // Admin 认证中间件确保已通过 SigV4 验证 + admin 权限检查
> ) -> Result<Json<StorageInfo>, S3Error> {
>     let info = state.object.storage_info().await?;
>     Ok(Json(info))
> }
> ```

---

## 3. STS API

基础路径为 `/`，使用 `application/x-www-form-urlencoded` 内容类型。参考原版 `cmd/sts-handlers.go:152`。

### 3.1 认证端点

| 方法 | 条件 | Go 处理器 | 认证方式 |
|------|------|-----------|---------|
| `POST` | `?Action=AssumeRole` + SigV4 | `AssumeRole` | AWS SigV4 |
| `POST` | `?Action=AssumeRoleWithWebIdentity&WebIdentityToken=` | `AssumeRoleWithWebIdentity` | OIDC JWT |
| `POST` | `?Action=AssumeRoleWithClientGrants&Token=` | `AssumeRoleWithClientGrants` | OAuth2 JWT |
| `POST` | `?Action=AssumeRoleWithLDAPIdentity&LDAPUsername=&LDAPPassword=` | `AssumeRoleWithLDAPIdentity` | LDAP Bind |
| `POST` | `?Action=AssumeRoleWithCertificate` | `AssumeRoleWithCertificate` | X.509 mTLS |
| `POST` | `?Action=AssumeRoleWithCustomToken` | `AssumeRoleWithCustomToken` | 身份插件 |

### 3.2 公共参数

所有 STS 请求共享：
- `Version`: `2011-06-15`
- `DurationSeconds`: 凭证有效期 (默认1小时，受 `MINIO_STS_DURATION` 限制)
- `Policy`: 可选的 Base64 编码会话策略 (进一步限定权限)

### 3.3 响应格式

```xml
<AssumeRoleWithWebIdentityResponse>
  <AssumeRoleWithWebIdentityResult>
    <Credentials>
      <AccessKeyId>...</AccessKeyId>
      <SecretAccessKey>...</SecretAccessKey>
      <SessionToken>...</SessionToken>
      <Expiration>ISO8601</Expiration>
    </Credentials>
    <SubjectFromWebIdentityToken>...</SubjectFromWebIdentityToken>
    <Audience>...</Audience>
    <Provider>...</Provider>
  </AssumeRoleWithWebIdentityResult>
  <ResponseMetadata>
    <RequestId>...</RequestId>
  </ResponseMetadata>
</AssumeRoleWithWebIdentityResponse>
```

### 3.4 STS 错误码

| 错误码 | HTTP | 说明 |
|--------|------|------|
| `AccessDenied` | 403 | 认证失败或权限不足 |
| `MissingParameter` | 400 | 缺少必要参数 |
| `InvalidParameterValue` | 400 | 参数值无效 |
| `ExpiredToken` | 400 | JWT/WebIdentity 令牌过期 |
| `InvalidClientGrantsToken` | 400 | 客户端凭证令牌无效 |
| `MalformedPolicyDocument` | 400 | 会话策略格式错误 |
| `InsecureConnection` | 400 | 需要 TLS |
| `InvalidClientCertificate` | 400 | 客户端证书无效 |
| `TooManyIntermediateCAs` | 400 | 证书链过深 |
| `STSNotInitialized` | 503 | STS 子系统未初始化 |
| `STSIAMNotInitialized` | 503 | IAM 子系统未初始化 |
| `InternalError` | 500 | 内部错误 |

> **Rust 实现注解**: STS 集中在 `s3::handlers::sts` 模块，认证逻辑在 `iam::sts` crate。全部归入 **Phase 3**。JWT 处理使用 `jsonwebtoken` crate，LDAP 绑定使用 `ldap3` crate。STS 凭证的 session token 使用 `uuid` v4 + HMAC 签名，有效期通过 `DurationSeconds` 参数控制。Rust 中 AssumeRoleWithWebIdentity 流程:
>
> ```rust
> // iam/src/sts/web_identity.rs
> pub async fn assume_role_with_web_identity(
>     state: &AppState,
>     token: &str,
>     duration: Option<u32>,
>     policy: Option<&str>,
> ) -> Result<StsCredentials, StsError> {
>     // 1. 验证 JWT (issuer, audience, exp, nbf)
>     let claims = verify_jwt(token, &state.oidc_config)?;
>     // 2. 生成临时凭证
>     let access_key = generate_access_key();
>     let secret_key = generate_secret_key();
>     let session_token = sign_session_token(&access_key, &state.root_token);
>     let expiration = Utc::now() + Duration::seconds(duration.unwrap_or(3600) as i64);
>     // 3. 持久化到 IAM store
>     state.iam.add_sts_credential(access_key, ...).await?;
>     Ok(StsCredentials { access_key, secret_key, session_token, expiration })
> }
> ```

---

## 4. KMS API

前缀：`/minio/kms/v1/`，注册参考原版 `cmd/kms-router.go:38`。

| 方法 | 路径 | Go 处理器 | 说明 |
|------|------|-----------|------|
| `GET` | `/status` | `KMSStatusHandler` | KMS状态 |
| `GET` | `/metrics` | `KMSMetricsHandler` | KMS指标 |
| `GET` | `/apis` | `KMSAPIsHandler` | 列出KMS API |
| `GET` | `/version` | `KMSVersionHandler` | KMS版本 |
| `POST` | `/key/create?key-id={id}` | `KMSCreateKeyHandler` | 创建密钥 |
| `GET` | `/key/list?pattern={p}` | `KMSListKeysHandler` | 列出密钥 |
| `GET` | `/key/status` | `KMSKeyStatusHandler` | 密钥状态 |

> **Rust 实现注解**: KMS API 路由注册在 `s3::router::kms`，后端由 `iam::kms` crate 实现。归入 **Phase 3**。KMS 密钥操作在 Rust 中使用 `aes-gcm` 或 `chacha20-poly1305` 实现 envelope encryption，密钥包装使用 HKDF。

---

## 5. Health API

前缀：`/minio/health/`，注册参考原版 `cmd/healthcheck-router.go:36`。

| 方法 | 路径 | Go 处理器 | 用途 |
|------|------|-----------|------|
| `GET/HEAD` | `/live` | `LivenessCheckHandler` | 存活探针 (进程运行) |
| `GET/HEAD` | `/ready` | `ReadinessCheckHandler` | 就绪探针 (可处理请求) |
| `GET/HEAD` | `/cluster` | `ClusterCheckHandler` | 集群健康 (写Quorum) |
| `GET/HEAD` | `/cluster/read` | `ClusterReadCheckHandler` | 集群读健康 (读Quorum) |

> **Rust 实现注解**: Health API **Phase 1 即可实现**。`/live` 返回 200 OK (不检查任何依赖)。`/ready` 检查磁盘初始化状态和配置加载。`/cluster` 和 `/cluster/read` 在 Phase 2 (分布式) 实现，通过检查在线磁盘数是否满足 Quorum。Rust 实现:
>
> ```rust
> // s3/src/handlers/health.rs
> pub async fn liveness() -> StatusCode {
>     StatusCode::OK
> }
>
> pub async fn readiness(State(state): State<AppState>) -> StatusCode {
>     if state.is_initialized() { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE }
> }
> ```

---

## 6. Metrics API

前缀：`/minio/`，注册参考原版 `cmd/metrics-router.go:53`。

| 方法 | 路径 | 说明 |
|------|------|------|
| `ANY` | `/prometheus/metrics` | 旧版 Prometheus V1 |
| `ANY` | `/v2/metrics/cluster` | 集群级指标 V2 |
| `ANY` | `/v2/metrics/node` | 节点级指标 V2 |
| `ANY` | `/v2/metrics/bucket` | Bucket级指标 V2 |
| `ANY` | `/v2/metrics/resource` | 资源指标 V2 |
| `GET` | `/metrics/v3/{path}` | 指标 V3 (按 Collector 路径) |

### V3 收集器路径

| 路径 | 指标组 |
|------|--------|
| `/api/requests` | 请求速率、延迟、错误率、流量 |
| `/bucket/api` | 每Bucket API统计 |
| `/bucket/replication` | 每Bucket复制指标 |
| `/system/` | 网络、磁盘、内存、CPU、进程 |
| `/cluster/` | 健康、使用、擦除集、IAM |
| `/ilm` | ILM指标 |
| `/audit` | 审计指标 |
| `/scanner` | 扫描器指标 |

> **Rust 实现注解**: Metrics API 在 Rust 中使用 `prometheus-client` crate 实现。Phase 1 实现 `/prometheus/metrics` 基础版本 (请求数、错误数、延迟)。V2/V3 指标归入 Phase 2-4。Rust 端将指标注册为 `Registry` 中的 Counter/Histogram/Gauge，通过 axum middleware 自动记录请求指标:
>
> ```rust
> // s3/src/middleware/metrics.rs
> pub async fn metrics_middleware(
>     req: Request,
>     next: Next,
> ) -> Response {
>     let start = Instant::now();
>     let method = req.method().to_string();
>     let uri = req.uri().path().to_string();
>     let response = next.run(req).await;
>     let latency = start.elapsed();
>     HTTP_REQUESTS_TOTAL.with_label(&[&method, &uri]).inc();
>     HTTP_REQUEST_DURATION.with_label(&[&method, &uri]).observe(latency);
>     response
> }
> ```

---

## 7. 认证体系

参考原版 `cmd/auth-handler.go`。

### 7.1 认证类型检测

```go
func getRequestAuthType(r *http.Request) AuthType
```

| AuthType | 检测条件 |
|----------|---------|
| `authTypeAnonymous` | 无 Authorization header |
| `authTypeSigned` | `AWS4-HMAC-SHA256` Authorization |
| `authTypeSignedV2` | `AWS` Authorization (非V4) |
| `authTypePresigned` | `X-Amz-Credential` Query参数 |
| `authTypePresignedV2` | `AWSAccessKeyId` Query参数 |
| `authTypePostPolicy` | POST `multipart/form-data` |
| `authTypeStreamingSigned` | `STREAMING-AWS4-HMAC-SHA256-PAYLOAD` |
| `authTypeStreamingSignedTrailer` | `STREAMING-AWS4-HMAC-SHA256-PAYLOAD-TRAILER` |
| `authTypeStreamingUnsignedTrailer` | `UNSIGNED-PAYLOAD-TRAILER` |
| `authTypeJWT` | `Bearer` JWT Authorization |
| `authTypeSTS` | `?Action=` Query参数 |

> **Rust 实现注解**: 认证类型检测在 Rust 中通过解析 `Authorization` header 和 query 参数实现，封装为 `base::auth::AuthType` 枚举。Phase 1 实现 `authTypeAnonymous` (跳过认证)，Phase 3 实现完整的 SigV4/SigV2/JWT 检测:
>
> ```rust
> // base/src/auth.rs
> pub enum AuthType {
>     Anonymous,
>     SignedV4,        // AWS4-HMAC-SHA256
>     SignedV2,        // AWS (legacy)
>     PresignedV4,     // X-Amz-Credential in query
>     PresignedV2,     // AWSAccessKeyId in query
>     PostPolicy,      // multipart/form-data
>     StreamingSigned, // STREAMING-AWS4-HMAC-SHA256-PAYLOAD
>     StreamingSignedTrailer,
>     StreamingUnsignedTrailer,
>     Jwt,             // Bearer token
>     Sts,             // ?Action= in query
> }
>
> impl AuthType {
>     pub fn detect(headers: &HeaderMap, query: &HashMap<String, String>) -> Self { ... }
> }
> ```

### 7.2 请求认证流程

```
1. getRequestAuthType() → 识别认证类型
2. 检查 Date Header (SigV2/V4 不能偏差超过 globalMaxSkewTime)
3. authenticateRequest():
   - V4: reqSignatureV4Verify() → 验证签名+Content-SHA256
   - V2: isReqAuthenticatedV2() → 验证签名
   - JWT: getClaimsFromToken() → 验证并解析Claims
4. authorizeRequest():
   - 匿名: 检查Bucket级别策略
   - 已认证: globalIAMSys.IsAllowed() → 策略评估
```

> **Rust 实现注解**: 认证流程在 Rust 中实现为 axum middleware。Phase 1 实现匿名模式 (跳过认证)，Phase 3 实现完整认证链:
>
> ```rust
> // s3/src/middleware/auth.rs
> pub async fn auth_middleware(
>     State(state): State<AppState>,
>     req: Request,
>     next: Next,
> ) -> Result<Response, S3Error> {
>     let auth_type = AuthType::detect(req.headers(), &extract_query(&req));
>     let creds = match auth_type {
>         AuthType::Anonymous => None,
>         AuthType::SignedV4 => verify_sig_v4(&req, &state.iam).await?,
>         AuthType::SignedV2 => verify_sig_v2(&req, &state.iam).await?,
>         AuthType::Jwt => verify_jwt_token(&req, &state.iam).await?,
>         // ... other variants
>     };
>     // 将认证结果注入扩展
>     req.extensions_mut().insert(AuthInfo { creds, auth_type });
>     Ok(next.run(req).await)
> }
> ```

### 7.3 SigV4 签名验证

1. 提取 Authorization header: `AWS4-HMAC-SHA256 Credential=.../.../.../s3/aws4_request, SignedHeaders=..., Signature=...`
2. 重新计算签名:
   ```
   HMAC-SHA256(HMAC-SHA256(HMAC-SHA256(HMAC-SHA256(HMAC-SHA256("AWS4"+secretKey, date), region), "s3"), "aws4_request"), stringToSign)
   ```
3. 对比提供的签名与计算签名
4. 验证 Content-SHA256 (如果存在且不是流式上传)

> **Rust 实现注解**: SigV4 签名验证在 `iam::sigv4` crate 中实现，使用 `hmac` + `sha2` crate。核心 HMAC 链:
>
> ```rust
> // iam/src/sigv4.rs
> use hmac::{Hmac, Mac};
> use sha2::Sha256;
>
> type HmacSha256 = Hmac<Sha256>;
>
> pub fn verify_signature(secret_key: &str, date: &str, region: &str, service: &str, string_to_sign: &str, signature: &str) -> bool {
>     let k_date = hmac(b"AWS4" + secret_key.as_bytes(), date);
>     let k_region = hmac(&k_date, region);
>     let k_service = hmac(&k_region, service);       // "s3" or "sts"
>     let k_signing = hmac(&k_service, b"aws4_request");
>     let calculated = hex::encode(hmac(&k_signing, string_to_sign.as_bytes()));
>     // ... timing-safe comparison
>     calculated == signature
> }
>
> fn hmac(key: &[u8], data: &str) -> Vec<u8> {
>     let mut mac = HmacSha256::new_from_slice(key).expect("HMAC key");
>     mac.update(data.as_bytes());
>     mac.finalize().into_bytes().to_vec()
> }
> ```

### 7.4 Admin API 认证

- 仅支持 SigV4，不支持 Presigned/JWT/Anonymous
- 原版 `checkAdminRequestAuth()`: 验证签名 + 检查 IAM 策略中特定 admin 操作权限

> **Rust 实现注解**: Admin API 认证在 `s3::middleware::admin_auth` 中实现。Phase 2 先跳过认证 (仅允许本地 root)，Phase 3 实现完整 SigV4 验证 + IAM 策略检查。

---

## 8. 错误码枚举

参考原版 `cmd/api-errors.go`。错误码以 `S3ErrorCode` 枚举定义，HTTP 状态码映射通过 `impl S3ErrorCode { fn http_status(&self) -> StatusCode }` 实现。

### 通用错误

| 错误码 | HTTP | 说明 |
|--------|------|------|
| `AccessDenied` | 403 | 访问被拒绝 |
| `BadDigest` | 400 | Content-MD5 不匹配 |
| `EntityTooSmall` | 400 | 上传分片过小 |
| `EntityTooLarge` | 400 | 上传分片过大 |
| `InternalError` | 500 | 内部错误 |
| `InvalidAccessKeyId` | 403 | 无效的 AccessKey |
| `AccessKeyDisabled` | 403 | AccessKey 已禁用 |
| `InvalidBucketName` | 400 | 无效的 Bucket 名称 |
| `InvalidRange` | 416 | 无效的 Range 请求 |
| `MalformedXML` | 400 | XML 格式错误 |
| `MissingContentLength` | 411 | 缺少 Content-Length |
| `NoSuchBucket` | 404 | Bucket 不存在 |
| `NoSuchKey` | 404 | 对象不存在 |
| `NoSuchUpload` | 404 | Multipart Upload 不存在 |
| `NoSuchVersion` | 404 | 对象版本不存在 |
| `NotImplemented` | 501 | 功能未实现 |
| `PreconditionFailed` | 412 | 前提条件失败 |
| `RequestTimeTooSkewed` | 403 | 请求时间偏差过大 |
| `SignatureDoesNotMatch` | 403 | 签名不匹配 |
| `MethodNotAllowed` | 405 | 方法不允许 |
| `BucketAlreadyExists` | 409 | Bucket 已存在 |
| `BucketNotEmpty` | 409 | Bucket 非空 |
| `SlowDownRead` | 503 | 读限速 |
| `SlowDownWrite` | 503 | 写限速 |
| `TooManyRequests` | 429 | 请求过多 |
| `StorageFull` | 507 | 存储满 |

### 加密错误

| 错误码 | HTTP | 说明 |
|--------|------|------|
| `InvalidEncryptionMethod` | 400 | 无效的加密方法 |
| `KMSNotConfigured` | 400 | KMS 未配置 |
| `KMSKeyNotFound` | 400 | KMS 密钥未找到 |
| `InsecureSSECustomerRequest` | 400 | 非 TLS 的 SSE-C 请求 |
| `ObjectTampered` | 400 | 对象被篡改 |

### 复制错误

| 错误码 | HTTP | 说明 |
|--------|------|------|
| `ReplicationConfigurationNotFound` | 404 | 复制配置未找到 |
| `RemoteDestinationNotFound` | 404 | 远程目标未找到 |
| `RemoteTargetNotVersioned` | 400 | 远程目标未开启版本控制 |
| `ReplicationNeedsVersioning` | 400 | 源Bucket需要版本控制 |

### Admin 错误

| 错误码 | HTTP | 说明 |
|--------|------|------|
| `AdminNoSuchUser` | 404 | 用户不存在 |
| `AdminNoSuchGroup` | 404 | 组不存在 |
| `AdminNoSuchPolicy` | 404 | 策略不存在 |
| `AdminConfigNoQuorum` | 503 | 配置变更无 Quorum |
| `AdminBucketQuotaExceeded` | 403 | Bucket 配额超限 |

> **Rust 实现注解**: 错误码在 `base::errors` 模块中定义为枚举，每个变体携带 HTTP 状态码和 S3 错误码字符串。序列化为 S3 标准 XML 错误响应:
>
> ```rust
> // base/src/errors.rs
> use axum::response::{IntoResponse, Response};
> use axum::http::StatusCode;
>
> #[derive(Debug, thiserror::Error)]
> pub enum S3Error {
>     #[error("Access Denied")]
>     AccessDenied,
>     #[error("The specified bucket does not exist")]
>     NoSuchBucket(String),
>     #[error("The specified key does not exist")]
>     NoSuchKey(String),
>     // ...
> }
>
> impl S3Error {
>     pub fn code(&self) -> &'static str {
>         match self {
>             S3Error::AccessDenied => "AccessDenied",
>             S3Error::NoSuchBucket(_) => "NoSuchBucket",
>             S3Error::NoSuchKey(_) => "NoSuchKey",
>             // ...
>         }
>     }
>
>     pub fn http_status(&self) -> StatusCode {
>         match self {
>             S3Error::AccessDenied => StatusCode::FORBIDDEN,
>             S3Error::NoSuchBucket(_) => StatusCode::NOT_FOUND,
>             S3Error::NoSuchKey(_) => StatusCode::NOT_FOUND,
>             // ...
>         }
>     }
> }
>
> impl IntoResponse for S3Error {
>     fn into_response(self) -> Response {
>         let body = format!(
>             r#"<?xml version="1.0" encoding="UTF-8"?>
> <Error><Code>{}</Code><Message>{}</Message></Error>"#,
>             self.code(), self
>         );
>         (self.http_status(), [(CONTENT_TYPE, "application/xml")], body).into_response()
>     }
> }
> ```

---

## 9. Rust 实现状态总览

| 功能组 | Phase | 说明 |
|--------|-------|------|
| **S3 Object 核心** (HEAD/GET/PUT/DELETE) | **P1** | `s3::handlers::object` + `object::erasure_objects` |
| **S3 Bucket 核心** (PUT/HEAD/DELETE/List) | **P1** | `s3::handlers::bucket` |
| **ListObjectsV2** / **ListBuckets** | **P1** | Phase 1 列出能力 |
| **Health API** (/live, /ready) | **P1** | `s3::handlers::health` |
| **CopyObject** / **DeleteMultipleObjects** | **P1** 延伸 | 依赖读锁和写锁 |
| **Dummy API** (ACL/CORS/Website 占位) | **P1** | 返回固定响应 |
| **Multipart Upload** (全部) | **P2** | `s3::handlers::multipart` |
| **Object Tagging/Retention/LegalHold** | **P2** | 元数据扩展 |
| **Bucket Policy/Lifecycle/Encryption** | **P2** | Bucket 元数据系统 |
| **Bucket Versioning/ObjectLock** | **P2** | `base::types::VersionEntry` |
| **Bucket Notification/Tagging/Replication** | **P2** | 高级 Bucket 元数据 |
| **ListObjectsVersions** / **ListMultipartUploads** | **P2** | 版本和 multipart 列出 |
| **Admin: 存储信息/修复/池管理/追踪** | **P2** | `s3::handlers::admin` 子集 |
| **Admin: 速度测试** | **P2** | 性能诊断 |
| **Metrics V1** | **P2** | 基础 Prometheus 指标 |
| **SigV4 认证中间件** | **P3** | `s3::middleware::auth` |
| **STS API** (AssumeRole 系列) | **P3** | `iam::sts` |
| **Admin: 全部 IAM 端点** | **P3** | 用户/组/策略/服务账户/IDP |
| **Admin: 配置 KV** | **P3** | 配置管理 |
| **Admin: KMS 管理** | **P3** | KMS 状态/密钥 |
| **KMS API** (/minio/kms/v1/) | **P3** | KMS 操作 |
| **Metrics V2/V3** | **P3** | 详细指标收集 |
| **Health cluster** (/cluster) | **P3** | 分布式健康检查 |
| **Admin: 站点复制、批处理、分层** | **P4** | 高级管理 |
| **Admin: 再平衡** | **P4** | 池再平衡 |
| **S3 Select / Lambda** | **P4** | 计算下推 |
| **Admin: 复制目标管理** | **P4** | 远程度复制配置 |

---

> 跨文档参考: 极端 Case 处理见 `./EDGE_CASES.md` | 架构与分层设计见 `./ARCHITECTURE.md` | 分阶段计划见 `./PLAN.md`
