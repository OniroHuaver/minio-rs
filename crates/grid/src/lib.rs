//! grid: 分布式 RPC 通信层
//!
//! 对应 Go: internal/grid/
//!
//! Phase 2 实现，当前为占位。

/// Grid RPC 节点信息
#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub id: String,
    pub endpoint: String,
    pub online: bool,
}
