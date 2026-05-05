# S3 HTTP API 层设计文档

> 本文档覆盖 Phase 1 的 S3 HTTP API 层架构、路由、中间件、Handler 骨架、请求解析、响应构建、错误映射及关键设计决策。

---

## 1. 架构概览

```
client (AWS SDK / curl / mc)
        │
        ▼
   axum Router (s3 crate)
        │
        ├── Middleware Chain
        │     ├── Trace (请求日志)
        │     ├── CORS (Phase 1 全放通)
        │     └── Auth (Phase 1 占位, Phase 3 实现)
        │
        ├── S3 Handler
        │     ├── 解析 HTTP 请求 → S3 Args
        │     ├── 调用 ObjectAPI
        │     └── 构建 S3 XML 响应
        │
        ▼
   ObjectAPI (ErasureObjects)
        │
        ▼
   storage → erasure → base
```

s3 crate 是整个系统的 HTTP 入口，负责：
1. 将 HTTP 请求解析为 S3 语义参数
2. 调用 `ObjectAPI` trait 执行操作
3. 将结果序列化为 S3 XML 响应格式

---

## 2. 路由设计

使用 axum `Router`，path-style (`/{bucket}/{key}`)：

```rust
let app = Router::new()
    // Service: ListBuckets
    .route("/", get(list_buckets_handler))

    // Bucket operations
    .route("/{bucket}", get(list_objects_v2_handler))
    .route("/{bucket}", put(create_bucket_handler))
    .route("/{bucket}", head(bucket_exists_handler))
    .route("/{bucket}", delete(delete_bucket_handler))

    // Object operations
    .route("/{bucket}/{*key}", put(put_object_handler))
    .route("/{bucket}/{*key}", get(get_object_handler))
    .route("/{bucket}/{*key}", head(head_object_handler))
    .route("/{bucket}/{*key}", delete(delete_object_handler))
    .with_state(app_state);
```

### 路由表

| 方法 | 路径 | Query | Handler | S3 操作 |
|------|------|-------|---------|---------|
| GET | `/` | — | `list_buckets_handler` | ListBuckets |
| PUT | `/{bucket}` | — | `create_bucket_handler` | CreateBucket |
| HEAD | `/{bucket}` | — | `bucket_exists_handler` | HeadBucket |
| DELETE | `/{bucket}` | — | `delete_bucket_handler` | DeleteBucket |
| GET | `/{bucket}` | `list-type=2` | `list_objects_v2_handler` | ListObjectsV2 |
| PUT | `/{bucket}/{*key}` | — | `put_object_handler` | PutObject |
| GET | `/{bucket}/{*key}` | — | `get_object_handler` | GetObject |
| HEAD | `/{bucket}/{*key}` | — | `head_object_handler` | HeadObject |
| DELETE | `/{bucket}/{*key}` | — | `delete_object_handler` | DeleteObject |

**设计决策**：
- 使用 `{*key}`（axum wildcard catch-all）捕获 `/bucket/` 后的完整对象路径
- 同一路径 `/{bucket}` 通过 `method` + `query` 区分
- Phase 1 只实现 path-style，virtual-hosted style 在 Phase 2 添加

---

## 3. AppState 设计

```rust
pub struct AppState {
    pub object_api: Arc<dyn ObjectAPI>,
    pub instance_id: String,
    pub region: String,
}
```

- `object_api` 为 `Arc<dyn ObjectAPI>`，方便未来切换实现
- `instance_id` 用于 `x-amz-id-2` header
- `region` 默认 `"us-east-1"`

---

## 4. 请求解析

### Metadata 提取规则

| HTTP Header | S3 含义 | 目标 |
|-------------|---------|------|
| `Content-Length` | Body 字节长度 | `content_length` |
| `Content-Type` | MIME 类型 | `content_type`, `system_metadata` |
| `Content-MD5` | Body base64 MD5 | 完整性校验 |
| `x-amz-meta-{key}` | 用户元数据 | `user_metadata` (去掉前缀) |
| `Range` | 字节范围 | `range` |

### ListObjectsV2 Query 参数

| Parameter | 类型 | 默认值 |
|-----------|------|--------|
| `list-type` | `u32` | `2` |
| `prefix` | `String` | `""` |
| `delimiter` | `String` | `""` |
| `max-keys` | `usize` | `1000` |
| `continuation-token` | `Option<String>` | `None` |

---

## 5. XML 响应格式

使用 `quick-xml` (v0.36+) 进行 XML 序列化。

### ListBucketsResult

```xml
<?xml version="1.0" encoding="UTF-8"?>
<ListAllMyBucketsResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Owner>
    <ID>minio</ID>
    <DisplayName>minio</DisplayName>
  </Owner>
  <Buckets>
    <Bucket>
      <Name>my-bucket</Name>
      <CreationDate>2025-01-01T00:00:00.000Z</CreationDate>
    </Bucket>
  </Buckets>
</ListAllMyBucketsResult>
```

### ListObjectsV2Result

```xml
<ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Name>bucket</Name>
  <Prefix></Prefix>
  <KeyCount>1</KeyCount>
  <MaxKeys>1000</MaxKeys>
  <IsTruncated>false</IsTruncated>
  <Contents>
    <Key>object-key</Key>
    <LastModified>2025-01-01T00:00:00.000Z</LastModified>
    <ETag>"d41d8cd98f00b204e9800998ecf8427e"</ETag>
    <Size>1024</Size>
    <StorageClass>STANDARD</StorageClass>
  </Contents>
  <CommonPrefixes>
    <Prefix>photos/</Prefix>
  </CommonPrefixes>
</ListBucketResult>
```

