//! Iceberg Catalog 连接管理。
//!
//! 支持两种后端：
//! - MinIO AIStor Tables (SigV4 认证)
//! - Apache Polaris (OAuth2 认证)

use crate::iceberg::schema::*;
use crate::iceberg::tree::NamespaceInfo;
use reqwest::Client;
use serde_json::Value;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;


// ---------------------------------------------------------------------------
// Catalog 类型枚举
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExternalCatalogType {
    None,
    Polaris,
}

impl Default for ExternalCatalogType {
    fn default() -> Self {
        Self::None
    }
}

impl ExternalCatalogType {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "polaris" => Self::Polaris,
            _ => Self::None,
        }
    }
}

// ---------------------------------------------------------------------------
// CatalogConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CatalogConfig {
    pub catalog_uri: String,
    pub warehouse: String,
    pub access_key: String,
    pub secret_key: String,
    pub region: String,
    pub tls: bool,
    pub external_catalog: ExternalCatalogType,
}

impl Default for CatalogConfig {
    fn default() -> Self {
        Self {
            catalog_uri: "http://localhost:9001".into(),
            warehouse: "benchmarkcatalog".into(),
            access_key: "minioadmin".into(),
            secret_key: "minioadmin".into(),
            region: "us-east-1".into(),
            tls: false,
            external_catalog: ExternalCatalogType::None,
        }
    }
}

// ---------------------------------------------------------------------------
// RestCatalog — Iceberg REST Catalog HTTP 客户端
// ---------------------------------------------------------------------------

pub struct RestCatalog {
    base_url: String,
    warehouse: String,
    client: Client,
    auth_header: Option<String>,
}

impl RestCatalog {
    /// AIStor Tables catalog（SigV4 签名）
    pub fn new_ai_stor(
        cfg: &CatalogConfig,
    ) -> Result<Self, String> {
        let base_url = format!(
            "{}://{}/_iceberg/v1",
            if cfg.tls { "https" } else { "http" },
            cfg.catalog_uri
                .trim_start_matches("http://")
                .trim_start_matches("https://")
        );
        Ok(Self {
            base_url,
            warehouse: cfg.warehouse.clone(),
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .map_err(|e| format!("failed to build HTTP client: {e}"))?,
            auth_header: None, // SigV4 per-request
        })
    }

