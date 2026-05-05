//! grid: Distributed RPC communication layer
//!
//! Phase 2 implementation, currently a placeholder.

/// Grid RPC node information
#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub id: String,
    pub endpoint: String,
    pub online: bool,
}
