# Console (Web UI) 设计文档

> 对照原 MinIO Console (Go) 架构，规划 minio-rs 的 Web 管理控制台实现。

---

## 1. 原版架构概览

MinIO 的 Web 控制台并非内嵌在 MinIO Server 进程中，而是一个**独立进程**（Console Server），通过浏览器重定向衔接：

```
Browser ──GET /──→ :9000 (MinIO Server)
                     │ guessIsBrowserReq() ?
                     │ 307 Redirect
                     └──→ :13333 (Console Server)
                            │ React SPA (静态文件)
                            │ /api/v1/* (REST API)
                            │ /ws (WebSocket)
                            └──→ MinIO Server (STS 凭证调用 S3 API)
```

关键设计决策：
- **分离部署**：Console 是独立的 Go 二进制，通过 `--console-address` 指定端口
- **SPA 嵌入**：React 前端编译为静态文件，通过 `//go:embed` 嵌入 Console 二进制
- **同源免除 CORS**：浏览器访问 Console 的 IP:Port，API 请求也发到同源，不需要 CORS 预检
- **STS 会话**：用户登录后，Console 获取 STS 临时凭证，后续所有 S3 操作使用 STS 凭证代理

---

## 2. 浏览器重定向机制

### 2.1 触发条件 (MinIO Server 端)

```go
// cmd/generic-handlers.go
func guessIsBrowserReq(r *http.Request) bool {
    aType := getRequestAuthType(r)
    return strings.Contains(r.Header.Get("User-Agent"), "Mozilla") &&
        globalBrowserEnabled && aType == authTypeAnonymous
}
```

三个条件同时满足：
1. `User-Agent` 包含 `Mozilla`（浏览器）
2. `globalBrowserEnabled == true`（未通过 `--no-browser` 禁用）
3. 请求为匿名（无 S3 签名）

### 2.2 重定向目标

只对以下路径触发重定向：`/`（根路径）、`/minio`、`/favicon-*.png`、`/index.html`。其余 S3 API 路径正常处理。

重定向策略：
- 若有 `globalBrowserRedirectURL`（`MINIO_BROWSER_REDIRECT_URL` 环境变量），使用自定义 URL
- 否则取请求 Host，替换端口为 `globalMinioConsolePort`（默认 `13333`），scheme 跟随原请求（HTTP/HTTPS）

### 2.3 防误判

- 健康检查请求（`/minio/health/*`）**不触发**重定向
- 带 S3 签名的请求（`aws-cli`、`mc`、SDK）不触发
- 非 GET/HEAD 请求不触发

### 2.4 Rust 实现要点

在 axum middleware 层实现：

```rust
// 伪代码：browser redirect middleware
async fn browser_redirect_middleware(
    req: Request,
    next: Next,
) -> Response {
    if is_browser(&req) && is_redirectable_path(req.uri().path()) {
        let redirect_url = format!("http://{}:{}/", req_host, console_port);
        return Response::builder()
            .status(StatusCode::TEMPORARY_REDIRECT)
            .header(LOCATION, redirect_url)
            .body(Body::empty())
            .unwrap();
    }
    next.run(req).await
}
```

---

## 3. Console Server 架构

### 3.1 整体结构

```
Console Server (:13333)
│
├── FileServerMiddleware
│   ├── /api/*     → REST API handlers
│   ├── /ws        → WebSocket upgrade
│   └── /*         → SPA 静态文件 (index.html fallback)
│
├── Global Middleware Chain
│   ├── GzipHandler
│   ├── AuditLogMiddleware
│   ├── FileServerMiddleware (路由分发)
│   ├── ContextMiddleware
│   ├── AuthenticationMiddleware (cookie → Bearer token)
│   ├── DebugLogMiddleware
│   └── Secure (CSP/HSTS/Referrer-Policy 安全头)
│
└── API Handlers (go-swagger 生成)
```

### 3.2 认证中间件

`AuthenticationMiddleware` 的核心逻辑：

