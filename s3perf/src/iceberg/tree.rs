//! Iceberg N叉命名空间树构建。

use rand::Rng;

#[derive(Debug, Clone)]
pub struct TreeConfig {
    pub namespace_width: usize,
    pub namespace_depth: usize,
    pub tables_per_ns: usize,
    pub views_per_ns: usize,
    pub columns: usize,
    pub properties: usize,
    pub base_location: String,
    pub catalog_name: String,
}

impl Default for TreeConfig {
    fn default() -> Self {
        Self {
            namespace_width: 2,
            namespace_depth: 3,
            tables_per_ns: 5,
            views_per_ns: 5,
            columns: 10,
            properties: 5,
            base_location: "s3://benchmark".into(),
            catalog_name: "benchmarkcatalog".into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct NamespaceInfo {
    pub path: Vec<String>,
    pub ordinal: usize,
}

#[derive(Debug, Clone)]
pub struct TableInfo {
    pub namespace: Vec<String>,
    pub name: String,
    pub location: String,
}

#[derive(Debug, Clone)]
pub struct ViewInfo {
    pub namespace: Vec<String>,
    pub name: String,
    pub location: String,
}

pub struct Tree {
    cfg: TreeConfig,
}

impl Tree {
    pub fn new(cfg: TreeConfig) -> Self {
        Self { cfg }
    }

    pub fn config(&self) -> &TreeConfig {
        &self.cfg
    }

    pub fn total_namespaces(&self) -> usize {
        if self.cfg.namespace_width == 1 {
            return self.cfg.namespace_depth;
        }
        let w = self.cfg.namespace_width;
        let d = self.cfg.namespace_depth;
        (w.pow(d as u32) - 1) / (w - 1)
    }

    pub fn leaf_namespaces(&self) -> usize {
        self.cfg
            .namespace_width
            .pow((self.cfg.namespace_depth - 1) as u32)
    }

    pub fn total_tables(&self) -> usize {
        self.leaf_namespaces() * self.cfg.tables_per_ns
    }

    pub fn total_views(&self) -> usize {
        self.leaf_namespaces() * self.cfg.views_per_ns
    }

    pub fn all_namespaces(&self) -> Vec<NamespaceInfo> {
        let mut out = Vec::with_capacity(self.total_namespaces());
        let w = self.cfg.namespace_width;
        let depth = self.cfg.namespace_depth;

        let mut ordinal = 0;
        for d in 0..depth {
            let count = w.pow(d as u32);
            for i in 0..count {
                let mut path = Vec::with_capacity(d + 1);
                for level in 0..=d {
                    let _level_size = w.pow(level as u32);
                    let idx = if level == d {
                        i
                    } else {
                        i / w.pow((d - level) as u32) % w.pow(level as u32)
                            + if level > 0 {
                                w.pow(level as u32 - 1) * (w - 1)
                            } else {
                                0
                            }
                    };
                    path.push(format!("ns_{}", idx));
                }
                out.push(NamespaceInfo { path, ordinal });
                ordinal += 1;
            }
        }
        out
    }

    pub fn all_tables(&self) -> Vec<TableInfo> {
        let leaves = self
            .all_namespaces()
            .into_iter()
            .filter(|ns| ns.path.len() == self.cfg.namespace_depth)
            .collect::<Vec<_>>();

        let mut out =
            Vec::with_capacity(leaves.len() * self.cfg.tables_per_ns);
        for ns in &leaves {
            for t in 0..self.cfg.tables_per_ns {
                let name = format!("tbl_{}", t);
                let loc = format!(
                    "{}/{}/{name}",
                    self.cfg.base_location,
                    ns.path.join("/")
                );
                out.push(TableInfo {
                    namespace: ns.path.clone(),
                    name,
                    location: loc,
                });
            }
        }
        out
    }

    pub fn all_views(&self) -> Vec<ViewInfo> {
        let leaves = self
            .all_namespaces()
            .into_iter()
            .filter(|ns| ns.path.len() == self.cfg.namespace_depth)
            .collect::<Vec<_>>();

        let mut out =
            Vec::with_capacity(leaves.len() * self.cfg.views_per_ns);
        for ns in &leaves {
            for v in 0..self.cfg.views_per_ns {
                let name = format!("view_{}", v);
                let loc = format!(
                    "{}/{}/{name}",
                    self.cfg.base_location,
                    ns.path.join("/")
                );
                out.push(ViewInfo {
                    namespace: ns.path.clone(),
                    name,
                    location: loc,
                });
            }
        }
        out
    }

    pub fn leaf_namespaces_list(&self) -> Vec<NamespaceInfo> {
        self.all_namespaces()
            .into_iter()
            .filter(|ns| ns.path.len() == self.cfg.namespace_depth)
            .collect()
    }

    pub fn random_namespace<R: Rng>(
        &self,
        rng: &mut R,
    ) -> NamespaceInfo {
        let all = self.all_namespaces();
        all[rng.gen_range(0..all.len())].clone()
    }

    pub fn random_table<R: Rng>(&self, rng: &mut R) -> TableInfo {
        let all = self.all_tables();
        all[rng.gen_range(0..all.len())].clone()
    }

    pub fn random_view<R: Rng>(&self, rng: &mut R) -> ViewInfo {
        let all = self.all_views();
        all[rng.gen_range(0..all.len())].clone()
    }
}

/// 将 namespace path 转为 Iceberg REST identifier
pub fn namespace_path(parent: &[String], name: &str) -> Vec<String> {
    let mut p = parent.to_vec();
    p.push(name.to_string());
    p
}

pub fn to_table_identifier(
    namespace: &[String],
    name: &str,
) -> Vec<String> {
    let mut p = namespace.to_vec();
    p.push(name.to_string());
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tree_counts() {
        let cfg = TreeConfig {
            namespace_width: 2,
            namespace_depth: 3,
            ..Default::default()
        };
        let tree = Tree::new(cfg);
        assert_eq!(tree.total_namespaces(), 7); // 1+2+4
        assert_eq!(tree.leaf_namespaces(), 4);
        assert_eq!(tree.total_tables(), 20); // 4*5
        assert_eq!(tree.total_views(), 20);
    }

    #[test]
    fn test_tree_namespace_list() {
        let cfg = TreeConfig {
            namespace_width: 1,
            namespace_depth: 2,
            ..Default::default()
        };
        let tree = Tree::new(cfg);
        let all = tree.all_namespaces();
        assert_eq!(all.len(), 2); // depth=2, width=1 → 2 ns
    }
}