### Error

```xml
<Error>
  <Code>NoSuchKey</Code>
  <Message>The specified key does not exist.</Message>
  <Resource>/bucket/key</Resource>
  <RequestId>REQUEST_ID</RequestId>
</Error>
```

---

## 6. 错误映射

| `MinioError` 变体 | HTTP Status | S3 Code |
|-------------------|-------------|---------|
| `DiskIO` | 500 | `InternalError` |
| `DiskNotFound` | 500 | `InternalError` |
| `CorruptedDisk` | 500 | `InternalError` |
| `XlMetaFormat` | 500 | `InternalError` |
| `MessagePack` | 500 | `InternalError` |
| `EncodeError` | 500 | `InternalError` |
| `DecodeError` | 500 | `InternalError` |
| `InsufficientReadQuorum` | 503 | `ServiceUnavailable` |
| `InsufficientWriteQuorum` | 503 | `ServiceUnavailable` |
| `ObjectNotFound` | 404 | `NoSuchKey` |
| `BucketNotFound` | 404 | `NoSuchBucket` |
| `ObjectAlreadyExists` | 409 | `BucketAlreadyExists` |
| `ChecksumMismatch` | 400 | `BadDigest` |
| `InvalidSignature` | 403 | `SignatureDoesNotMatch` |
| `AccessDenied` | 403 | `AccessDenied` |
| `Internal` | 500 | `InternalError` |

---

## 7. Handler 骨架

### PutObject

```rust
async fn put_object_handler(
    State(state): State<Arc<AppState>>,
    Path(params): Path<HashMap<String, String>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // 1. Extract bucket, object key from path
    // 2. Extract Content-Length, check max size (5 GiB)
    // 3. Extract metadata from headers (sys_meta, user_meta)
    // 4. Content-MD5 verification if present
    // 5. Call state.object_api.put_object(bucket, object, data, metadata)
    // 6. Return ETag, version_id, request_id headers
}
```

### GetObject

```rust
async fn get_object_handler(
    State(state): State<Arc<AppState>>,
    Path(params): Path<HashMap<String, String>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // 1. Parse Range header if present
    // 2. Call get_object or get_object_range
    // 3. Return data + metadata headers (Content-Type, ETag, Last-Modified, x-amz-meta-*)
    // 4. Status: 200 (full) or 206 (partial)
}
```

### DeleteObject

```rust
async fn delete_object_handler(
    State(state): State<Arc<AppState>>,
    Path(params): Path<HashMap<String, String>>,
) -> impl IntoResponse {
    // 1. Call state.object_api.delete_object(bucket, object)
    // 2. Return 204 No Content + x-amz-delete-marker: true
}
```

### ListObjectsV2

```rust
async fn list_objects_v2_handler(
    State(state): State<Arc<AppState>>,
    Path(bucket): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    // 1. Parse query params: prefix, delimiter, max-keys
    // 2. Call state.object_api.list_objects(bucket, prefix, delimiter, max_keys)
    // 3. Build ListBucketResult XML response
}
```

---

## 8. 中间件链

```rust
use tower::ServiceBuilder;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

let middleware = ServiceBuilder::new()
    .layer(TraceLayer::new_for_http())  // 请求追踪
    .layer(CorsLayer::permissive());    // CORS 全放通
```

Phase 3 添加 `AuthLayer` 进行 AWS SigV4 签名验证。

---

## 9. Cargo.toml 依赖

需要新增的依赖：
```toml
quick-xml = { version = "0.36", features = ["serialize"] }
md-5 = { workspace = true }
hex = { workspace = true }
base64 = { workspace = true }
chrono = { workspace = true }
```

---

## 10. 模块结构

```
crates/s3/src/
├── lib.rs              # crate 根, pub mod 声明
├── state.rs            # AppState
├── router.rs           # axum Router 构建
├── handlers/
│   ├── mod.rs
│   ├── service.rs      # ListBuckets
│   ├── bucket.rs       # CreateBucket, DeleteBucket, HeadBucket
│   ├── object.rs       # PutObject, GetObject, HeadObject, DeleteObject
│   └── list.rs         # ListObjectsV2
├── request.rs          # S3Request 解析, extract_metadata, parse_range
├── response.rs         # XML 响应结构体, error_response
├── error.rs            # S3ErrorCode 定义 + to_s3_error_code 映射
└── xml.rs              # quick-xml Serialize 结构体
```

---

## 11. 设计决策总结

| 决策 | 选择 | 理由 |
|------|------|------|
| XML库 | `quick-xml` | 性能好、serde支持完整、社区活跃 |
| 路由风格 | path-style | axum 原生支持 `{*key}` catch-all |
| 状态注入 | `Arc<dyn ObjectAPI>` | 便于未来切换实现、trait object分派 |
| 错误响应 | S3 XML Error | 与AWS SDK兼容、统一错误格式 |
| 认证 | Phase 1 跳过 | Phase 3 实现 SigV4 |

### Phase 1 不实现的功能

- AWS Signature V4（Phase 3）
- Multipart Upload（Phase 2）
- CopyObject（Phase 2）
- Virtual-hosted style（Phase 2）
- SSE-C/SSE-S3 加密（Phase 3）
- Versioning HTTP 接口（Phase 2）
- 事件通知（Phase 3）
