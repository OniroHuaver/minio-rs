//! Iceberg Schema 构建函数。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type Properties = HashMap<String, String>;

/// 简化的 Iceberg Schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schema {
    #[serde(rename = "schema-id", default)]
    pub schema_id: i32,
    #[serde(rename = "type", default = "default_schema_type")]
    pub schema_type: String,
    pub fields: Vec<Field>,
}

fn default_schema_type() -> String {
    "struct".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Field {
    pub id: i32,
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

/// 构建测试用 schema (默认 column types)
pub fn build_iceberg_schema(columns: usize) -> Schema {
    let mut fields = Vec::with_capacity(columns);
    for i in 0..columns {
        fields.push(Field {
            id: (i + 1) as i32,
            name: format!("col_{i}"),
            field_type: column_type(i),
            required: false,
            doc: None,
        });
    }
    Schema {
        schema_id: 0,
        schema_type: "struct".into(),
        fields,
    }
}

fn column_type(idx: usize) -> String {
    match idx % 5 {
        0 => "long".into(),
        1 => "string".into(),
        2 => "double".into(),
        3 => "timestamp".into(),
        _ => "boolean".into(),
    }
}

/// 构建表/视图 properties: {"tbl_prop_0": "val_0", ...}
pub fn build_properties(count: usize, prefix: &str) -> Properties {
    let mut props = HashMap::with_capacity(count + 1);
    props.insert("format-version".to_string(), "2".to_string());
    for i in 0..count {
        props.insert(format!("{prefix}_{i}"), format!("val_{i}"));
    }
    props
}

/// 构建 ViewVersion (用于 CreateView)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewVersion {
    pub version_id: i32,
    pub schema_id: i32,
    #[serde(rename = "default-namespace")]
    pub default_namespace: Vec<String>,
    pub representations: Vec<ViewRepresentation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewRepresentation {
    #[serde(rename = "type")]
    pub rep_type: String,
    pub sql: String,
    pub dialect: String,
}

pub fn build_iceberg_view_version(namespace: &[String], view_name: &str) -> ViewVersion {
    let ns_path = namespace.join(".");
    ViewVersion {
        version_id: 1,
        schema_id: 0,
        default_namespace: namespace.to_vec(),
        representations: vec![ViewRepresentation {
            rep_type: "sql".into(),
            sql: format!("SELECT * FROM {ns_path}.{view_name} WHERE col_0 > 0"),
            dialect: "spark".into(),
        }],
    }
}

/// Table 相关请求体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTableRequest {
    pub name: String,
    pub schema: Schema,
    pub location: String,
    pub properties: Properties,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateViewRequest {
    pub name: String,
    pub location: String,
    pub schema: Schema,
    #[serde(rename = "view-version")]
    pub view_version: ViewVersion,
    pub properties: Properties,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateTableRequest {
    pub requirements: Vec<serde_json::Value>,
    pub updates: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitTableRequest {
    pub requirements: Vec<serde_json::Value>,
    pub updates: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNamespaceRequest {
    pub namespace: Vec<String>,
    pub properties: Properties,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateNamespaceRequest {
    pub removals: Vec<String>,
    pub updates: Properties,
}