```
1. 从 Cookie 读取 token
2. AES-GCM 解密 token → TokenClaims { STS_AK, STS_SK, STS_Token, AccountAK }
3. 设置 Authorization: Bearer <decrypted_token> 到请求头
4. 下游 handler 从 Header 中提取 Principal
```

匿名路由（`/api/v1/login`）不走认证中间件。

### 3.3 Session Token 格式

Token 是加密的 JSON payload，密钥派生自 PBKDF2：

```
TokenClaims {
    stsAccessKeyID:     string   // STS 临时 AccessKey
    stsSecretAccessKey: string   // STS 临时 SecretKey
    stsSessionToken:    string   // STS SessionToken
    accountAccessKey:   string   // 登录时用的 Account AK（用于登出）
    hm:                 bool     // 是否隐藏菜单
    ob:                 bool     // 是否仅对象浏览器模式
    customStyleOb:      string   // 自定义样式
}
```

Cookie 属性：
- Name: `token`
- HttpOnly: true（防止 XSS）
- Secure: TLS 环境下为 true
- SameSite: Lax
- MaxAge: 等于 STS 凭证有效期

### 3.4 SPA 静态文件服务

```
请求路径          → 响应
/                 → index.html (SPA 入口)
/static/js/*.js   → 匹配的静态文件
/api/v1/*         → API handler
/ws               → WebSocket
/any-other-path   → 文件存在则返回，否则 fallback 到 index.html (SPA 路由)
```

特殊处理：`handleSPA` 中检测 URL query 参数 `sts`、`sts_a`、`sts_s`，用于 STS token 注入（从外部 SSO 回调携带凭证）。

---

## 4. REST API 完整清单

所有 API 前缀：`/api/v1/`

### 4.1 认证

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/login` | 获取登录策略（form/oauth2）、IDP 列表 |
| POST | `/login` | 表单登录（accessKey + secretKey + sts） |
| POST | `/login/oauth2/auth` | OAuth2 回调（code + state） |
| POST | `/logout` | 登出，销毁会话 Cookie |
| GET | `/session` | 验证会话，返回权限、features、环境常量 |

### 4.2 用户管理

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/users` | 列出所有用户 |
| POST | `/users` | 创建用户 |
| GET | `/user/{name}` | 获取用户详情 |
| PUT | `/user/{name}/groups` | 更新用户组成员 |
| PUT | `/user/{name}/policies` | 设置用户策略 |
| GET | `/user/policy` | 获取当前用户策略 |
| GET | `/user/{name}/service-accounts` | 列出用户的服务账号 |
| POST | `/user/{name}/service-accounts` | 创建服务账号 |
| POST | `/users-groups-bulk` | 批量更新用户组 |

### 4.3 组管理

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/groups` | 列出所有组 |
| POST | `/groups` | 创建组 |
| GET | `/group/{name}` | 获取组详情 |
| PUT | `/group/{name}` | 更新组 |
| DELETE | `/group/{name}` | 删除组 |

### 4.4 策略管理

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/policies` | 列出所有策略 |
| POST | `/policies` | 创建策略 |
| GET | `/policy/{name}` | 获取策略详情 |
| DELETE | `/policy/{name}` | 删除策略 |
| GET | `/policies/{policy}/users` | 列出绑定此策略的用户 |
| GET | `/policies/{policy}/groups` | 列出绑定此策略的组 |
| GET | `/bucket-policy/{bucket}` | 获取 bucket 策略 |
| GET | `/bucket-users/{bucket}` | 列出有 bucket 访问权限的用户 |
| PUT | `/set-policy` | 绑定策略到用户/组 |
| PUT | `/set-policy-multi` | 批量绑定策略 |

