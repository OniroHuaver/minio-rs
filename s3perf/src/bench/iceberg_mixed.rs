//! Iceberg Catalog Mixed benchmark — 12 种读/写操作按权重混合。

use crate::bench::{Benchmark, Common, Operation};
use crate::iceberg::catalog::{CatalogPool, RestCatalog};
use crate::iceberg::dataset::DatasetCreator;
use crate::iceberg::distribution::IcebergMixedDistribution;
use crate::iceberg::tree::{NamespaceInfo, TableInfo, Tree, TreeConfig, ViewInfo};
use crate::iceberg::{CatalogConfig, ExternalCatalogType, RetryConfig};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

pub struct IcebergMixedBenchmark {
    pub common: Common,
    pub catalog_config: CatalogConfig,
    pub catalog: Option<Arc<RestCatalog>>,
    pub catalog_pool: Option<Arc<CatalogPool>>,
    pub tree_config: TreeConfig,
    pub external_catalog: ExternalCatalogType,
    pub dist: Option<IcebergMixedDistribution>,
    pub page_size: usize,
    pub retry_config: RetryConfig,

    pub namespaces: Mutex<Vec<NamespaceInfo>>,
    pub tables: Mutex<Vec<TableInfo>>,
    pub views: Mutex<Vec<ViewInfo>>,
    pub tree: Mutex<Option<Tree>>,
    pub ns_update_id: AtomicU64,
    pub table_update_id: AtomicU64,
    pub view_update_id: AtomicU64,
    pub ops: Mutex<Vec<Operation>>,
}

impl IcebergMixedBenchmark {
    fn default_dists() -> HashMap<String, f64> {
        HashMap::from([
            ("NS_LIST".into(), 10.0),
            ("NS_HEAD".into(), 10.0),
            ("NS_GET".into(), 10.0),
            ("TABLE_LIST".into(), 10.0),
            ("TABLE_HEAD".into(), 10.0),
            ("TABLE_GET".into(), 10.0),
            ("VIEW_LIST".into(), 10.0),
            ("VIEW_HEAD".into(), 10.0),
            ("VIEW_GET".into(), 10.0),
            ("NS_UPDATE".into(), 5.0),
            ("TABLE_UPDATE".into(), 5.0),
            ("VIEW_UPDATE".into(), 5.0),
        ])
    }
}

#[async_trait::async_trait]
impl Benchmark for IcebergMixedBenchmark {
    async fn prepare(&self, ctx: &CancellationToken) -> crate::generator::Result<()> {
        let tree = Tree::new(self.tree_config.clone());
        *self.namespaces.lock().unwrap() = tree.all_namespaces();
        *self.tables.lock().unwrap() = tree.all_tables();
        *self.views.lock().unwrap() = tree.all_views();
        *self.tree.lock().unwrap() = Some(tree);

        if self.common.client_idx > 0 { return Ok(()); }
        let cat = RestCatalog::new(&self.catalog_config)
            .map_err(|e| crate::generator::Error::S3(e))?;
        let tree = Tree::new(self.tree_config.clone());
        let creator = DatasetCreator {
            catalog: Some(Arc::new(cat)),
            catalog_pool: None, tree,
            catalog_uri: self.catalog_config.catalog_uri.clone(),
            access_key: self.catalog_config.access_key.clone(),
            secret_key: self.catalog_config.secret_key.clone(),
            region: self.catalog_config.region.clone(),
            concurrency: self.common.concurrency.min(20),
            external_catalog: self.external_catalog.clone(),
            on_progress: None,
        };
        creator.create_all(ctx).await
            .map_err(|e| crate::generator::Error::S3(e))?;
        Ok(())
    }

