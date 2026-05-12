//! Collector implementations for specific V3 paths.
//!
//! Each sub-module constructs one or more `MetricsGroup`s with registered
//! Prometheus counters, gauges, and histograms.

pub mod api;
pub mod cluster;
pub mod system;

use std::sync::Arc;

use crate::metrics::http_stats::HttpStats;
use crate::metrics::registry::{MetricsGroup, MetricsRegistry};
use crate::object::ObjectAPI;
use crate::storage::DiskInfo;

/// Result of building all collectors, ready for injection into `AppState`.
pub struct RegistryBundle {
    pub registry: MetricsRegistry,
    pub http_stats: Arc<HttpStats>,
    pub system_collector: Option<Arc<system::SystemCollector>>,
}

/// Build all collectors and return the top-level `MetricsRegistry`,
/// `HttpStats`, and a `SystemCollector` handle.
///
/// Called once at server startup from `server::run`.
pub fn build_registry(
    object_api: Arc<dyn ObjectAPI>,
    disk_infos: &[DiskInfo],
    total_disks: usize,
) -> RegistryBundle {
    let (sys_groups, sys_collector) = system::build_system_groups(disk_infos);

    let http_stats = HttpStats::new();
    let (api_group, http_stats) = api::requests_group(&http_stats);
    let http_stats = Arc::new(http_stats);

    let groups: Vec<Arc<MetricsGroup>> = {
        let mut g = sys_groups;
        g.push(Arc::new(cluster::health_group(disk_infos, total_disks)));
        g.push(Arc::new(cluster::usage_group(object_api)));
        g.push(Arc::new(cluster::erasure_set_group(disk_infos, total_disks)));
        g.push(Arc::new(api_group));
        g
    };

    RegistryBundle {
        registry: MetricsRegistry::new(groups),
        http_stats,
        system_collector: Some(sys_collector),
    }
}
