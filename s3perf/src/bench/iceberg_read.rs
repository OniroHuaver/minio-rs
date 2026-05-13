//! Iceberg Catalog Read benchmark — 9 种读操作按权重分布。

use crate::bench::{Benchmark, Common, Operation};
use crate::iceberg::catalog::{CatalogPool, RestCatalog};
use crate::iceberg::dataset::DatasetCreator;
use crate::iceberg::distribution::IcebergMixedDistribution;
use crate::iceberg::tree::{NamespaceInfo, TableInfo, Tree, TreeConfig, ViewInfo};
use crate::iceberg::{CatalogConfig, ExternalCatalogType};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

pub struct IcebergReadBenchmark {
    pub common: Common,
    pub catalog_config: CatalogConfig,
    pub catalog: Option<Arc<RestCatalog>>,
    pub catalog_pool: Option<Arc<CatalogPool>>,
    pub tree_config: TreeConfig,
    pub dist: Option<IcebergMixedDistribution>,
    pub page_size: usize,
    pub external_catalog: ExternalCatalogType,

    // 已缓存的 tree 数据
    pub tree: Mutex<Option<Tree>>,
    pub namespaces: Mutex<Vec<NamespaceInfo>>,
    pub tables: Mutex<Vec<TableInfo>>,
    pub views: Mutex<Vec<ViewInfo>>,
    pub ops: Mutex<Vec<Operation>>,
}

impl IcebergReadBenchmark {
    fn distribute_weights(&self) -> HashMap<String, f64> {
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
        ])
    }
}

#[async_trait::async_trait]
impl Benchmark for IcebergReadBenchmark {
    async fn prepare(&self, ctx: &CancellationToken) -> crate::generator::Result<()> {
        let tree = Tree::new(self.tree_config.clone());
        *self.namespaces.lock().unwrap() = tree.all_namespaces();
        *self.tables.lock().unwrap() = tree.all_tables();
        *self.views.lock().unwrap() = tree.all_views();
        *self.tree.lock().unwrap() = Some(tree);

        if self.common.client_idx > 0 {
            return Ok(());
        }

        let cat =
            RestCatalog::new(&self.catalog_config).map_err(|e| crate::generator::Error::S3(e))?;
        let tree = Tree::new(self.tree_config.clone());
        let creator = DatasetCreator {
            catalog: Some(Arc::new(cat)),
            catalog_pool: None,
            tree,
            catalog_uri: self.catalog_config.catalog_uri.clone(),
            access_key: self.catalog_config.access_key.clone(),
            secret_key: self.catalog_config.secret_key.clone(),
            region: self.catalog_config.region.clone(),
            concurrency: self.common.concurrency.min(20),
            external_catalog: self.external_catalog.clone(),
            on_progress: None,
        };
        creator
            .create_all(ctx)
            .await
            .map_err(|e| crate::generator::Error::S3(e))?;
        Ok(())
    }