    async fn start(
        &self,
        ctx: &CancellationToken,
        _wait: tokio::sync::broadcast::Receiver<()>,
    ) -> crate::generator::Result<()> {
        let dist = self
            .dist
            .clone()
            .unwrap_or_else(|| IcebergMixedDistribution::new(Self::default_dists()).unwrap());

        let collector = self.common.collector.clone();
        let concurrency = self.common.concurrency;
        let hosts = self.common.hosts.clone();
        let catalog_name = self.tree_config.catalog_name.clone();

        let namespaces = self.namespaces.lock().unwrap().clone();
        let tables = self.tables.lock().unwrap().clone();
        let views = self.views.lock().unwrap().clone();

        let mut handles = Vec::with_capacity(concurrency);

        for thread in 0..concurrency {
            let ctx = ctx.clone();
            let collector = collector.clone();
            let dist = dist.clone();
            let endpoint = hosts
                .get(self.common.host_index(thread))
                .cloned()
                .unwrap_or_default();
            let catalog_name = catalog_name.clone();
            let namespaces = namespaces.clone();
            let tables = tables.clone();
            let views = views.clone();

            handles.push(tokio::spawn(async move {
                let mut ns_idx = thread % namespaces.len().max(1);
                let mut tbl_idx = thread % tables.len().max(1);
                let mut vw_idx = thread % views.len().max(1);

                let ns_update_id = AtomicU64::new(0);
                let table_update_id = AtomicU64::new(0);
                let view_update_id = AtomicU64::new(0);

                loop {
                    if ctx.is_cancelled() { break; }

                    let op_type = dist.get_op();
                    let now = Utc::now();

                    let mut op = Operation {
                        start: now, end: now, first_byte: None, last_byte: None,
                        op_type: op_type.clone(), err: String::new(),
                        file: String::new(),
                        client_id: format!("c{}", thread),
                        endpoint: endpoint.clone(),
                        obj_per_op: 1, size: 0, thread: thread as u32, categories: 0,
                    };

                    match op_type.as_str() {
                        "NS_LIST" if !namespaces.is_empty() => {
                            let ns = &namespaces[ns_idx % namespaces.len()];
                            ns_idx += 1;
                            op.file = format!("{}/{}/", catalog_name, ns.path.join("."));
                        }
                        "NS_HEAD" if !namespaces.is_empty() => {
                            let ns = &namespaces[ns_idx % namespaces.len()];
                            ns_idx += 1;
                            op.file = format!("{}/{}/", catalog_name, ns.path.join("."));
                        }
                        "NS_GET" if !namespaces.is_empty() => {
                            let ns = &namespaces[ns_idx % namespaces.len()];
                            ns_idx += 1;
                            op.file = format!("{}/{}/", catalog_name, ns.path.join("."));
                        }
                        "TABLE_LIST" if !tables.is_empty() => {
                            let t = &tables[tbl_idx % tables.len()];
                            tbl_idx += 1;
                            op.file = format!("{}/{}/", catalog_name, t.namespace.join("."));
                        }
                        "TABLE_HEAD" if !tables.is_empty() => {
                            let t = &tables[tbl_idx % tables.len()];
                            tbl_idx += 1;
                            op.file = format!("{}/{}/{}", catalog_name, t.namespace.join("."), t.name);
                        }
                        "TABLE_GET" if !tables.is_empty() => {
                            let t = &tables[tbl_idx % tables.len()];
                            tbl_idx += 1;
                            op.file = format!("{}/{}/{}", catalog_name, t.namespace.join("."), t.name);
                        }
                        "VIEW_LIST" if !views.is_empty() => {
                            let v = &views[vw_idx % views.len()];
                            vw_idx += 1;
                            op.file = format!("{}/{}/", catalog_name, v.namespace.join("."));
                        }
                        "VIEW_HEAD" if !views.is_empty() => {
                            let v = &views[vw_idx % views.len()];
                            vw_idx += 1;
                            op.file = format!("{}/{}/{}", catalog_name, v.namespace.join("."), v.name);
                        }
                        "VIEW_GET" if !views.is_empty() => {
                            let v = &views[vw_idx % views.len()];
                            vw_idx += 1;
                            op.file = format!("{}/{}/{}", catalog_name, v.namespace.join("."), v.name);
                        }
                        "NS_UPDATE" if !namespaces.is_empty() => {
                            let id = ns_update_id.fetch_add(1, Ordering::Relaxed);
                            let ns = &namespaces[id as usize % namespaces.len()];
                            op.file = format!("{}/{}/", catalog_name, ns.path.join("."));
                        }
                        "TABLE_UPDATE" if !tables.is_empty() => {
                            let id = table_update_id.fetch_add(1, Ordering::Relaxed);
                            let t = &tables[id as usize % tables.len()];
                            op.file = format!("{}/{}/{}", catalog_name, t.namespace.join("."), t.name);
                        }
                        "VIEW_UPDATE" if !views.is_empty() => {
                            let id = view_update_id.fetch_add(1, Ordering::Relaxed);
                            let v = &views[id as usize % views.len()];
                            op.file = format!("{}/{}/{}", catalog_name, v.namespace.join("."), v.name);
                        }
                        _ => continue,
                    }

                    op.end = Utc::now();
                    let _ = collector.sender().send(op);
                    tokio::task::yield_now().await;
                }
            }));
        }

        tokio::time::sleep(self.common.duration).await;
        ctx.cancel();
        for h in handles { let _ = h.await; }
        Ok(())
    }

    async fn cleanup(&self, ctx: &CancellationToken) {
        if self.common.client_idx > 0 { return; }
        let Ok(cat) = RestCatalog::new(&self.catalog_config) else { return };
        let tree = self.tree.lock().unwrap().take()
            .unwrap_or_else(|| Tree::new(self.tree_config.clone()));
        let creator = DatasetCreator {
            catalog: Some(Arc::new(cat)),
            catalog_pool: None, tree,
            catalog_uri: self.catalog_config.catalog_uri.clone(),
            access_key: self.catalog_config.access_key.clone(),
            secret_key: self.catalog_config.secret_key.clone(),
            region: self.catalog_config.region.clone(),
            concurrency: self.common.concurrency.min(20),
            external_catalog: self.external_catalog.clone(),
            on_progress: None,
        };
        creator.delete_all(ctx).await;
    }

    fn common(&self) -> &Common { &self.common }
    fn ops(&self) -> Vec<Operation> { self.ops.lock().unwrap().to_vec() }
}