### 4.5 Bucket 管理

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/buckets` | 列出全部 bucket |
| POST | `/buckets` | 创建 bucket |
| GET | `/buckets/{name}` | 获取 bucket 详情 |
| DELETE | `/buckets/{name}` | 删除 bucket |
| PUT | `/buckets/{name}/set-policy` | 设置 bucket 访问策略 |
| GET | `/buckets/{name}/quota` | 获取配额 |
| PUT | `/buckets/{bucket_name}/tags` | 设置标签 |
| GET | `/buckets/{bucket_name}/versioning` | 获取版本控制状态 |
| PUT | `/buckets/{bucket_name}/versioning` | 设置/暂停版本控制 |
| GET | `/buckets/{bucket_name}/object-locking` | 获取 object lock 状态 |
| GET/PUT | `/buckets/{bucket_name}/retention` | 获取/设置保留配置 |
| POST | `/buckets/{bucket_name}/encryption/enable` | 启用加密 |
| POST | `/buckets/{bucket_name}/encryption/disable` | 禁用加密 |
| GET | `/buckets/{bucket_name}/encryption/info` | 获取加密配置 |
| GET | `/buckets/{bucket_name}/replication` | 获取复制规则 |
| GET | `/buckets/{bucket_name}/replication/{rule_id}` | 获取单条复制规则 |
| DELETE | `/buckets/{bucket_name}/delete-all-replication-rules` | 删除全部复制规则 |
| GET | `/buckets/{bucket_name}/rewind/{date}` | 时间点回溯列表 |
| GET | `/buckets/max-share-exp` | 获取最大分享链接过期时间 |

### 4.6 Bucket 事件

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/buckets/{bucket_name}/events` | 列出事件通知 |
| POST | `/buckets/{bucket_name}/events` | 创建事件通知 |
| DELETE | `/buckets/{bucket_name}/events/{arn}` | 删除事件通知 |

### 4.7 Bucket 生命周期

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/buckets/{bucket_name}/lifecycle` | 添加生命周期规则 |
| PUT | `/buckets/{bucket_name}/lifecycle/{lifecycle_id}` | 编辑生命周期规则 |
| DELETE | `/buckets/{bucket_name}/lifecycle/{lifecycle_id}` | 删除生命周期规则 |
| POST | `/buckets/multi-lifecycle` | 批量添加生命周期 |

### 4.8 对象操作

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/buckets/{bucket_name}/objects` | 列出对象 |
| POST | `/buckets/{bucket_name}/objects/upload` | 上传对象 |
| GET | `/buckets/{bucket_name}/objects/download` | 下载单个对象 |
| POST | `/buckets/{bucket_name}/objects/download-multiple` | 批量下载 (zip) |
| POST | `/buckets/{bucket_name}/delete-objects` | 批量删除 |
| DELETE | `/buckets/{bucket_name}/objects` | 删除单个对象 |
| PUT | `/buckets/{bucket_name}/objects/legalhold` | 设置 legal hold |
| PUT | `/buckets/{bucket_name}/objects/retention` | 设置保留期 |
| DELETE | `/buckets/{bucket_name}/objects/retention` | 删除保留期 |
| PUT | `/buckets/{bucket_name}/objects/tags` | 设置标签 |
| POST | `/buckets/{bucket_name}/objects/restore` | 恢复对象版本 |
| GET | `/buckets/{bucket_name}/objects/metadata` | 获取对象元数据 |
| POST | `/buckets/{bucket_name}/objects/share` | 生成分享链接 |

### 4.9 服务账号

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/service-accounts` | 列出当前用户的服务账号 |
| POST | `/service-accounts` | 创建服务账号 |
| POST | `/service-account-credentials` | 创建服务账号凭证 |
| DELETE | `/service-accounts/delete-multi` | 批量删除 |
| GET | `/service-accounts/{access_key}` | 获取服务账号详情 |
| DELETE | `/service-accounts/{access_key}` | 删除服务账号 |
| PUT | `/service-accounts/{access_key}` | 更新服务账号 |

### 4.10 配置

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/configs` | 列出配置 |
| GET | `/configs/{name}` | 获取子系统配置 |
| PUT | `/configs/{name}` | 设置子系统配置 |
| POST | `/configs/{name}/reset` | 重置子系统配置 |
| GET | `/configs/export` | 导出全部配置 |
| POST | `/configs/import` | 导入配置 |
| POST | `/admin/notification_endpoints` | 添加通知端点 |

