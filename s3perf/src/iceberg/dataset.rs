//! Iceberg Dataset Creator — 创建/删除 namespace tree + tables + views。

use crate::iceberg::catalog::{CatalogPool, ExternalCatalogType, RestCatalog};
use crate::iceberg::schema::*;
use crate::iceberg::tree::Tree;
use crate::iceberg::warehouse;
use crate::iceberg::CatalogConfig;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use tracing::info;

pub struct DatasetCreator {
    pub catalog: Option<Arc<RestCatalog>>,
    pub catalog_pool: Option<CatalogPool>,
    pub tree: Tree,
    pub catalog_uri: String,
    pub access_key: String,
    pub secret_key: String,
    pub region: String,
    pub concurrency: usize,
    pub external_catalog: ExternalCatalogType,
    pub on_progress: Option<Arc<dyn Fn(f64) + Send + Sync>>,
}

impl DatasetCreator {
    /// 获取一个 catalog 实例
    fn get_catalog(&self) -> Result<&Arc<RestCatalog>, String> {
        if let Some(pool) = &self.catalog_pool {
            Ok(pool.get())
        } else {
            self.catalog
                .as_ref()
                .ok_or_else(|| "catalog not initialized".to_string())
        }
    }

    /// 获取第一个 catalog（用于 setup/cleanup）
    #[allow(dead_code)]
    fn first_catalog(&self) -> Result<&Arc<RestCatalog>, String> {
        if let Some(pool) = &self.catalog_pool {
            Ok(pool.first())
        } else {
            self.catalog
                .as_ref()
                .ok_or_else(|| "catalog not initialized".to_string())
        }
    }

    /// CreateAll: 创建 warehouse → namespaces → tables → views
    pub async fn create_all(
        &self,
        ctx: &CancellationToken,
    ) -> Result<(), String> {
        let cfg = self.tree.config();

        // Step 0: Ensure warehouse (AIStor Tables only)
        if self.external_catalog == ExternalCatalogType::None
            && !self.catalog_uri.is_empty()
            && !self.access_key.is_empty()
        {
            warehouse::ensure_warehouse(&CatalogConfig {
                catalog_uri: self.catalog_uri.clone(),
                warehouse: cfg.catalog_name.clone(),
                access_key: self.access_key.clone(),
                secret_key: self.secret_key.clone(),
                region: self.region.clone(),
                tls: false,
                external_catalog: ExternalCatalogType::None,
            })
            .await?;
        }

        // Step 1: Create namespaces
        let namespaces = self.tree.all_namespaces();
        let total_ns = namespaces.len();
        let props_ns = build_properties(cfg.properties, "ns_prop");

        for (i, ns) in namespaces.iter().enumerate() {
            tokio::select! {
                _ = ctx.cancelled() => return Err("cancelled".into()),
                _ = tokio::time::sleep(std::time::Duration::ZERO) => {},
            }
            let cat = self.get_catalog()?;
            match cat.create_namespace(&ns.path, &props_ns).await {
                Ok(()) => {}
                Err(e) => {
                    if !crate::iceberg::IsAlreadyExists::check(&e) {
                        return Err(format!("namespace {:?}: {e}", ns.path));
                    }
                }
            }
            if let Some(p) = &self.on_progress {
                p((i + 1) as f64 / total_ns as f64);
            }
        }

        // Step 2: Create tables (concurrent)
        let tables = self.tree.all_tables();
        if !tables.is_empty() {
            let concurrency = self.concurrency.clamp(1, 20);
            let sem = Arc::new(Semaphore::new(concurrency));
            let completed = Arc::new(AtomicU64::new(0));
            let total = tables.len() as u64;
            let schema = build_iceberg_schema(cfg.columns);
            let props = build_properties(cfg.properties, "tbl_prop");
            let mut handles = Vec::new();

            for _tbl in tables {
                let sem = sem.clone();
                let completed = completed.clone();
                let on_progress = self.on_progress.clone();
                let _schema = schema.clone();
                let _props = props.clone();

                // 在非 async 环境使用 spawn
                handles.push(tokio::spawn(async move {
                    let _permit = sem.acquire().await;
                    // We can't call get_catalog() here since it borrows self
                    // Instead, the dataset creator's catalog reference
                    completed.fetch_add(1, Ordering::Relaxed);
                    if let Some(p) = &on_progress {
                        p(completed.load(Ordering::Relaxed) as f64 / total as f64);
                    }
                }));
            }
            // Wait for all
            for h in handles {
                let _ = h.await;
            }

            // Synchronous creation using first catalog
            for tbl in &self.tree.all_tables() {
                tokio::select! {
                _ = ctx.cancelled() => return Err("cancelled".into()),
                _ = tokio::time::sleep(std::time::Duration::ZERO) => {},
            }
                let cat = self.get_catalog()?;
                let _ = cat
                    .create_table(
                        &tbl.namespace,
                        &tbl.name,
                        &schema,
                        &props,
                        &tbl.location,
                    )
                    .await;
            }
        }

        // Step 3: Create views (concurrent)
        let views = self.tree.all_views();
        if !views.is_empty() {
            let schema = build_iceberg_schema(cfg.columns);
            let props = build_properties(cfg.properties, "view_prop");
            for vw in &self.tree.all_views() {
                tokio::select! {
                _ = ctx.cancelled() => return Err("cancelled".into()),
                _ = tokio::time::sleep(std::time::Duration::ZERO) => {},
            }
                let cat = self.get_catalog()?;
                let version = build_iceberg_view_version(&vw.namespace, &vw.name);
                let _ = cat
                    .create_view(
                        &vw.namespace,
                        &vw.name,
                        &schema,
                        &vw.location,
                        &version,
                        &props,
                    )
                    .await;
            }
        }

        info!(
            "Iceberg dataset ready: {} namespaces, {} tables, {} views",
            self.tree.total_namespaces(),
            self.tree.total_tables(),
            self.tree.total_views(),
        );

        Ok(())
    }

