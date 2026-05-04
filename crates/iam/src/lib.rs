//! iam: IAM 身份与访问管理子系统
//!
//! 对应 Go: cmd/iam.go + cmd/iam-store.go + cmd/sts-handlers.go
//!
//! Phase 3 实现，当前为占位。

/// 用户类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserType {
    /// 内部用户 (Admin API 创建)
    Regular,
    /// STS 临时用户
    STS,
    /// 服务账户 (继承父策略)
    ServiceAccount,
}

/// IAM 用户
#[derive(Debug, Clone)]
pub struct IAMUser {
    pub access_key: String,
    pub secret_key: String,
    pub user_type: UserType,
    pub parent_user: Option<String>,
    pub policy: Option<String>,
    pub groups: Vec<String>,
}