### 4.11 管理

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/service/restart` | 重启服务 |
| POST | `/profiling/start` | 启动性能分析 |
| POST | `/profiling/stop` | 停止并下载 profile |
| GET | `/admin/info` | 获取服务器信息 |
| GET | `/admin/info/widgets/{widgetId}` | 仪表盘 widget 数据 |
| GET | `/admin/arns` | 列出 ARN |
| GET | `/nodes` | 列出节点 |
| GET | `/remote-buckets` | 列出远程 bucket |
| POST | `/remote-buckets` | 添加远程 bucket |
| DELETE | `/remote-buckets/{name}` | 删除远程 bucket |

### 4.12 站点复制

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/admin/site-replication` | 获取站点复制信息 |
| POST | `/admin/site-replication` | 添加站点复制 |
| PUT | `/admin/site-replication` | 编辑站点复制 |
| DELETE | `/admin/site-replication` | 删除站点复制 |
| GET | `/admin/site-replication/status` | 获取复制状态 |

### 4.13 Tier (分层存储)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/admin/tiers` | 列出 tier |
| GET | `/admin/tiers/names` | 列出 tier 名称 |
| POST | `/admin/tiers` | 添加 tier |
| GET | `/admin/tiers/{type}/{name}` | 获取 tier 详情 |
| PUT | `/admin/tiers/{type}/{name}/credentials` | 编辑 tier 凭证 |
| DELETE | `/admin/tiers/{name}/remove` | 删除 tier |

### 4.14 KMS

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/kms/status` | KMS 状态 |
| GET | `/kms/metrics` | KMS 指标 |
| GET | `/kms/apis` | 列出 KMS API |
| GET | `/kms/version` | KMS 版本 |
| GET | `/kms/keys` | 列出密钥 |
| POST | `/kms/keys` | 创建密钥 |
| GET | `/kms/keys/{name}` | 密钥状态 |

### 4.15 IDP

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/idp/{type}` | 列出 IDP 配置 |
| POST | `/idp/{type}` | 创建 IDP 配置 |
| GET | `/idp/{type}/{name}` | 获取 IDP 配置 |
| PUT | `/idp/{type}/{name}` | 更新 IDP 配置 |
| DELETE | `/idp/{type}/{name}` | 删除 IDP 配置 |
| GET | `/ldap-entities` | 列出 LDAP 实体 |

### 4.16 其他

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/logs/search` | 搜索日志 |
| POST | `/admin/inspect` | 下载 inspect 数据 |
| GET | `/releases` | 获取版本列表 |
| GET | `/download-shared-object/{url}` | 公开分享对象下载 |

### 4.17 WebSocket

| 路径 | 说明 |
|------|------|
| `/ws` | 实时数据推送（对象浏览器、监控指标、日志流） |

---

## 5. 登录认证流程

### 5.1 表单登录 (POST /api/v1/login)

```
Browser                    Console Server                  MinIO Server
  │                             │                               │
  │  POST /api/v1/login         │                               │
  │  {accessKey, secretKey}     │                               │
  │────────────────────────────→│                               │
  │                             │  STS AssumeRole (或直接验证)    │
  │                             │──────────────────────────────→│
  │                             │  STS Credentials              │
  │                             │←──────────────────────────────│
  │                             │                               │
  │                             │  加密 TokenClaims → AES-GCM   │
  │                             │  Set-Cookie: token=<enc>      │
  │  {sessionId, permissions}   │                               │
  │←────────────────────────────│                               │
