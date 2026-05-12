//! iam: Identity and Access Management subsystem
//!
//! Phase 3 implementation, currently a placeholder.

/// User type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserType {
    /// Internal user (created via Admin API)
    Regular,
    /// STS temporary user
    STS,
    /// Service account (inherits parent policy)
    ServiceAccount,
}

/// IAM user
#[derive(Debug, Clone)]
pub struct IAMUser {
    pub access_key: String,
    pub secret_key: String,
    pub user_type: UserType,
    pub parent_user: Option<String>,
    pub policy: Option<String>,
    pub groups: Vec<String>,
}

#[cfg(test)]
mod tests {
    //! Unit tests (currently a placeholder)
    //! Integration tests live in ../tests/
}
