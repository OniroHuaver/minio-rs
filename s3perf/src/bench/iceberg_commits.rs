//! Iceberg Catalog Commits benchmark — Table/View 属性更新压测。

use crate::bench::{Benchmark, Common, Operation};
use crate::iceberg::catalog::{CatalogPool, RestCatalog};
use crate::iceberg::dataset::DatasetCreator;
use crate::iceberg::tree::{TableInfo, Tree, TreeConfig, ViewInfo};
use crate::iceberg::{CatalogConfig, ExternalCatalogType, RetryConfig};
use chrono::Utc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

pub struct IcebergCommitsBenchmark {
    pub common: Common,
    pub catalog_config: CatalogConfig,
    pub catalog: Option<Arc<RestCatalog>>,
    pub catalog_pool: Option<Arc<CatalogPool>>,
    pub tree_config: TreeConfig,
    pub external_catalog: ExternalCatalogType,
    pub table_workers: usize,
    pub view_workers: usize,
    pub retry_config: RetryConfig,

    pub tables: Mutex<Vec<TableInfo>>,
    pub views: Mutex<Vec<ViewInfo>>,
    pub tree: Mutex<Option<Tree>>,
    pub ops: Mutex<Vec<Operation>>,
}

fn spawn_update_worker(
    worker_type: &'static str,
    ctx: CancellationToken,
    collector: Arc<dyn crate::bench::collector::Collector>,
    _catalog_config: CatalogConfig,
    _catalog: Option<Arc<RestCatalog>>,
    _catalog_pool: Option<Arc<CatalogPool>>,
    tree_config: TreeConfig,
    _external_catalog: ExternalCatalogType,
    _retry_config: RetryConfig,
    table_count: usize,
    view_count: usize,
    tables: Vec<TableInfo>,
    views: Vec<ViewInfo>,
    thread: usize,
) -> tokio::task::JoinHandle<()> {
    let endpoint = tree_config.catalog_name.clone();

    tokio::spawn(async move {
        let sender = collector.sender();
        let global_id = AtomicU64::new(0);
        let mut idx = thread;

        loop {
            if ctx.is_cancelled() {
                break;
            }

            let item = match worker_type {
                "TABLE" if !tables.is_empty() => {
                    let t = &tables[idx % tables.len()];
                    idx += 1;
                    let file = format!(
                        "{}/{}/{}",
                        endpoint,
                        t.namespace.join("."),
                        t.name
                    );
                    Some((t.namespace.clone(), t.name.clone(), file))
                }
                "VIEW" if !views.is_empty() => {
                    let v = &views[idx % views.len()];
                    idx += 1;
                    let file = format!(
                        "{}/{}/{}",
                        endpoint,
                        v.namespace.join("."),
                        v.name
                    );
                    Some((v.namespace.clone(), v.name.clone(), file))
                }
                _ => None,
            };

            if let Some((_ns, _name, file)) = item {
                if idx % (match worker_type {
                    "TABLE" => table_count.max(1),
                    _ => view_count.max(1),
                }) == 0
                {
                    global_id.fetch_add(1, Ordering::Relaxed);
                }
                let _update_id = global_id.load(Ordering::Relaxed);

                let now = Utc::now();
                let op_type = match worker_type {
                    "TABLE" => "TABLE_UPDATE",
                    _ => "VIEW_UPDATE",
                };

                let mut op = Operation {
                    start: now,
                    end: now,
                    first_byte: None,
                    last_byte: None,
                    op_type: op_type.into(),
                    err: String::new(),
                    file,
                    client_id: format!("c{}", thread),
                    endpoint: endpoint.clone(),
                    obj_per_op: 1,
                    size: 0,
                    thread: thread as u32,
                    categories: 0,
                };

                // TODO: Actual catalog API call with retry
                // cat.update_table_properties(&ns, &name, &props).await
                op.end = Utc::now();
                let _ = sender.send(op);
            }

            tokio::task::yield_now().await;
        }
    })
}

#[async_trait::async_trait]
impl Benchmark for IcebergCommitsBenchmark {
    async fn prepare(&self, ctx: &CancellationToken) -> crate::generator::Result<()> {
        let tree = Tree::new(self.tree_config.clone());
        *self.tables.lock().unwrap() = tree.all_tables();
        *self.views.lock().unwrap() = tree.all_views();
        *self.tree.lock().unwrap() = Some(tree);

        if self.common.client_idx > 0 {
            return Ok(());
        }
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
        let concurrency = self.common.concurrency;
        let tables = self.tables.lock().unwrap().clone();
        let views = self.views.lock().unwrap().clone();

        let table_workers = if self.table_workers > 0 {
            self.table_workers
        } else if !tables.is_empty() {
            (concurrency / 2).max(1)
        } else {
            0
        };

        let view_workers = if self.view_workers > 0 {
            self.view_workers
        } else if !views.is_empty() {
            (concurrency / 2).max(1)
        } else {
            0
        };

        let mut handles = Vec::new();

        for i in 0..table_workers {
            handles.push(spawn_update_worker(
                "TABLE",
                ctx.clone(),
                self.common.collector.clone(),
                self.catalog_config.clone(),
                self.catalog.clone(),
                self.catalog_pool.clone(),
                self.tree_config.clone(),
                self.external_catalog.clone(),
                self.retry_config.clone(),
                tables.len(), views.len(),
                tables.clone(), views.clone(),
                i,
            ));
        }

        for i in 0..view_workers {
            handles.push(spawn_update_worker(
                "VIEW",
                ctx.clone(),
                self.common.collector.clone(),
                self.catalog_config.clone(),
                self.catalog.clone(),
                self.catalog_pool.clone(),
                self.tree_config.clone(),
                self.external_catalog.clone(),
                self.retry_config.clone(),
                tables.len(), views.len(),
                tables.clone(), views.clone(),
                i + table_workers,
            ));
        }

        tokio::time::sleep(self.common.duration).await;
        ctx.cancel();

        for h in handles {
            let _ = h.await;
        }
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
