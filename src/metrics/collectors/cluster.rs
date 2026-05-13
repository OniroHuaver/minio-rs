//! Cluster-level metrics: health, object usage, erasure-set status.

use std::sync::Arc;

use crate::metrics::registry::MetricsGroup;
use crate::metrics::types::{MetricInfo, MetricType};
use crate::storage::DiskInfo;

pub fn health_group(disk_infos: &[DiskInfo], total_disks: usize) -> MetricsGroup {
    let infos = vec![
        MetricInfo {
            name: "cluster_health_status".into(),
            help: "Cluster health status (1 = online)".into(),
            metric_type: MetricType::Gauge,
            labels: vec![],
        },
        MetricInfo {
            name: "cluster_disk_online".into(),
            help: "Number of online disks".into(),
            metric_type: MetricType::Gauge,
            labels: vec![],
        },
        MetricInfo {
            name: "cluster_disk_total".into(),
            help: "Total number of configured disks".into(),
            metric_type: MetricType::Gauge,
            labels: vec![],
        },
    ];

    let group = MetricsGroup::new("/cluster/health", infos);

    let online_count = disk_infos.len();

    let status = prometheus::Gauge::with_opts(
        prometheus::Opts::new("cluster_health_status", "Cluster health status (1 = online)"),
    )
    .unwrap();
    status.set(if online_count > 0 { 1.0 } else { 0.0 });

    let disk_online = prometheus::Gauge::with_opts(
        prometheus::Opts::new("cluster_disk_online", "Number of online disks"),
    )
    .unwrap();
    disk_online.set(online_count as f64);

    let disk_total = prometheus::Gauge::with_opts(
        prometheus::Opts::new("cluster_disk_total", "Total number of configured disks"),
    )
    .unwrap();
    disk_total.set(total_disks as f64);

    group.register(Box::new(status));
    group.register(Box::new(disk_online));
    group.register(Box::new(disk_total));
    group
}

pub fn usage_group(_object_api: Arc<dyn crate::object::ObjectAPI>) -> MetricsGroup {
    let infos = vec![
        MetricInfo {
            name: "cluster_usage_objects_total".into(),
            help: "Total number of objects in the cluster".into(),
            metric_type: MetricType::Gauge,
            labels: vec![],
        },
        MetricInfo {
            name: "cluster_usage_total_bytes".into(),
            help: "Total bytes used by objects".into(),
            metric_type: MetricType::Gauge,
            labels: vec![],
        },
    ];

    let group = MetricsGroup::new("/cluster/usage/objects", infos);

    let objects = prometheus::Gauge::with_opts(
        prometheus::Opts::new(
            "cluster_usage_objects_total",
            "Total number of objects in the cluster",
        ),
    )
    .unwrap();
    let bytes = prometheus::Gauge::with_opts(
        prometheus::Opts::new(
            "cluster_usage_total_bytes",
            "Total bytes used by objects",
        ),
    )
    .unwrap();

    objects.set(0.0);
    bytes.set(0.0);

    group.register(Box::new(objects));
    group.register(Box::new(bytes));
    group
}

pub fn erasure_set_group(disk_infos: &[DiskInfo], total_disks: usize) -> MetricsGroup {
    let infos = vec![
        MetricInfo {
            name: "cluster_erasure_set_online".into(),
            help: "Erasure set online status (1 = healthy)".into(),
            metric_type: MetricType::Gauge,
            labels: vec![],
        },
        MetricInfo {
            name: "cluster_erasure_set_drives".into(),
            help: "Number of drives in the erasure set".into(),
            metric_type: MetricType::Gauge,
            labels: vec![],
        },
    ];

    let group = MetricsGroup::new("/cluster/erasure-set", infos);

    let online = prometheus::Gauge::with_opts(
        prometheus::Opts::new(
            "cluster_erasure_set_online",
            "Erasure set online status (1 = healthy)",
        ),
    )
    .unwrap();
    online.set(if disk_infos.len() == total_disks && !disk_infos.is_empty() {
        1.0
    } else {
        0.0
    });

    let drives = prometheus::Gauge::with_opts(
        prometheus::Opts::new(
            "cluster_erasure_set_drives",
            "Number of drives in the erasure set",
        ),
    )
    .unwrap();
    drives.set(total_disks as f64);

    group.register(Box::new(online));
    group.register(Box::new(drives));
    group
}
