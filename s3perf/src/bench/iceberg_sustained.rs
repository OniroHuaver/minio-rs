//! Iceberg Sustained benchmark — 持续写入 (commit) + 可选读取。

use crate::bench::{Benchmark, Common, Operation};
use crate::iceberg::catalog::{CatalogPool, RestCatalog};
use crate::iceberg::dataset::DatasetCreator;
use crate::iceberg::tree::{TableInfo, Tree, TreeConfig};
use crate::iceberg::{CatalogConfig, ExternalCatalogType, RetryConfig};
use chrono::Utc;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

pub struct IcebergSustainedBenchmark {
    pub common: Common,
    pub catalog_config: CatalogConfig,
    pub catalog: Option<Arc<RestCatalog>>,
    pub catalog_pool: Option<Arc<CatalogPool>>,
    pub tree_config: TreeConfig,
    pub external_catalog: ExternalCatalogType,
    pub num_files: usize,
    pub rows_per_file: usize,
    pub files_per_commit: usize,
    pub tpcds: bool,
    pub scale_factor: f64,
    pub tpcds_table: Option<String>,
    pub cache_dir: String,
    pub skip_upload: bool,
    pub simulate_read: bool,
    pub read_concurrent: usize,
    pub read_rps_limit: f64,
    pub retry_config: RetryConfig,

    pub tables: Mutex<Vec<TableInfo>>,
    pub tree: Mutex<Option<Tree>>,
    pub ops: Mutex<Vec<Operation>>,
}

#[async_trait::async_trait]
impl Benchmark for IcebergSustainedBenchmark {
    async fn prepare(&self, ctx: &CancellationToken) -> crate::generator::Result<()> {
        let tree = Tree::new(self.tree_config.clone());
        *self.tables.lock().unwrap() = tree.all_tables();
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
        // TODO: Generate/download parquet data files, upload if skip_upload
        Ok(())
    }

    async fn start(
        &self,
        ctx: &CancellationToken,
        _wait: tokio::sync::broadcast::Receiver<()>,
    ) -> crate::generator::Result<()> {
        let collector = self.common.collector.clone();
        let concurrency = self.common.concurrency;
        let tables = self.tables.lock().unwrap().clone();
        let catalog_name = self.tree_config.catalog_name.clone();

        let mut handles = Vec::new();

        // Commit workers
        for thread in 0..concurrency {
            let ctx = ctx.clone();
            let collector = collector.clone();
            let tables = tables.clone();
            let catalog_name = catalog_name.clone();

            handles.push(tokio::spawn(async move {
                let mut tbl_idx = thread % tables.len().max(1);
                loop {
                    if ctx.is_cancelled() { break; }

                    let tbl = &tables[tbl_idx % tables.len()];
                    tbl_idx += 1;

                    let now = Utc::now();
                    let op = Operation {
                        start: now, end: now, first_byte: None, last_byte: None,
                        op_type: "COMMIT".into(), err: String::new(),
                        file: format!("{}/{}/{}", catalog_name, tbl.namespace.join("."), tbl.name),
                        client_id: format!("c{}", thread),
                        endpoint: catalog_name.clone(),
                        obj_per_op: 1, size: 0, thread: thread as u32, categories: 0,
                    };
                    // TODO: actual commit with parquet files
                    let _ = collector.sender().send(op);
                    tokio::task::yield_now().await;
                }
            }));
        }

        // Optional read workers
        if self.simulate_read {
            for thread in 0..self.read_concurrent {
                let ctx = ctx.clone();
                let collector = collector.clone();
                let tables = tables.clone();
                let catalog_name = catalog_name.clone();

                handles.push(tokio::spawn(async move {
                    let mut tbl_idx = thread % tables.len().max(1);
                    loop {
                        if ctx.is_cancelled() { break; }
                        let tbl = &tables[tbl_idx % tables.len()];
                        tbl_idx += 1;

                        let now = Utc::now();
                        let op = Operation {
                            start: now, end: now, first_byte: None, last_byte: None,
                            op_type: "TABLE_GET".into(), err: String::new(),
                            file: format!("{}/{}/{}", catalog_name, tbl.namespace.join("."), tbl.name),
                            client_id: format!("cr{}", thread),
                            endpoint: catalog_name.clone(),
                            obj_per_op: 1, size: 0,
                            thread: (thread + concurrency) as u32, categories: 0,
                        };
                        let _ = collector.sender().send(op);
                        tokio::task::yield_now().await;
                    }
                }));
            }
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
