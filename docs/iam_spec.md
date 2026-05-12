# MinIO IAM/STS 安全体系规格

> 本文档整合了 MinIO Go 原版中 IAM、STS、KMS、Security、Site-Replication 相关文档，
> 以 Rust 重写（minio-rs）的视角重构为一套统一的实现参考规格。
>
> 原始文档来源：`/Users/wang/Desktop/minio/docs/`
> Rust 实现项目：`/Users/wang/Desktop/minio-rs/`

---

## 1. IAM 用户模型

### 1.1 用户类型

MinIO 定义三种用户类型，由 `AccountType` 枚举区分：

| 类型 | 标识 | 说明 | 凭证来源 |
|------|------|------|----------|
| **regUser** (Regular User) | `Account` | 通过 `mc admin user add` 创建的长久用户 | 静态 AccessKey/SecretKey |
| **stsUser** (STS User) | `STSUser` | 通过 STS API 生成的临时用户 | 临时 AK/SK + SessionToken |
| **svcUser** (Service Account) | `ServiceAccount` | 由 regUser 或 stsUser 创建的长期子账号 | 关联父用户的策略 |

Rust 枚举定义建议：

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccountType {
    /// 普通用户（通过 mc admin user add 创建）
    Regular,
    /// STS 临时凭证用户
    Sts,
    /// 服务账号
    Service,
}

#[derive(Clone, Debug)]
pub struct Account {
    pub access_key: String,
    pub secret_key: EncryptedSecret,    // 加密存储
    pub parent: Option<String>,         // stsUser/svcUser 的父用户
    pub account_type: AccountType,
    pub policies: Vec<String>,          // 关联的策略名称
    pub claims: HashMap<String, String>, // JWT claims（仅 stsUser）
    pub groups: Vec<String>,
    pub session_token: Option<String>,
    pub expiration: Option<DateTime<Utc>>,
    pub status: AccountStatus,          // Active / Suspended
}
```

### 1.2 存储后端

IAM 数据存储有两种后端，Rust 中通过 trait 抽象：

| 后端 | 适用场景 | 关键特征 |
|------|----------|----------|
| **ObjectStore** (内置) | 单机/分布式（默认） | 与数据存储共用 erasure-coded 后端，支持加密 |
| **Etcd** (外部) | 需要在多集群间共享 IAM 状态 | 通过 `MINIO_ETCD_ENDPOINTS` 配置，支持 TLS |

```rust
#[async_trait]
pub trait IamStorage: Send + Sync {
    /// 读取用户信息
    async fn get_user(&self, access_key: &str) -> Result<Option<Account>, IamError>;
    /// 列出所有用户
    async fn list_users(&self) -> Result<Vec<Account>, IamError>;
    /// 保存用户
    async fn put_user(&self, user: &Account) -> Result<(), IamError>;
    /// 删除用户
    async fn delete_user(&self, access_key: &str) -> Result<(), IamError>;
    /// 策略相关
    async fn get_policy(&self, name: &str) -> Result<Option<IamPolicy>, IamError>;
    async fn put_policy(&self, name: &str, policy: &IamPolicy) -> Result<(), IamError>;
    async fn list_policies(&self) -> Result<Vec<(String, IamPolicy)>, IamError>;
    async fn delete_policy(&self, name: &str) -> Result<(), IamError>;
    /// 用户-策略映射
    async fn get_user_policies(&self, access_key: &str) -> Result<Vec<String>, IamError>;
    async fn set_user_policies(&self, access_key: &str, policies: &[String]) -> Result<(), IamError>;
    /// 组-策略映射
    async fn get_group_policies(&self, group: &str) -> Result<Vec<String>, IamError>;
    async fn set_group_policies(&self, group: &str, policies: &[String]) -> Result<(), IamError>;
    /// 组成员
    async fn get_group_members(&self, group: &str) -> Result<Vec<String>, IamError>;
}
```

### 1.3 策略评估链路

策略评估链路遵循 AWS IAM 的"显式 Deny > Allow"模型：

```
请求到达
  │
  ├─ 提取请求上下文 (Action, Resource, User, Groups, Conditions)
  │
  ├─ [Access Management Plugin] 若配置，外发 webhook 决定 Allow/Deny
  │    跳过内置策略评估
  │
  ├─ 收集所有适用的策略：
  │   ├─ 用户直接关联的策略
  │   ├─ 用户所在组关联的策略
  │   ├─ Session Policy（STS 请求中传递的内联策略）
  │   └─ Bucket Policy（桶级策略）
  │
  ├─ 策略评估引擎：
  │   ├─ 任何策略含 Deny → 最终决策 Deny
  │   ├─ 任何策略含 Allow → 最终决策 Allow
  │   └─ 无匹配 → 隐式 Deny
  │
  └─ 返回 Allow / Deny
```

Rust 策略评估引擎设计：

```rust
pub struct PolicyEngine {
    /// IAM 策略存储
    storage: Arc<dyn IamStorage>,
    /// 可选的 Access Management Plugin 客户端
    plugin_client: Option<AccessPluginClient>,
}

#[async_trait]
pub trait PolicyEvaluator: Send + Sync {
    /// 评估请求是否允许
    async fn evaluate(&self, ctx: &RequestContext) -> Result<EvaluationResult, IamError>;
}

