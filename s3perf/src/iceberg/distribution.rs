//! Iceberg 操作分布 — shuffled pool。

use rand::seq::SliceRandom;
use rand::thread_rng;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

struct DistInner {
    ops: Vec<String>,
    current: usize,
}

#[derive(Clone)]
pub struct IcebergMixedDistribution {
    inner: Arc<Mutex<DistInner>>,
}

impl IcebergMixedDistribution {
    pub fn new(weights: HashMap<String, f64>) -> Result<Self, String> {
        let total: f64 = weights.values().sum();
        if total == 0.0 {
            return Err("distribution total is 0".into());
        }

        let gen_ops = 1000usize;
        let mut pool = Vec::with_capacity(gen_ops);

        for (op, weight) in &weights {
            let count = (0.5 + weight / total * gen_ops as f64) as usize;
            for _ in 0..count {
                pool.push(op.clone());
            }
        }

        pool.shuffle(&mut thread_rng());
        Ok(Self {
            inner: Arc::new(Mutex::new(DistInner {
                ops: pool,
                current: 0,
            })),
        })
    }

    pub fn get_op(&self) -> String {
        let mut inner = self.inner.lock().unwrap();
        if inner.ops.is_empty() {
            return String::new();
        }
        let op = inner.ops[inner.current].clone();
        inner.current = (inner.current + 1) % inner.ops.len();
        op
    }
}