    /// Polaris catalog（OAuth2）
    pub fn new_polaris(cfg: &CatalogConfig) -> Result<Self, String> {
        let token = format!("{}:{}", cfg.access_key, cfg.secret_key);
        let auth = format!("Bearer {}", base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            token.as_bytes(),
        ));
        let base_url = format!(
            "{}://{}/api/catalog/v1",
            if cfg.tls { "https" } else { "http" },
            cfg.catalog_uri
                .trim_start_matches("http://")
                .trim_start_matches("https://")
        );
        Ok(Self {
            base_url,
            warehouse: cfg.warehouse.clone(),
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .map_err(|e| format!("failed to build HTTP client: {e}"))?,
            auth_header: Some(auth),
        })
    }

    /// 根据配置自动选择后端
    pub fn new(cfg: &CatalogConfig) -> Result<Self, String> {
        match cfg.external_catalog {
            ExternalCatalogType::Polaris => Self::new_polaris(cfg),
            _ => Self::new_ai_stor(cfg),
        }
    }

    /// 构建 REST API URL
    fn api_url(&self, path: &str) -> String {
        format!("{}/{}/{}", self.base_url, self.warehouse, path.trim_start_matches('/'))
    }

    async fn get(&self, path: &str) -> Result<serde_json::Value, String> {
        let url = self.api_url(path);
        let mut req = self.client.get(&url);
        if let Some(auth) = &self.auth_header {
            req = req.header("Authorization", auth.clone());
        }
        let resp = req.send().await.map_err(|e| format!("GET {path}: {e}"))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("GET {path}: {status} - {body}"));
        }
        resp.json()
            .await
            .map_err(|e| format!("GET {path} JSON: {e}"))
    }

    async fn post(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let url = self.api_url(path);
        let mut req = self
            .client
            .post(&url)
            .header("Content-Type", "application/json");
        if let Some(auth) = &self.auth_header {
            req = req.header("Authorization", auth.clone());
        }
        let resp = req
            .json(body)
            .send()
            .await
            .map_err(|e| format!("POST {path}: {e}"))?;
        let status = resp.status();
        let resp_body = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(format!("POST {path}: {status} - {resp_body}"));
        }
        if resp_body.is_empty() {
            return Ok(serde_json::Value::Null);
        }
        serde_json::from_str(&resp_body)
            .map_err(|e| format!("POST {path} JSON: {e}"))
    }

    async fn post_body(
        &self,
        path: &str,
        body: Value,
    ) -> Result<serde_json::Value, String> {
        self.post(path, &body).await
    }

    async fn delete_raw(&self, path: &str) -> Result<(), String> {
        let url = self.api_url(path);
        let mut req = self.client.delete(&url);
        if let Some(auth) = &self.auth_header {
            req = req.header("Authorization", auth.clone());
        }
        let resp = req
            .send()
            .await
            .map_err(|e| format!("DELETE {path}: {e}"))?;
        let status = resp.status();
        if !status.is_success() && status.as_u16() != 404 {
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("DELETE {path}: {status} - {body}"));
        }
        Ok(())
    }

    async fn head(&self, path: &str) -> Result<bool, String> {
        let url = self.api_url(path);
        let mut req = self.client.head(&url);
        if let Some(auth) = &self.auth_header {
            req = req.header("Authorization", auth.clone());
        }
        let resp = req.send().await.map_err(|e| format!("HEAD {path}: {e}"))?;
        if resp.status().as_u16() == 404 {
            return Ok(false);
        }
        if !resp.status().is_success() {
            return Err(format!("HEAD {path}: {}", resp.status()));
        }
        Ok(true)
    }

    // ---- Namespace ops ----

    pub async fn list_namespaces(
        &self,
        parent: Option<&NamespaceInfo>,
    ) -> Result<Vec<String>, String> {
        let path = match parent {
            Some(p) => format!("namespaces?parent={}", p.path.join(".")),
            None => "namespaces".into(),
        };
        let v = self.get(&path).await?;
        let namespaces: Vec<String> = v
            .get("namespaces")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|n| n.as_array())
                    .map(|parts| {
                        parts
                            .iter()
                            .filter_map(|p| p.as_str())
                            .collect::<Vec<_>>()
                            .join(".")
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(namespaces)
    }

    pub async fn namespace_exists(&self, ns: &[String]) -> Result<bool, String> {
        let path = format!("namespaces/{}", ns.join("."));
        self.head(&path).await
    }

    pub async fn load_namespace(
        &self,
        ns: &[String],
    ) -> Result<serde_json::Value, String> {
        let path = format!("namespaces/{}", ns.join("."));
        self.get(&path).await
    }

    pub async fn create_namespace(
        &self,
        ns: &[String],
        props: &Properties,
    ) -> Result<(), String> {
        let body = serde_json::json!({
            "namespace": ns,
            "properties": props,
        });
        let path = "namespaces";
        self.post_body(path, body).await?;
        Ok(())
    }

    pub async fn update_namespace(
        &self,
        ns: &[String],
        removals: &[String],
        updates: &Properties,
    ) -> Result<(), String> {
        let body = serde_json::json!({
            "removals": removals,
            "updates": updates,
        });
        let path = format!("namespaces/{}/properties", ns.join("."));
        self.post_body(&path, body).await?;
        Ok(())
    }

    pub async fn drop_namespace(&self, ns: &[String]) -> Result<(), String> {
        let path = format!("namespaces/{}", ns.join("."));
        self.delete_raw(&path).await
    }

    // ---- Table ops ----

    pub async fn list_tables(
        &self,
        ns: &[String],
        _page_size: usize,
    ) -> Result<Vec<String>, String> {
        let path = format!("namespaces/{}/tables", ns.join("."));
        let v = self.get(&path).await?;
        let tables: Vec<String> = v
            .get("identifiers")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| t.get("name"))
                    .filter_map(|n| n.as_str())
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default();
        Ok(tables)
    }

    pub async fn table_exists(
        &self,
        ns: &[String],
        table: &str,
    ) -> Result<bool, String> {
        let path = format!("namespaces/{}/tables/{}", ns.join("."), table);
        self.head(&path).await
    }

    pub async fn load_table(
        &self,
        ns: &[String],
        table: &str,
    ) -> Result<serde_json::Value, String> {
        let path = format!("namespaces/{}/tables/{}", ns.join("."), table);
        self.get(&path).await
    }

    pub async fn create_table(
        &self,
        ns: &[String],
        table: &str,
        schema: &Schema,
        props: &Properties,
        location: &str,
    ) -> Result<(), String> {
        let body = serde_json::json!({
            "name": table,
            "schema": serde_json::to_value(schema).unwrap_or_default(),
            "location": location,
            "properties": props,
        });
        let path = format!("namespaces/{}/tables", ns.join("."));
        match self.post_body(&path, body).await {
            Ok(_) => Ok(()),
            Err(e) => {
                if e.contains("AlreadyExists") || e.contains("Conflict") {
                    Ok(()) // 幂等
                } else {
                    Err(e)
                }
            }
        }
    }

    pub async fn update_table_properties(
        &self,
        ns: &[String],
        table: &str,
        props: &Properties,
    ) -> Result<(), String> {
        let updates = vec![serde_json::json!({
            "action": "set-properties",
            "updates": props,
        })];
        let body = serde_json::json!({
            "requirements": [],
            "updates": updates,
        });
        let path = format!("namespaces/{}/tables/{}", ns.join("."), table);
        self.post_body(&path, body).await?;
        Ok(())
    }

    pub async fn drop_table(
        &self,
        ns: &[String],
        table: &str,
    ) -> Result<(), String> {
        let path = format!(
            "namespaces/{}/tables/{}?purgeRequested=true",
            ns.join("."),
            table
        );
        self.delete_raw(&path).await
    }

    // ---- View ops ----

    pub async fn list_views(
        &self,
        ns: &[String],
    ) -> Result<Vec<String>, String> {
        let path = format!("namespaces/{}/views", ns.join("."));
        let v = self.get(&path).await?;
        let views: Vec<String> = v
            .get("identifiers")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| t.get("name"))
                    .filter_map(|n| n.as_str())
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default();
        Ok(views)
    }

    pub async fn view_exists(
        &self,
        ns: &[String],
        view: &str,
    ) -> Result<bool, String> {
        let path = format!("namespaces/{}/views/{}", ns.join("."), view);
        self.head(&path).await
    }

    pub async fn load_view(
        &self,
        ns: &[String],
        view: &str,
    ) -> Result<serde_json::Value, String> {
        let path = format!("namespaces/{}/views/{}", ns.join("."), view);
        self.get(&path).await
    }

    pub async fn create_view(
        &self,
        ns: &[String],
        view: &str,
        schema: &Schema,
        location: &str,
        view_version: &ViewVersion,
        props: &Properties,
    ) -> Result<(), String> {
        let body = serde_json::json!({
            "name": view,
            "location": location,
            "schema": serde_json::to_value(schema).unwrap_or_default(),
            "view-version": serde_json::to_value(view_version).unwrap_or_default(),
            "properties": props,
        });
        let path = format!("namespaces/{}/views", ns.join("."));
        match self.post_body(&path, body).await {
            Ok(_) => Ok(()),
            Err(e) => {
                if e.contains("AlreadyExists") || e.contains("Conflict") {
                    Ok(())
                } else {
                    Err(e)
                }
            }
        }
    }

    pub async fn update_view_properties(
        &self,
        ns: &[String],
        view: &str,
        props: &Properties,
    ) -> Result<(), String> {
        let updates = vec![serde_json::json!({
            "action": "set-properties",
            "updates": props,
        })];
        let body = serde_json::json!({
            "requirements": [],
            "updates": updates,
        });
        let path = format!("namespaces/{}/views/{}", ns.join("."), view);
        self.post_body(&path, body).await?;
        Ok(())
    }

    pub async fn drop_view(
        &self,
        ns: &[String],
        view: &str,
    ) -> Result<(), String> {
        let path = format!("namespaces/{}/views/{}", ns.join("."), view);
        self.delete_raw(&path).await
    }
}

// ---------------------------------------------------------------------------
// CatalogPool — Round-robin 多 host catalog 池
// ---------------------------------------------------------------------------

pub struct CatalogPool {
    catalogs: Vec<Arc<RestCatalog>>,
    counter: AtomicU64,
}

impl CatalogPool {
    pub fn new(catalogs: Vec<Arc<RestCatalog>>) -> Self {
        Self {
            catalogs,
            counter: AtomicU64::new(0),
        }
    }

    pub fn get(&self) -> &Arc<RestCatalog> {
        let idx = self.counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        &self.catalogs[idx as usize % self.catalogs.len()]
    }

    pub fn first(&self) -> &Arc<RestCatalog> {
        &self.catalogs[0]
    }

    pub fn len(&self) -> usize {
        self.catalogs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.catalogs.is_empty()
    }
}