pub struct RequestContext {
    pub account: Account,
    pub action: S3Action,          // s3:GetObject, s3:PutObject, ...
    pub bucket: Option<String>,
    pub object: Option<String>,
    pub conditions: ConditionSet,   // IP, TLS, Time, SSE 等条件
    pub session_policy: Option<IamPolicy>,
    pub deny_only: bool,            // 仅评估 Deny（用于某些内部场景）
}

pub enum EvaluationResult {
    Allow,
    Deny,
    /// 无显式策略匹配（隐式 Deny）
    ImplicitDeny,
}
```

### 1.4 策略 JSON 模型

IAM 策略遵循 AWS IAM 策略语法 v2012-10-17：

```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct IamPolicy {
    #[serde(default = "default_version")]
    pub version: String,                          // "2012-10-17"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub statement: Vec<Statement>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Statement {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sid: Option<String>,
    pub effect: Effect,         // Allow / Deny
    pub principal: Option<Principal>,
    pub action: OneOrMany<String>,
    pub resource: Option<OneOrMany<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<ConditionBlock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_action: Option<OneOrMany<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_resource: Option<OneOrMany<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_principal: Option<Principal>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Effect {
    Allow,
    Deny,
}

/// ConditionBlock 实现条件键的匹配逻辑
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ConditionBlock {
    /// StringEquals, StringNotEquals, StringLike, ...
    #[serde(flatten)]
    pub conditions: HashMap<String, HashMap<String, Vec<String>>>,
}
```

### 1.5 Access Management 插件

当配置 `MINIO_POLICY_PLUGIN_URL` 后，所有 API 请求的策略评估委托给外部 webhook。

**配置项：**

| 环境变量 | 说明 |
|----------|------|
| `MINIO_POLICY_PLUGIN_URL` (必填) | 插件 HTTP(S) 端点 |
| `MINIO_POLICY_PLUGIN_AUTH_TOKEN` | 请求中的 Authorization 头部 |
| `MINIO_POLICY_PLUGIN_ENABLE_HTTP2` | 启用 HTTP2（默认关闭） |

**请求-响应协议：**

- MinIO 发送 `POST` 请求，JSON body 包含完整的请求上下文
- 插件返回 `{"result": true}`（允许）或 `{"result": false}`（拒绝）
- 也接受 `{"result": {"allow": true}}` 格式

**请求 body 结构（Rust 模型）：**

```rust
#[derive(Serialize, Deserialize, Debug)]
pub struct PolicyPluginRequest {
    pub input: PluginInput,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PluginInput {
    pub account: String,
    pub groups: Option<Vec<String>>,
    pub action: String,           // e.g. "s3:ListBucket"
    pub bucket: String,
    pub conditions: HashMap<String, Vec<String>>,
    pub owner: bool,
    #[serde(default)]
    pub object: String,
    pub claims: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub deny_only: bool,
}
```

### 1.6 Identity Management 插件

当配置 `MINIO_IDENTITY_PLUGIN_URL` 后，开启 `AssumeRoleWithCustomToken` STS API。

**配置项：**

| 环境变量 | 说明 |
|----------|------|
| `MINIO_IDENTITY_PLUGIN_URL` (必填) | 身份验证 webhook 端点 |
| `MINIO_IDENTITY_PLUGIN_AUTH_TOKEN` | Authorization 头部 |
| `MINIO_IDENTITY_PLUGIN_ROLE_POLICY` (必填) | 策略名称列表（逗号分隔） |
| `MINIO_IDENTITY_PLUGIN_ROLE_ID` | 自定义 Role ARN 标识 |

**请求-响应协议：**

插件收到 `POST` 请求，query param 携带 `token`。

成功响应 (`200`)：

```json
{
    "user": "<identifier>",
    "maxValiditySeconds": <900-31536000>,
    "claims": { "key1": "value1" }
}
```

失败响应 (`403`)：

```json
{
    "reason": "<error message>"
}
```

---

## 2. STS 安全令牌服务

### 2.1 六个认证入口

MinIO 实现了 6 个 STS 端点，对应不同的认证方式。Rust 枚举统一入口：

```rust
pub enum StsAction {
    /// AssumeRole - 现有 MinIO 用户获取临时凭证
    AssumeRole(AssumeRoleRequest),
    /// AssumeRoleWithWebIdentity - OIDC 身份提供商
    AssumeRoleWithWebIdentity(WebIdentityRequest),
    /// AssumeRoleWithClientGrants - OAuth2 Client Credentials grant
    AssumeRoleWithClientGrants(ClientGrantsRequest),
    /// AssumeRoleWithLDAPIdentity - LDAP/AD 认证
    AssumeRoleWithLDAPIdentity(LdapIdentityRequest),
    /// AssumeRoleWithCertificate - X.509 客户端证书
    AssumeRoleWithCertificate(CertificateRequest),
    /// AssumeRoleWithCustomToken - Identity Management Plugin
    AssumeRoleWithCustomToken(CustomTokenRequest),
}
```

#### 2.1.1 AssumeRole

- **用途**：现有 MinIO 用户（regUser）为自身获取临时凭证
- **认证**：使用 AWS SigV4 签名，凭 AccessKey/SecretKey
- **特点**：
  - 解决 multipart upload 需要使用预签名 URL 的问题
  - 临时凭证的策略继承自原用户策略 + 可选的内联 session policy（交集）
  - 不要求 `--role-arn` 和 `--role-session-name`（可填任意值）
- **请求参数**：

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `Version` | String | 是 | 固定 `2011-06-15` |
| `DurationSeconds` | Integer | 否 | 900~31536000 (秒)，默认 3600 |
| `Policy` | String | 否 | 内联 session policy JSON（最长 2048 字符） |

#### 2.1.2 AssumeRoleWithWebIdentity

- **用途**：通过 OIDC/OpenID 身份提供商（如 Keycloak, Google, Dex, Casdoor）获取临时凭证
- **认证**：JWT `id_token`，由提供商的 JWKS 端点验证签名
- **CVE-2026-33322 安全增强**：`RELEASE.2026-03-25T00-00-00Z` 起，拒绝 HMAC 签名令牌（`HS256/384/512`），仅接受 RSA PKCS#1 v1.5 和 ECDSA 系列（`RS256/384/512`、`ES256/384/512`）。`PS256` 和 `EdDSA` 暂不支持。
- **策略指定方式**（二选一，不可同时使用）：
  1. **Role Policy（推荐）**：`MINIO_IDENTITY_OPENID_ROLE_POLICY`，所有同提供商用户获得相同策略，需传 `RoleArn`
  2. **JWT Claims**：从 `id_token` 的 claim（默认 `policy`）中提取策略名，**不**传 `RoleArn`

**OpenID 配置项：**

| 环境变量 | 说明 |
|----------|------|
| `MINIO_IDENTITY_OPENID_CONFIG_URL` (必填) | OIDC Discovery URL |
| `MINIO_IDENTITY_OPENID_CLIENT_ID` (必填) | OAuth2 客户端 ID |
| `MINIO_IDENTITY_OPENID_CLIENT_SECRET` | 客户端密钥（用于 OAuth2 交互，不用于 JWT 验证） |
| `MINIO_IDENTITY_OPENID_ROLE_POLICY` | 角色策略名称（逗号分隔） |
| `MINIO_IDENTITY_OPENID_CLAIM_NAME` | 自定义 JWT claim 名（默认 `policy`） |
| `MINIO_IDENTITY_OPENID_SCOPES` | OpenID scopes |
| `MINIO_IDENTITY_OPENID_VENDOR` | 提供商类型（如 `keycloak`） |
| `MINIO_IDENTITY_OPENID_CLAIM_USERINFO` | 是否从 UserInfo 端点获取 claims |
| `MINIO_IDENTITY_OPENID_REDIRECT_URI_DYNAMIC` | 基于 Host header 的动态 redirect（适用于负载均衡场景） |
| `MINIO_IDENTITY_OPENID_KEYCLOAK_REALM` | Keycloak realm 名称 |
| `MINIO_IDENTITY_OPENID_KEYCLOAK_ADMIN_URL` | Keycloak Admin REST API 端点 |

**支持同时配置多个 OpenID Provider**：通过名称后缀（如 `_APP2`），每个可配置独立的 role policy。任意数量 role-policy provider + 至多 1 个 claim-based provider。

**请求参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `WebIdentityToken` | String | 是 | JWT id_token（4~2048 字符） |
| `WebIdentityAccessToken` | String | 否 | MinIO 扩展，从 UserInfo 端点获取 claims |
| `RoleArn` | String | 否 | 当使用 role policy 时必传 |
| `DurationSeconds` | Integer | 否 | 900~31536000，默认 3600 |
| `Policy` | String | 否 | 内联 session policy |

#### 2.1.3 AssumeRoleWithClientGrants

- **用途**：OAuth2 Client Credentials 模式，用于机器到机器认证
- **认证**：JWT `access_token`（与 WebIdentity 用 `id_token` 不同）
- **安全要求**：与 WebIdentity 相同，废弃 HMAC 签名，仅接受 RSA/ECDSA JWKS 验证
- **适用场景**：Keycloak、Okta 等支持 client_credentials grant 的 IDP
- **请求参数**：与 WebIdentity 类似，但使用 `Token` 参数传递 access_token

Rust 凭证验证流程（所有 JWT-based STS 共用）：

```rust
/// JWT 验证器配置
pub struct JwtVerifierConfig {
    pub jwks_url: Url,
    pub client_id: String,
    pub expected_audience: Option<String>,
    pub allowed_algorithms: HashSet<JwsAlgorithm>,
}

/// 支持的 JWS 算法
pub enum JwsAlgorithm {
    Rs256, Rs384, Rs512,
    Es256, Es384, Es512,
    // HS256/384/512 被 CVE-2026-33322 废弃
}

pub async fn verify_jwt_token(
    token: &str,
    config: &JwtVerifierConfig,
) -> Result<VerifiedClaims, StsError> {
    // 1. 从 JWKS 端点获取公钥（缓存 + 定期刷新）
    // 2. 验证 JWT 签名
    // 3. 验证签发算法在白名单内（拒绝 HMAC）
    // 4. 验证 iss, aud, exp, nbf
    // 5. 提取策略 claims
}
```

#### 2.1.4 AssumeRoleWithLDAPIdentity

- **用途**：LDAP/Active Directory 用户使用用户名密码获取临时凭证
- **认证流程**：

```
用户提供 LDAP 用户名+密码
  → MinIO 用 LDAP 只读服务账号执行 DN 查找
  → 使用该 DN 验证用户密码
  → 查询用户所属组
  → 查找用户和组的关联策略
  → 生成临时凭证（凭证中加密保存组列表）
```

- **系统运维人员注意事项**：
  - 启用 LDAP 后，**不再支持**内部长久用户（仅保留 root user）
  - `mc admin user` 和 `mc admin group` 命令受限，只支持 `info` 子命令
  - 用户和组在 AD/LDAP 中定义，MinIO 只做策略映射

- **自动 LDAP 同步**：MinIO 定期轮询 LDAP 服务器，清理已删除用户的凭证，更新组员变更

- **CVE-2026-33419 安全增强**：未知用户和密码错误返回**相同**的错误响应 `400 InvalidParameterValue`，防止用户名枚举

**LDAP 配置项：**

| 环境变量 | 说明 |
|----------|------|
| `MINIO_IDENTITY_LDAP_SERVER_ADDR` (必填) | LDAP 服务器地址（端口默认 636） |
| `MINIO_IDENTITY_LDAP_SRV_RECORD_NAME` | DNS SRV 记录名（`ldap`/`ldaps`/`on`） |
| `MINIO_IDENTITY_LDAP_LOOKUP_BIND_DN` | 只读服务账号 DN |
| `MINIO_IDENTITY_LDAP_LOOKUP_BIND_PASSWORD` | 服务账号密码（空=匿名绑定） |
| `MINIO_IDENTITY_LDAP_USER_DN_SEARCH_BASE_DN` | 用户搜索基准 DN（`;` 分隔） |
| `MINIO_IDENTITY_LDAP_USER_DN_SEARCH_FILTER` | 用户 DN 搜索过滤器（`%s`=用户名） |
| `MINIO_IDENTITY_LDAP_USER_DN_ATTRIBUTES` | 附加用户属性（`,` 分隔） |
| `MINIO_IDENTITY_LDAP_GROUP_SEARCH_FILTER` | 组搜索过滤器（`%s`=用户名, `%d`=DN） |
| `MINIO_IDENTITY_LDAP_GROUP_SEARCH_BASE_DN` | 组搜索基准 DN |
| `MINIO_IDENTITY_LDAP_TLS_SKIP_VERIFY` | 跳过 TLS 验证（默认 `off`） |
| `MINIO_IDENTITY_LDAP_SERVER_INSECURE` | 允许明文连接（默认 `off`） |
| `MINIO_IDENTITY_LDAP_SERVER_STARTTLS` | 使用 StartTLS（默认 `off`） |
| `MINIO_IDENTITY_LDAP_STS_TRUSTED_PROXIES` | 可信反向代理列表（用于速率限制的客户端 IP 提取） |

#### 2.1.5 AssumeRoleWithCertificate

- **用途**：通过 X.509 客户端证书进行认证
- **优势**：不依赖外部组件（如 OIDC 或 LDAP 服务器），运维复杂度低
- **工作机制**：
  1. 客户端通过 mTLS 发送 `POST` 请求到 STS 端点
  2. MinIO 验证客户端证书有效性
  3. MinIO 取证书 `Subject: CN` 字段作为策略名查找 IAM 策略
  4. 返回关联该策略的临时凭证
- **证书要求**：必须包含 `Extended Key Usage: TLS Web Client Authentication`
- **有效期**：临时凭证有效期 ≤ 客户端证书有效期
- **配置**：默认关闭，需 `MINIO_IDENTITY_TLS_ENABLE=on`

#### 2.1.6 AssumeRoleWithCustomToken

- **用途**：配合 Identity Management Plugin 使用，验证不透明令牌
- **特性**：令牌对 MinIO 不透明，直接转发给插件验证
- **不支持 Console UI 登录**：仅用于机器认证
- **请求参数**：

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `Token` | String | 是 | 不透明令牌 |
| `RoleArn` | String | 是 | 必须匹配插件生成的 Role ARN |

### 2.2 JWT Claims 结构

STS 返回的 `SessionToken` 是一个 JWT，包含以下 claims：

```json
{
  "accessKey": "...",
  "exp": 3600000000000,
  "policy": "readwrite",
  "parent": "<parent_user>",
  "sub": "<parent_user>",
  "groups": ["group1", "group2"],
  "roleArn": "arn:minio:iam:::role/...",
  "iat": 1541807471,
  "iss": "https://localhost:9443/oauth2/token",
  "jti": "a0b27629-ee1a-43bf-8739-f3374a4cdbc0",
  "aud": "PoEgXP6uVO45IsENRngDXj5Au5Ya",
  "azp": "PoEgXP6uVO45IsENRngDXj5Au5Ya"
}
```

Rust SessionToken 解码模型：

```rust
#[derive(Deserialize, Debug)]
pub struct SessionClaims {
    pub access_key: String,
    pub exp: u64,
    #[serde(default)]
    pub policy: Option<String>,
    #[serde(default)]
    pub parent: Option<String>,
    pub sub: String,
    #[serde(default)]
    pub groups: Vec<String>,
    #[serde(default)]
    pub role_arn: Option<String>,
    pub iat: u64,
    pub iss: Option<String>,
    pub jti: Option<String>,
    pub aud: Option<String>,
    pub azp: Option<String>,
}
```

### 2.3 速率限制规则

#### LDAP STS 限流

| 项 | 值 |
|----|-----|
| 限制维度 | 源 IP 和 标准化用户名，独立跟踪 |
| 突发容量 | 每个桶 10 次尝试 |
| 补充速率 | 每 6 秒 1 个 token（约 10 次/分钟/桶） |
| 保留期 | LDAP bind 期间持有 |
| 空闲清理 | 15 分钟无活动后清除 |
| 限流响应 | `429 ThrottlingException` + `Retry-After: 6` |
| 范围 | 每节点内存内（非集群级别） |
| 可配置性 | 暂不可配置 |

**安全设计**：
- 默认 Source IP 取 socket peer 地址（**不信任** `X-Forwarded-For` 等头）
- 成功登录和 LDAP 基础设施失败**退还** token
- 仅真正认证失败才消耗 token
- 可通过 `MINIO_IDENTITY_LDAP_STS_TRUSTED_PROXIES` 配置可信代理

**LDAP 错误响应码**：

| 条件 | HTTP 状态 | STS 错误码 |
|------|-----------|------------|
| 未知用户或密码错误 | `400` | `InvalidParameterValue`（防止用户名枚举） |
| LDAP 后端/网络故障 | `500` | `InternalError` |
| 限流耗尽 | `429` | `ThrottlingException` |

### 2.4 STS 通用临时凭证结构

所有 STS API 返回相同的临时凭证结构（XML 序列化）：

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct StsCredentials {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: String,
    pub expiration: DateTime<Utc>,
}
```

凭证生命周期：
- 最短 900 秒（15 分钟）
- 最长 365 天
- 默认 3600 秒（1 小时）
- Certificate STS：不得超过客户端证书有效期

---

## 3. KMS 密钥管理

### 3.1 KMS 集成模式

MinIO 支持三种 KMS 运行模式：

| 模式 | 配置方式 | 适用场景 |
|------|----------|----------|
| **无 KMS** | 不配置 | 开发/测试，IAM 数据明文存储 |
| **静态密钥** | `MINIO_KMS_SECRET_KEY` | 小规模部署，单密钥加密 IAM 数据 |
| **KMS + KES** | `MINIO_KMS_KES_*` 系列变量 | 生产环境，支持密钥轮换、安全擦除 |

KES (Key Encryption Service) 是 MinIO 的 KMS 代理层，统一对接各类 KMS 后端：

| KMS 后端 | 说明 |
|----------|------|
| Hashicorp Vault | 本地 KMS，**推荐** |
| AWS KMS + Secrets Manager | 云端 KMS |
| Gemalto KeySecure / Thales CipherTrust | 本地 KMS |
| GCP Secret Manager | 云端 KMS |
| Filesystem Keystore | 仅开发/测试 |

**KES 配置项：**

| 环境变量 | 说明 |
|----------|------|
| `MINIO_KMS_KES_ENDPOINT` | KES 服务器端点 |
| `MINIO_KMS_KES_KEY_FILE` | KES 客户端私钥文件路径 |
| `MINIO_KMS_KES_KEY_PASSWORD` | 加密私钥的密码 |
| `MINIO_KMS_KES_CERT_FILE` | KES 客户端证书文件路径 |
| `MINIO_KMS_KES_KEY_NAME` | 默认主密钥名称 |
| `MINIO_KMS_KES_CAPATH` | KES 服务器 CA 证书 |

### 3.2 IAM 数据加密

**架构变化**：原版 MinIO 使用 root 凭证 + Argon2 内存硬函数加密 IAM 数据，已统一迁移到 KMS 加密。

**密钥层级**：

```
       CMK (Customer Master Key)  — KMS 管理
           │
    ┌──────┴──────┐
    │   EK (External Key)         — KMS.GenerateKey() 生成
    │   (明文 + 密文两份返回)
    └──────┬──────┘
           │
    ┌──────┴──────┐
    │   KEK (Key Encryption Key)   — PRF(EK, IV, context)
    │   HMAC-SHA-256 派生
    └──────┬──────┘
           │
    ┌──────┴──────┐
    │   OEK (Object Encryption Key) — 随机生成，唯一每对象
    │   AEAD 加密存储为对象元数据（密文）
    └──────┬──────┘
           │
    ┌──────┴──────┐
    │   内容加密     — AEAD (AES-256-GCM 或 ChaCha20-Poly1305)
    │   分块 65536 字节，每块唯一 nonce
    │   multipart: PRF(OEK, part_id) 每部分独立密钥
    └───────────────┘
```

**加密原语**：

| 组件 | 算法 |
|------|------|
| PRF (密钥派生) | HMAC-SHA-256 |
| AEAD (数据加密) | AES-256-GCM（x86-64 + AES-NI）/ ChaCha20-Poly1305（其他） |
| 密钥长度 | 256 位 |
| Nonce | 96 位，每对象/每个 multipart 部分随机生成 |
| 分块大小 | 65536 字节（支持最大 256 TiB 明文） |

**优势**：
- 集中密钥管理（S3 对象 + IAM 数据统一 KMS）
- 启动更快（不再使用 Argon2）
- root 凭证可独立变更
- 支持安全擦除（封禁 KMS 主密钥即可锁定数据）

### 3.3 KMS 密钥轮换

**SSE-S3 密钥轮换**：

```
对象元数据中的旧 EK_密文
  → KMS.DecryptKey(CMK_ID, EK_encrypted) → EK_plain
  → 解密 OEK (KEK = PRF(EK_plain, IV, context))
  → KMS.GenerateKey(当前 CMK_ID) → 新 EK_plain + EK_encrypted
  → 重新生成 KEK' → 重加密 OEK
  → 保存新 EK_encrypted 到对象元数据
```

**SSE-C 密钥轮换**：
- S3 COPY 操作（源=目标）
- header 同时携带旧和新客户密钥

---

## 4. 安全策略与最佳实践

### 4.1 安全公告摘要 (pgsty/minio fork)

| CVE | 影响区域 | 漏洞类型 | 修复版本 |
|-----|----------|----------|----------|
| CVE-2026-33322 | OIDC STS (WebIdentity, ClientGrants) | JWT 算法混淆，拒绝 HMAC 签名 | `RELEASE.2026-03-25T00-00-00Z` |
| CVE-2026-33419 | LDAP STS 认证 | 用户名枚举 + 缺少限流 | `RELEASE.2026-03-21T00-00-00Z` |
| CVE-2026-34204 | 复制元数据处理 | 不可信 header 注入 | `56fa63bfd` |
| CVE-2026-39414 | S3 Select 超大记录处理 | 未检查缓冲区增长 | `3252d5b7f` |
| CVE-2026-40027 | Unsigned-trailer PUT | 查询字符串认证绕过 | `f444b6f37` |
| CVE-2026-40028 | Snowball auto-extract | 提取前未验证认证 | `efb6e5b00` |
| CVE-2026-34986 | 依赖安全 | go-jose 升级到 v4.1.4 | `68e0ba997` |
| CVE-2026-39883 | 依赖安全 | OpenTelemetry 依赖更新 | `1869bd30b` |
| Go 1.26.2 | 工具链安全 | Go 上游安全修复 | `db4c0fd5e` |

**关键操作指引**：

- `MINIO_IDENTITY_OPENID_CLIENT_SECRET` 仅用于 OAuth2 客户端交互，**不**用于 JWT 验证密钥
- 所有 JWT-based STS 必须使用 JWKS 发布的 RSA/ECDSA 公钥验证
- LDAP STS 部署中，未知用户和密码错误返回**相同**错误码
- LDAP 部署**必须**使用 TLS/StartTLS，禁止明文传输密码
- 在代理后部署时配置 `MINIO_IDENTITY_LDAP_STS_TRUSTED_PROXIES` 以确保源 IP 限流准确性

### 4.2 TLS/mTLS 要求

- 所有 S3 API 端点推荐使用 TLS
- SSE-C 在无 TLS 时明文传输加密密钥，**必须**搭配 TLS/HTTPS
- mTLS 用于 Certificate-based STS 认证
- 与 LDAP 服务器的连接**强烈建议**使用 TLS（默认端口 636）
- 支持 StartTLS

### 4.3 SSE 策略示例

以下两个策略文件展示了如何使用条件键控制 SSE-KMS 加密要求：

**拒绝非 SSE-KMS 对象**（`deny-non-sse-kms-objects.json`）：

```json
{
   "Version":"2012-10-17",
   "Statement":[{
         "Effect":"Deny",
         "Action":"s3:PutObject",
         "Resource":"arn:aws:s3:::multi-key-poc/*",
         "Condition":{
            "Null":{
               "s3:x-amz-server-side-encryption-aws-kms-key-id":"true"
            }
         }
      }
   ]
}
```

**拒绝无效 SSE-KMS Key ID**（`deny-objects-with-invalid-sse-kms-key-id.json`）：

```json
{
   "Version":"2012-10-17",
   "Statement":[{
         "Effect":"Deny",
         "Action":"s3:PutObject",
         "Resource":"arn:aws:s3:::multi-key-poc/*",
         "Condition":{
            "StringNotEquals":{
               "s3:x-amz-server-side-encryption-aws-kms-key-id":"minio-default-key"
            }
         }
      }
   ]
}
```

---

## 5. 站点复制

### 5.1 复制实体列表

当启用站点复制后，以下变更会在所有 peer 站点间复制：

| 复制实体 | 复制方向 |
|----------|----------|
| Bucket 创建/删除 | 双向 |
| 对象创建/删除 | 双向 |
| IAM 用户、组、策略、映射关系 | 双向 |
| STS 临时凭证 | 双向 |
| 服务账号（非 root 用户所属） | 双向 |
| Bucket Policies | 双向 |
| Bucket Tags | 双向 |
| Bucket Object-Lock 配置（含 retention、legal hold） | 双向 |
| Bucket Encryption 配置 | 双向 |

**不复制**的实体：
- Bucket 通知配置（各站点独立）
- Bucket 生命周期/ILM 配置（各站点独立）

### 5.2 复制钩子机制

**启用前提**：
- 初始时只有**一个**站点有数据
- 所有站点使用**相同**的外部 IDP
- 如果使用 SSE-S3/KMS 加密，所有站点必须连接**同一**中心化 KMS
- 一旦配置成功，**不允许移除**已有站点

**复制流程**：

```
mc admin replicate add minio1 minio2 minio3
  → MinIO 验证各站点连通性
  → 创建站点间复制的服务账号
  → 使用该服务账号凭据签署 STS token（而非 root 凭据）
  → 初始同步（有数据的站点 → 空站点）
  → 后续增量同步（任一站点写入 → 所有其他站点）
```

**STS Token 签署变更说明**：
- 之前：使用 root 凭据签署 STS token
- 现在：使用 site-replicator 服务账号签署
- 升级后原有 STS token 失效，需重新生成
- 移除站点复制配置后 STS token 也将失效

**CVE-2026-34204 修复**：阻止不可信 `X-Minio-Replication-*` header 被注入内部复制元数据导致对象不可读。

Rust 中站点复制的核心抽象：

```rust
/// 站点复制管理器
pub struct SiteReplicator {
    peers: Vec<SitePeer>,
    service_account: ServiceAccount,
    storage: Arc<dyn IamStorage>,
}

pub struct SitePeer {
    pub endpoint: String,
    pub client: Arc<MinioClient>,
}

/// 需要复制的 IAM 变更事件
pub enum IamReplicationEvent {
    UserCreated(Account),
    UserDeleted(String),
    PolicyCreated(String, IamPolicy),
    PolicyDeleted(String),
    PolicyAttached { user: String, policy: String },
    PolicyDetached { user: String, policy: String },
    GroupCreated(String),
    GroupDeleted(String),
    GroupMemberAdded { group: String, user: String },
    GroupMemberRemoved { group: String, user: String },
    StsCredentialCreated(Account),
    ServiceAccountCreated(Account),
    ServiceAccountDeleted(String),
}
```

---

## 6. Rust 实现路线图

### 6.1 Phase 3 IAM Crate 结构建议

```text
minio-rs/
├── src/
│   └── iam/                          # IAM crate
│       ├── mod.rs                    # 公开 exports
│       ├── account.rs                # Account 类型 + AccountType
│       ├── policy/
│       │   ├── mod.rs                # PolicyEngine trait
│       │   ├── model.rs              # IamPolicy, Statement, Condition 模型
│       │   ├── parser.rs             # JSON 反序列化
│       │   ├── evaluator.rs          # 策略评估逻辑（Deny > Allow > ImplicitDeny）
│       │   ├── condition.rs          # Condition 键匹配（StringEquals, IpAddress 等）
│       │   └── action.rs             # S3Action 枚举 + ARN 解析
│       ├── storage/
│       │   ├── mod.rs                # IamStorage trait
│       │   ├── object_store.rs       # ObjectStore 后端实现
│       │   └── etcd.rs               # Etcd 后端实现（可选，gate 或 feature flag）
│       ├── sts/
│       │   ├── mod.rs                # StsAction 枚举 + dispatch
│       │   ├── handler.rs            # 统一的 STS 请求处理器
│       │   ├── assume_role.rs        # AssumeRole 实现
│       │   ├── web_identity.rs       # WebIdentity + ClientGrants 共享
│       │   ├── client_grants.rs      # ClientGrants 实现
│       │   ├── ldap.rs               # LDAP 认证 + 组查询 + 自动同步
│       │   ├── certificate.rs        # X.509 证书认证
│       │   ├── custom_token.rs       # Identity Management Plugin
│       │   └── jwt.rs                # JWT 验证器（JWKS + 算法白名单）
│       ├── plugin/
│       │   ├── mod.rs                # 插件 trait
│       │   ├── access_plugin.rs      # Access Management Plugin 客户端
│       │   └── identity_plugin.rs    # Identity Management Plugin 客户端
│       ├── replicator.rs             # 站点复制事件
│       └── throttler.rs              # LDAP STS 限流令牌桶
```

### 6.2 关键 Trait 定义

```rust
// ========== IAM 存储 ==========
#[async_trait]
pub trait IamStorage: Send + Sync { /* 见 1.2 */ }

// ========== 策略评估 ==========
#[async_trait]
pub trait PolicyEvaluator: Send + Sync {
    async fn evaluate(&self, ctx: &RequestContext) -> Result<EvaluationResult, IamError>;
}

// ========== STS 处理器 ==========
#[async_trait]
pub trait StsHandler: Send + Sync {
    async fn handle(&self, req: StsAction) -> Result<StsCredentials, StsError>;
}

// ========== KMS 接口 ==========
#[async_trait]
pub trait KmsBackend: Send + Sync {
    /// 生成数据密钥（返回明文 + 密文）
    async fn generate_key(&self, key_id: &str) -> Result<(Vec<u8>, Vec<u8>), KmsError>;
    /// 解密加密的数据密钥
    async fn decrypt_key(&self, key_id: &str, cipher_key: &[u8]) -> Result<Vec<u8>, KmsError>;
}

// ========== LDAP 连接器 ==========
#[async_trait]
pub trait LdapConnector: Send + Sync {
    /// 认证用户（验证密码）
    async fn authenticate(&self, dn: &str, password: &str) -> Result<bool, LdapError>;
    /// 查找用户 DN
    async fn lookup_user_dn(&self, username: &str) -> Result<Option<String>, LdapError>;
    /// 查询用户组成员
    async fn lookup_user_groups(&self, user_dn: &str) -> Result<Vec<String>, LdapError>;
    /// 查询用户属性
    async fn lookup_user_attributes(&self, user_dn: &str) -> Result<HashMap<String, String>, LdapError>;
}

// ========== 限流器 ==========
pub trait RateLimiter: Send + Sync {
    fn try_acquire(&self, key: &str) -> Result<(), ThrottleError>;
    fn release(&self, key: &str);
    fn consume(&self, key: &str); // 永久消耗（真正失败时）
}
```

### 6.3 策略评估引擎设计思路

策略评估引擎采用"Effect 短路"模型：

```
evaluate(ctx):
  1. 收集所有适用策略
     ├─ user_policies = get_user_policies(ctx.account.access_key)
     ├─ group_policies = flat_map(get_group_policies, ctx.account.groups)
     ├─ session_policy = ctx.session_policy
     └─ bucket_policies = get_bucket_policies(ctx.bucket)

  2. 构建扁平化 Statement 列表
     └─ 每条 Statement 展开 Action/Resource 通配符

  3. 匹配阶段：
     for stmt in all_statements:
         if stmt.effect == Deny && match(stmt, ctx):
             return Deny      # 立即短路

     for stmt in all_statements:
         if stmt.effect == Allow && match(stmt, ctx):
             return Allow

     return ImplicitDeny      # 无匹配
```

条件键匹配需支持的主要运算符：
- `StringEquals` / `StringNotEquals` / `StringEqualsIgnoreCase`
- `StringLike` / `StringNotLike`（支持 `*` 和 `?` 通配符）
- `NumericEquals` / `NumericNotEquals` / `NumericLessThan` 等
- `DateEquals` / `DateGreaterThan` 等
- `Bool`
- `IpAddress` / `NotIpAddress`（CIDR 匹配）
- `ArnEquals` / `ArnLike`
- `Null`（检查键是否存在）
- `BinaryEquals`

Rust 中条件键匹配可采用策略模式：

```rust
pub enum ConditionOperator {
    StringEquals, StringNotEquals, StringEqualsIgnoreCase,
    StringLike, StringNotLike,
    NumericEquals, NumericNotEquals, NumericLessThan, NumericGreaterThan,
    DateEquals, DateNotEquals, DateLessThan, DateGreaterThan,
    Bool,
    IpAddress, NotIpAddress,
    ArnEquals, ArnLike,
    Null,
    BinaryEquals,
}

pub trait ConditionMatcher {
    fn matches(&self, op: &ConditionOperator, key: &str, values: &[String],
               ctx: &RequestContext) -> Result<bool, IamError>;
}
```

### 6.4 STS JWT Token 生命周期管理

```rust
pub struct StsTokenManager {
    signing_key: HmacKey,       // HMAC-SHA512 签发 session token
    iam_storage: Arc<dyn IamStorage>,
    kms: Option<Arc<dyn KmsBackend>>,
}

impl StsTokenManager {
    /// 创建 STS 用户和凭证
    pub async fn issue_credentials(
        &self,
        parent: &str,
        policy: Option<String>,
        groups: Option<Vec<String>>,
        duration: Duration,
        claims: HashMap<String, String>,
        role_arn: Option<String>,
    ) -> Result<StsCredentials, StsError> {
        // 1. 生成随机 AccessKeyID / SecretAccessKey
        // 2. 签发 SessionToken (JWT, HMAC-SHA512 签名)
        // 3. 保存 STS 用户到 IAM 存储
        // 4. 返回凭证
    }

    /// 验证 SessionToken 并恢复用户上下文
    pub async fn validate_session(
        &self,
        token: &str,
    ) -> Result<Account, StsError> {
        // 1. 解析 JWT
        // 2. 验证签名
        // 3. 验证未过期
        // 4. 从存储加载用户（含策略信息）
    }
}
```

### 6.5 IAM 数据加密集成

IAM 数据存储在上层配合 KMS 实现透明加密：

```rust
pub struct EncryptedIamStorage {
    inner: Box<dyn IamStorage>,
    kms: Arc<dyn KmsBackend>,
    master_key_id: String,
}

// 写入时加密，读取时解密
#[async_trait]
impl IamStorage for EncryptedIamStorage {
    async fn get_user(&self, access_key: &str) -> Result<Option<Account>, IamError> {
        let user = self.inner.get_user(access_key).await?;
        match user {
            Some(mut u) => {
                u.secret_key = self.decrypt(&u.secret_key)?;
                Ok(Some(u))
            }
            None => Ok(None),
        }
    }

    async fn put_user(&self, user: &Account) -> Result<(), IamError> {
        let mut user = user.clone();
        user.secret_key = self.encrypt(&user.secret_key)?;
        self.inner.put_user(&user).await
    }
}
```

---

## 附录

### A. STS API 参数汇总

| STS API | Action 值 | 认证方式 | 主要参数 |
|---------|-----------|----------|----------|
| AssumeRole | `AssumeRole` | SigV4 签名 | DurationSeconds, Policy |
| WebIdentity | `AssumeRoleWithWebIdentity` | JWT id_token + JWKS 验证 | WebIdentityToken, RoleArn, DurationSeconds |
| ClientGrants | `AssumeRoleWithClientGrants` | JWT access_token + JWKS 验证 | Token, DurationSeconds |
| LDAP | `AssumeRoleWithLDAPIdentity` | LDAP 用户名+密码 | LDAPUsername, LDAPPassword, DurationSeconds |
| Certificate | `AssumeRoleWithCertificate` | X.509 mTLS 客户端证书 | DurationSeconds |
| CustomToken | `AssumeRoleWithCustomToken` | Identity Plugin webhook | Token, RoleArn, DurationSeconds |

### B. 原始文档索引

本文档整合了以下原始 MinIO 文档：
- `docs/iam/access-management-plugin.md`
- `docs/iam/identity-management-plugin.md`
- `docs/iam/policies/`（PBAC 策略示例 + 测试脚本）
- `docs/sts/README.md`
- `docs/sts/assume-role.md`
- `docs/sts/web-identity.md`
- `docs/sts/client-grants.md`
- `docs/sts/ldap.md`
- `docs/sts/tls.md`
- `docs/sts/custom-token-identity.md`
- `docs/sts/casdoor.md` / `docs/sts/etcd.md`
- `docs/security/README.md`
- `docs/security/advisories.md`
- `docs/kms/README.md`
- `docs/kms/IAM.md`
- `docs/site-replication/README.md`

### C. 变更跟踪

| 日期 | 变更 | 说明 |
|------|------|------|
| 2026-05-04 | 初始创建 | 整合 17+ 文档为统一 IAM/STS 规格 |
