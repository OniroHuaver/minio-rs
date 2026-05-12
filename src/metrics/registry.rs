//! Metrics registry — maps V3 collector paths to `prometheus::Registry`
//! instances and provides path-based gathering with optional `?bucket=`
//! filtering.

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use prometheus::proto::MetricFamily;
use prometheus::{Encoder, Registry, TextEncoder};

use crate::metrics::types::MetricInfo;

/// A group of Prometheus metrics registered under a single V3 collector path.
pub struct MetricsGroup {
    pub path: String,
    registry: Registry,
    infos: Vec<MetricInfo>,
}

impl MetricsGroup {
    pub fn new(path: impl Into<String>, infos: Vec<MetricInfo>) -> Self {
        Self {
            path: path.into(),
            registry: Registry::new(),
            infos,
        }
    }

    pub fn register(&self, c: Box<dyn prometheus::core::Collector>) {
        let _ = self.registry.register(c);
    }

    pub fn gather(&self) -> Vec<MetricFamily> {
        self.registry.gather()
    }

    pub fn infos(&self) -> &[MetricInfo] {
        &self.infos
    }
}

/// The top-level V3 metrics registry with path routing and bucket filtering.
pub struct MetricsRegistry {
    groups: Vec<Arc<MetricsGroup>>,
}

impl MetricsRegistry {
    pub fn new(groups: Vec<Arc<MetricsGroup>>) -> Self {
        Self { groups }
    }

    /// Gather metric families for the given V3 path, optionally filtering
    /// by `bucket` label values (comma-separated).
    pub fn gather(
        &self,
        path: &str,
        bucket_filter: Option<&HashSet<String>>,
    ) -> Vec<MetricFamily> {
        let path = path.trim_end_matches('/');
        let groups: Vec<&Arc<MetricsGroup>> = if path.is_empty() || path == "/" {
            self.groups.iter().collect()
        } else {
            self.groups
                .iter()
                .filter(|g| g.path == path || is_descendant_of(&g.path, path))
                .collect()
        };

        let mut all = Vec::new();
        for g in groups {
            all.extend(g.gather());
        }

        if let Some(buckets) = bucket_filter {
            apply_bucket_filter(&mut all, buckets);
        }

        all
    }

    /// Encode gathered metrics as Prometheus text format.
    pub fn encode_text(
        &self,
        path: &str,
        bucket_filter: Option<&HashSet<String>>,
    ) -> Result<String, String> {
        let mfs = self.gather(path, bucket_filter);
        let encoder = TextEncoder::new();
        let mut buf = Vec::new();
        encoder
            .encode(&mfs, &mut buf)
            .map_err(|e| format!("encode error: {e}"))?;
        String::from_utf8(buf).map_err(|e| format!("utf8 error: {e}"))
    }

    /// List metric metadata for `?list` queries.
    pub fn list(&self, path: Option<&str>) -> BTreeMap<String, Vec<MetricInfo>> {
        let path = path.unwrap_or("").trim_end_matches('/');
        let groups: Vec<&Arc<MetricsGroup>> = if path.is_empty() || path == "/" {
            self.groups.iter().collect()
        } else {
            self.groups
                .iter()
                .filter(|g| g.path == path || is_descendant_of(&g.path, path))
                .collect()
        };

        let mut map: BTreeMap<String, Vec<MetricInfo>> = BTreeMap::new();
        for g in groups {
            map.insert(g.path.clone(), g.infos.clone());
        }
        map
    }
}

/// Returns true if `child` is a descendant of `parent`.
fn is_descendant_of(child: &str, parent: &str) -> bool {
    child.starts_with(parent) && child.as_bytes().get(parent.len()) == Some(&b'/')
}

/// Retain only metrics whose `bucket` label matches one of the filter values.
/// Metrics without a `bucket` label are kept (they are global/system metrics).
fn apply_bucket_filter(mfs: &mut [MetricFamily], buckets: &HashSet<String>) {
    for mf in mfs.iter_mut() {
        mf.mut_metric().retain(|m| {
            let bucket_value = m
                .get_label()
                .iter()
                .find(|lp| lp.name() == "bucket")
                .map(|lp| lp.value().to_string());
            match bucket_value {
                Some(b) => buckets.contains(&b),
                None => true,
            }
        });
    }
}