```

### 5.2 OAuth2 登录 (POST /api/v1/login/oauth2/auth)

```
1. 用户点击 "Login with SSO" → GET /api/v1/login 返回 IDP 重定向 URL
2. 浏览器重定向到 IDP (如 Keycloak, Okta)
3. 用户在 IDP 完成认证
4. IDP 回调 /api/v1/login/oauth2/auth?code=xxx&state=yyy
5. Console 用 code 换 token → 验证 ID token → 获取 STS 凭证
6. 创建 session cookie → 重定向回主页
```

### 5.3 会话验证 (GET /api/v1/session)

每次 SPA 初始化时调用，返回：
- `status`: "ok" | "error"
- `permissions`: 用户权限映射 (key → [action1, action2, ...])
- `features`: 功能开关列表（监控、复制、加密等）
- `envConstants`: 环境常量（最大上传大小、超时等）

### 5.4 后续 API 请求

所有 `/api/v1/*` 请求由 Console Server 代理执行：
- Console 从 session token 中提取 STS 凭证
- 使用 STS 凭证调用 MinIO S3 API（作为 S3 客户端）
- 部分 Console API 也调用 MinIO Admin API

---

## 6. Rust 实现方案

### 6.1 架构选择

两种方案：

| 方案 | 描述 | 优点 | 缺点 |
|------|------|------|------|
| A. 独立 Console 进程 | 与原版一致，Console 是独立二进制 | 隔离性好，可独立扩缩 | 需要两个进程 |
| B. 内嵌 Console 路由 | Console API 和 S3 API 在同一个 axum Router | 部署简单，单端口 | 与原版不一致，CORS 复杂 |

**建议**：Phase 3 采用**方案 B（内嵌）**——在 `:9000` 的 axum Router 上同时挂载 S3 路由和 Console 路由（`/api/v1/*`、`/ws`、`/` 静态文件），降低部署复杂度。浏览器直接访问 `:9000` 即进入控制台，无需重定向。

方案 B 下的路由合并：

```rust
let app = Router::new()
    // S3 API (path-style)
    .merge(s3_router())
    // Console API + SPA
    .merge(console_router())
    // 通用中间件
    .layer(TraceLayer)
    .layer(CorsLayer::permissive());
```

路由分发逻辑（替代原版 FileServerMiddleware）：

```rust
async fn console_or_s3_handler(req: Request) -> Response {
    match req.uri().path() {
        p if p.starts_with("/api/v1/") => console_api_handler(req).await,
        p if p.starts_with("/ws")       => ws_handler(req).await,
        p if p == "/" || is_spa_route(p) => spa_file_handler(req).await,
        _ => s3_handler(req).await,  // fallback 到 S3
    }
}
```

### 6.2 技术栈

| 组件 | Go 原版 | Rust 方案 |
|------|---------|-----------|
| HTTP 框架 | go-swagger | `axum` 0.7（复用 S3 层） |
| 前端 | React (TypeScript) | 复用原版 React SPA（从 console release 提取静态文件） |
| 静态文件嵌入 | `//go:embed` | `rust-embed` 或 `include_dir` |
| Session Token | AES-GCM + PBKDF2 | `aes-gcm` + `pbkdf2` crate |
| JWT (可选) | - | `jsonwebtoken`（已引入） |
| WebSocket | gorilla/websocket | `axum` 内置 ws |
| OpenAPI/Swagger | go-swagger 生成 | `utoipa`（代码注解生成） |

### 6.3 分阶段实现

| 阶段 | 内容 | 依赖 |
|------|------|------|
| Phase 3.0 | Console 壳：静态文件服务 + `/api/v1/login` + `/api/v1/session` | IAM/STS 就绪 |
| Phase 3.1 | Bucket CRUD API（`/api/v1/buckets/*`） | Bucket 元数据 |
| Phase 3.2 | Object 浏览/上下传（`/api/v1/buckets/{name}/objects/*`） | 基础对象操作已就绪 |
| Phase 3.3 | User/Group/Policy 管理（IAM 可视化管理） | IAM Store |
| Phase 3.4 | 配置管理（`/api/v1/configs/*`） | 配置子系统 |
| Phase 3.5 | 监控 Dashboard + WebSocket 实时推送 | 指标采集就绪 |
| Phase 4 | 高级功能集成（Site Replication、ILM、KMS、Tier） | 各子系统就绪 |

### 6.4 认证适配

Rust 版的 Console API 使用与 S3 API 相同的认证机制：

```
1. POST /api/v1/login  → 验证 accessKey/secretKey → 签发 JWT (含 STS 信息)
2. Set-Cookie: token=<JWT>（HttpOnly, Secure, SameSite=Lax）
3. 后续 Console API 请求：
   a. 从 Cookie 取 JWT → 验证签名 → 提取 Principal
   b. 使用 Principal 中的 STS 凭证代理 S3 操作
```

简化方案（Phase 3 初期）：不用 STS，直接用 root 凭证操作。

### 6.5 SPA 静态文件来源

三种可选方案：

| 方案 | 描述 | 评价 |
|------|------|------|
| 1. 嵌入上游构建 | 从 MinIO Console Release 下载 `build/` 目录，`include_dir!` 嵌入 | 零维护成本，但版本绑定 |
| 2. 独立前端项目 | 用 Leptos / Dioxus 写 Rust 原生 WASM 前端 | 全栈 Rust，但工作量大 |
| 3. 轻量 HTML | Phase 3 初期先用纯 HTML + 少量 JS 做最小可用控制台 | 快速迭代，后续切换到方案 1 |

**建议**：Phase 3 初期用**方案 3**（最小 HTML 页面），验证 API 逻辑。后续切换到**方案 1**（嵌入上游 React 构建），获得与 MinIO 一致的用户体验。

### 6.6 目录规划

```
src/
├── console/
│   ├── mod.rs              # Console 模块入口
│   ├── router.rs           # axum Router 构造（/api/v1/* + /ws + SPA）
│   ├── auth.rs             # Cookie → JWT 认证中间件
│   ├── login.rs            # 登录/登出/会话 handler
│   ├── buckets.rs          # Bucket CRUD handler
│   ├── objects.rs          # Object 浏览 handler
│   ├── users.rs            # User/Group/Policy handler
│   ├── config.rs           # 配置管理 handler
│   ├── admin.rs            # 管理操作 handler（restart, profile, inspect）
│   └── ws.rs               # WebSocket handler
│
└── s3/                     # 现有 S3 路由（不变）
```

---

## 7. 关键设计决策

| 决策 | 选择 | 理由 |
|------|------|------|
| 部署模式 | 内嵌（Phase 3），可选独立进程（Phase 4） | 降低 Phase 3 复杂度，单端口部署 |
| 前端来源 | Phase 3 最小 HTML → Phase 4 嵌入 React | 先验证后端 API，前端迭代独立 |
| 认证方式 | JWT Cookie (HttpOnly) | 与 S3 SigV4 解耦，SPA 无需管理凭证 |
| S3 代理 | 服务端 STS 凭证代理 | 浏览器不直接持有 STS 凭证 |
| API 风格 | REST (JSON)，与原版 Console 保持一致 | 兼容原版 React SPA |
| WebSocket | axum 内置 ws | 零额外依赖 |

---

## 8. 与原版差异

| 项目 | Go 原版 | Rust 版 (计划) |
|------|---------|----------------|
| 端口 | 独立 `:13333` | Phase 3 共享 `:9000`；Phase 4 可选独立 |
| 重定向 | 307 到 Console 端口 | 无（同端口直接服务） |
| API 文档 | go-swagger (Swagger 2.0) | utoipa (OpenAPI 3.0) |
| Token 格式 | AES-GCM 加密 JSON | JWT (HMAC/RSA 签名) |
| 前端嵌入 | `//go:embed` | `include_dir!` 或 `rust-embed` |
| 配置注入 | `CONSOLE_*` 环境变量 | 启动参数 + 配置文件 |
