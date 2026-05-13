//! Iceberg REST Catalog 压测模块。
//!
//! 子模块:
//! - `catalog`:   Catalog 连接管理 (RestCatalog, CatalogPool)
//! - `tree`:      N叉命名空间树构建
//! - `dataset`:   Dataset 创建/删除
//! - `schema`:    Schema / ViewVersion 构建
//! - `warehouse`: Warehouse 管理 (AIStor Tables 专有)
//! - `retry`:     指数退避重试

pub mod catalog;
pub mod dataset;
pub mod distribution;
pub mod retry;
pub mod schema;
pub mod tree;
pub mod warehouse;

pub use catalog::{CatalogConfig, ExternalCatalogType};
pub use dataset::IsAlreadyExists;
pub use retry::RetryConfig;