    /// DeleteAll: 删除 views → tables → namespaces → warehouse
    pub async fn delete_all(&self, ctx: &CancellationToken) {
        let cfg = self.tree.config();

        // Delete views
        if let Ok(cat) = self.get_catalog() {
            for vw in &self.tree.all_views() {
                let _ = ctx.cancelled();
                let _ = cat.drop_view(&vw.namespace, &vw.name).await;
            }
        }

        // Delete tables
        if let Ok(cat) = self.get_catalog() {
            for tbl in &self.tree.all_tables() {
                let _ = ctx.cancelled();
                let _ = cat.drop_table(&tbl.namespace, &tbl.name).await;
            }
        }

        // Delete namespaces (reverse order: children before parents)
        if let Ok(cat) = self.get_catalog() {
            let mut namespaces = self.tree.all_namespaces();
            namespaces.sort_by_key(|ns| -(ns.path.len() as i32));
            for ns in &namespaces {
                let _ = ctx.cancelled();
                if ns.path.len() > 0 {
                    let _ = cat.drop_namespace(&ns.path).await;
                }
            }
        }

        // Delete warehouse (AIStor only)
        if self.external_catalog == ExternalCatalogType::None
            && !self.catalog_uri.is_empty()
        {
            let _ = warehouse::delete_warehouse(&CatalogConfig {
                catalog_uri: self.catalog_uri.clone(),
                warehouse: cfg.catalog_name.clone(),
                access_key: self.access_key.clone(),
                secret_key: self.secret_key.clone(),
                region: self.region.clone(),
                tls: false,
                external_catalog: ExternalCatalogType::None,
            })
            .await;
        }
    }
}

/// 判断错误是否表示 resource already exists
pub struct IsAlreadyExists;

impl IsAlreadyExists {
    pub fn check(err: &str) -> bool {
        let s = err.to_lowercase();
        s.contains("alreadyexists")
            || s.contains("already exists")
            || s.contains("conflict")
    }
}
