//! server: MinIO Rust 二进制入口 (库形式)
//!
//! 对应 Go: cmd/ 下的全部功能
//!
//! 本 crate 同时提供 [[bin]] (main.rs) 和 [lib] (lib.rs) 目标，
//! 以便集成测试 (tests/) 可以引用库中的公开 API。
//!
//! Phase 1: 仅定义模块骨架，实际功能逐步实现。

// ============================================================================
// 模块声明 — 每个模块对应 Go cmd/ 下的一个功能域
// ============================================================================

/// 端点 (Endpoint) 类型与端点解析
///
/// 对应 Go: cmd/endpoint.go (+ endpoint_test.go, endpoint_contrib_test.go, endpoint-ellipses_test.go)
pub mod endpoint {
    // TODO: Phase 2 - 实现 Endpoint, Endpoints, 省略号展开
}

/// 存储池布局
///
/// 对应 Go: cmd/storage-*.go, cmd/erasure-*.go
pub mod layout {
    // TODO: Phase 2 - 存储池、Set 划分、EC 布局
}

/// 网络工具
///
/// 对应 Go: cmd/net.go (+ net_test.go)
pub mod net {
    // TODO: Phase 2 - IP 排序、Host:Port 解析、本地地址判断
}

/// 通用工具函数
///
/// 对应 Go: cmd/utils.go (+ utils_test.go)
pub mod utils {
    // TODO: Phase 2 - 对象大小检查、路径解析、LCP、ETag 等
}

/// 服务器启动流程与配置
///
/// 对应 Go: cmd/server-main.go, cmd/server-startup-msg.go
pub mod server {
    // TODO: Phase 2 - Server 启动、global config、启动消息
}

/// 管理员处理
///
/// 对应 Go: cmd/admin-handlers.go
pub mod admin {
    // TODO: Phase 2 - 管理 API 路由和 handler
}

/// 更新检查
///
/// 对应 Go: cmd/update.go, cmd/update-notifier.go
pub mod update {
    // TODO: Phase 2 - 版本更新检查和通知
}

/// 操作系统工具
///
/// 对应 Go: cmd/os-readdir.go, cmd/os-reliable.go
pub mod osutil {
    // TODO: Phase 2 - readDir, mkdirAll, renameAll
}

/// ARN 类型
pub use base::format;

// 版本常量，对应 Go: cmd/version.go
pub const VERSION: &str = "DEVELOPMENT.GOGET";