    async fn start(
        &self,
        ctx: &CancellationToken,
        wait: tokio::sync::broadcast::Receiver<()>,
    ) -> crate::generator::Result<()> {
        let dist: IcebergMixedDistribution =
            self.dist.as_ref().cloned().unwrap_or_else(|| {
                IcebergMixedDistribution::new(self.distribute_weights()).unwrap()
            });

        let collector = self.common.collector.clone();
        let concurrency = self.common.concurrency;
        let hosts = self.common.hosts.clone();

        let mut handles = Vec::with_capacity(concurrency);

        for thread in 0..concurrency {
            let ctx = ctx.clone();
            let collector = collector.clone();
            let dist = dist.clone(); // cheap: Mutex<Vec<String>> + Mutex<usize>
            let endpoint = hosts
                .get(self.common.host_index(thread))
                .cloned()
                .unwrap_or_else(|| "unknown".into());
            let catalog_name = self.tree_config.catalog_name.clone();
            let _page_size = self.page_size;

            let namespaces = self.namespaces.lock().unwrap().clone();
            let tables = self.tables.lock().unwrap().clone();
            let views = self.views.lock().unwrap().clone();

            handles.push(tokio::spawn(async move {
                let mut ns_idx = thread % namespaces.len().max(1);
                let mut tbl_idx = thread % tables.len().max(1);
                let mut vw_idx = thread % views.len().max(1);

                loop {
                    if ctx.is_cancelled() {
                        break;
                    }

                    let op_type = dist.get_op();
                    let now = Utc::now();
                    let (op, _should_record) = match op_type.as_str() {
                        "NS_LIST" if !namespaces.is_empty() => {
                            let ns = &namespaces[ns_idx % namespaces.len()];
                            ns_idx += 1;
                            let mut op = Operation {
                                start: now,
                                end: now,
                                first_byte: None,
                                last_byte: None,
                                op_type: "NS_LIST".into(),
                                err: String::new(),
                                file: format!("{}/{}/", catalog_name, ns.path.join(".")),
                                client_id: format!("c{}", thread),
                                endpoint: endpoint.clone(),
                                obj_per_op: 1,
                                size: 0,
                                thread: thread as u32,
                                categories: 0,
                            };
                            // Simulate catalog API call
                            op.end = Utc::now();
                            (op, true)
                        }
                        "NS_HEAD" if !namespaces.is_empty() => {
                            let ns = &namespaces[ns_idx % namespaces.len()];
                            ns_idx += 1;
                            let mut op = Operation {
                                start: now,
                                end: now,
                                first_byte: None,
                                last_byte: None,
                                op_type: "NS_HEAD".into(),
                                err: String::new(),
                                file: format!("{}/{}/", catalog_name, ns.path.join(".")),
                                client_id: format!("c{}", thread),
                                endpoint: endpoint.clone(),
                                obj_per_op: 1,
                                size: 0,
                                thread: thread as u32,
                                categories: 0,
                            };
                            op.end = Utc::now();
                            (op, true)
                        }
                        "NS_GET" if !namespaces.is_empty() => {
                            let ns = &namespaces[ns_idx % namespaces.len()];
                            ns_idx += 1;
                            let mut op = Operation {
                                start: now,
                                end: now,
                                first_byte: None,
                                last_byte: None,
                                op_type: "NS_GET".into(),
                                err: String::new(),
                                file: format!("{}/{}/", catalog_name, ns.path.join(".")),
                                client_id: format!("c{}", thread),
                                endpoint: endpoint.clone(),
                                obj_per_op: 1,
                                size: 0,
                                thread: thread as u32,
                                categories: 0,
                            };
                            op.end = Utc::now();
                            (op, true)
                        }
                        "TABLE_LIST" if !tables.is_empty() => {
                            let tbl = &tables[tbl_idx % tables.len()];
                            tbl_idx += 1;
                            let mut op = Operation {
                                start: now,
                                end: now,
                                first_byte: None,
                                last_byte: None,
                                op_type: "TABLE_LIST".into(),
                                err: String::new(),
                                file: format!("{}/{}/", catalog_name, tbl.namespace.join(".")),
                                client_id: format!("c{}", thread),
                                endpoint: endpoint.clone(),
                                obj_per_op: 1,
                                size: 0,
                                thread: thread as u32,
                                categories: 0,
                            };
                            op.end = Utc::now();
                            (op, true)
                        }
                        "TABLE_HEAD" if !tables.is_empty() => {
                            let tbl = &tables[tbl_idx % tables.len()];
                            tbl_idx += 1;
                            let mut op = Operation {
                                start: now,
                                end: now,
                                first_byte: None,
                                last_byte: None,
                                op_type: "TABLE_HEAD".into(),
                                err: String::new(),
                                file: format!(
                                    "{}/{}/{}",
                                    catalog_name,
                                    tbl.namespace.join("."),
                                    tbl.name
                                ),
                                client_id: format!("c{}", thread),
                                endpoint: endpoint.clone(),
                                obj_per_op: 1,
                                size: 0,
                                thread: thread as u32,
                                categories: 0,
                            };
                            op.end = Utc::now();
                            (op, true)
                        }
                        "TABLE_GET" if !tables.is_empty() => {
                            let tbl = &tables[tbl_idx % tables.len()];
                            tbl_idx += 1;
                            let mut op = Operation {
                                start: now,
                                end: now,
                                first_byte: None,
                                last_byte: None,
                                op_type: "TABLE_GET".into(),
                                err: String::new(),
                                file: format!(
                                    "{}/{}/{}",
                                    catalog_name,
                                    tbl.namespace.join("."),
                                    tbl.name
                                ),
                                client_id: format!("c{}", thread),
                                endpoint: endpoint.clone(),
                                obj_per_op: 1,
                                size: 0,
                                thread: thread as u32,
                                categories: 0,
                            };
                            op.end = Utc::now();
                            (op, true)
                        }
                        "VIEW_LIST" if !views.is_empty() => {
                            let vw = &views[vw_idx % views.len()];
                            vw_idx += 1;
                            let mut op = Operation {
                                start: now,
                                end: now,
                                first_byte: None,
                                last_byte: None,
                                op_type: "VIEW_LIST".into(),
                                err: String::new(),
                                file: format!("{}/{}/", catalog_name, vw.namespace.join(".")),
                                client_id: format!("c{}", thread),
                                endpoint: endpoint.clone(),
                                obj_per_op: 1,
                                size: 0,
                                thread: thread as u32,
                                categories: 0,
                            };
                            op.end = Utc::now();
                            (op, true)
                        }
                        "VIEW_HEAD" if !views.is_empty() => {
                            let vw = &views[vw_idx % views.len()];
                            vw_idx += 1;
                            let mut op = Operation {
                                start: now,
                                end: now,
                                first_byte: None,
                                last_byte: None,
                                op_type: "VIEW_HEAD".into(),
                                err: String::new(),
                                file: format!(
                                    "{}/{}/{}",
                                    catalog_name,
                                    vw.namespace.join("."),
                                    vw.name
                                ),
                                client_id: format!("c{}", thread),
                                endpoint: endpoint.clone(),
                                obj_per_op: 1,
                                size: 0,
                                thread: thread as u32,
                                categories: 0,
                            };
                            op.end = Utc::now();
                            (op, true)
                        }
                        "VIEW_GET" if !views.is_empty() => {
                            let vw = &views[vw_idx % views.len()];
                            vw_idx += 1;
                            let mut op = Operation {
                                start: now,
                                end: now,
                                first_byte: None,
                                last_byte: None,
                                op_type: "VIEW_GET".into(),
                                err: String::new(),
                                file: format!(
                                    "{}/{}/{}",
                                    catalog_name,
                                    vw.namespace.join("."),
                                    vw.name
                                ),
                                client_id: format!("c{}", thread),
                                endpoint: endpoint.clone(),
                                obj_per_op: 1,
                                size: 0,
                                thread: thread as u32,
                                categories: 0,
                            };
                            op.end = Utc::now();
                            (op, true)
                        }
                        _ => continue,
                    };
                    // TODO: Replace with actual RestCatalog API calls
                    // cat.load_namespace(&ns.path).await
                    // cat.namespace_exists(&ns.path).await
                    // 等等

                    let _ = collector.sender().send(op);
                }
            }));
        }

        // 不等待 wait — 直接开始
        drop(wait);

        // 等待 duration 或 cancel
        if let Some(dur) = self.common.auto_term_dur {
            tokio::select! {
                _ = ctx.cancelled() => {},
                _ = tokio::time::sleep(std::time::Duration::from_secs(dur.as_secs())) => {},
            }
        } else {
            tokio::time::sleep(self.common.duration).await;
        }
        ctx.cancel();

        for h in handles {
            let _ = h.await;
        }
        Ok(())
    }

    async fn cleanup(&self, ctx: &CancellationToken) {
        if self.common.client_idx > 0 {
            return;
        }
        let Ok(cat) = RestCatalog::new(&self.catalog_config) else {
            return;
        };
        let tree = self
            .tree
            .lock()
            .unwrap()
            .take()
            .unwrap_or_else(|| Tree::new(self.tree_config.clone()));
        let creator = DatasetCreator {
            catalog: Some(Arc::new(cat)),
            catalog_pool: None,
            tree,
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

    fn common(&self) -> &Common {
        &self.common
    }
    fn ops(&self) -> Vec<Operation> {
        self.ops.lock().unwrap().to_vec()
    }
}
