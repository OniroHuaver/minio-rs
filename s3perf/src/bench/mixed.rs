//! Mixed Benchmark — GET/STAT/PUT/DELETE 按分布比例混合执行。

use crate::bench::{Benchmark, Common, Operation};
use chrono::Utc;
use rand::Rng;
use rand::seq::SliceRandom;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub struct MixedDistrib {
    pub get: f64,
    pub stat: f64,
    pub put: f64,
    pub delete: f64,
}

impl MixedDistrib {
    /// PUT 权重需 ≥ DELETE，避免读多写少时对象被删光后大量失败。
    pub fn validate(&self) -> crate::generator::Result<()> {
        if self.put < self.delete {
            return Err(crate::generator::Error::Bench(
                "mixed benchmark: PUT distribution must be >= DELETE distribution".into(),
            ));
        }
        let total = self.total();
        if total <= 0.0 {
            return Err(crate::generator::Error::Bench(
                "mixed benchmark: total distribution must be positive".into(),
            ));
        }
        Ok(())
    }

    pub fn select_op(&self, rng: &mut impl Rng) -> &str {
        let mut v: f64 = rng.gen();
        v -= self.get;
        if v <= 0.0 { return "GET"; }
        v -= self.stat;
        if v <= 0.0 { return "STAT"; }
        v -= self.put;
        if v <= 0.0 { return "PUT"; }
        v -= self.delete;
        if v <= 0.0 { return "DELETE"; }
        "GET" // fallback
    }

    pub fn total(&self) -> f64 {
        self.get + self.stat + self.put + self.delete
    }
}

impl Default for MixedDistrib {
    fn default() -> Self {
        Self { get: 0.45, stat: 0.05, put: 0.25, delete: 0.25 }
    }
}

pub struct MixedBenchmark {
    common: Common,
    distrib: MixedDistrib,
    objects: Mutex<Vec<String>>,
}

impl MixedBenchmark {
    pub fn new(common: Common, distrib: MixedDistrib) -> Self {
        Self { common, distrib, objects: Mutex::new(Vec::new()) }
    }
}

#[async_trait::async_trait]
impl Benchmark for MixedBenchmark {
    async fn prepare(&self, ctx: &CancellationToken) -> crate::generator::Result<()> {
        self.distrib.validate()?;
        let client = (self.common.client_factory)(0);
        let _ = client.create_bucket().bucket(&self.common.bucket).send().await;

        let sem = Arc::new(tokio::sync::Semaphore::new(self.common.concurrency));
        let mut handles = Vec::new();
        let keys = Arc::new(Mutex::new(Vec::new()));

        for _i in 0..self.common.objects {
            if ctx.is_cancelled() { break; }
            let client = client.clone();
            let bucket = self.common.bucket.clone();
            let sem = sem.clone();
            let ctx = ctx.clone();
            let keys = keys.clone();
            let mut source = (self.common.source)();

            handles.push(tokio::spawn(async move {
                let Ok(_permit) = sem.acquire().await else { return; };
                if ctx.is_cancelled() { return; }
                let obj = source.object();
                let key = obj.name.clone();
                if let Ok(body) = crate::bench::body::byte_stream_from_object(obj).await {
                    let _ = client.put_object().bucket(&bucket).key(&key).body(body).send().await;
                }
                keys.lock().unwrap().push(key);
            }));
        }
        for h in handles { let _ = h.await; }
        *self.objects.lock().unwrap() = super::take_from_arc_mutex(keys)
            .map_err(|e| crate::generator::Error::Bench(e.to_string()))?;
        Ok(())
    }

    async fn start(
        &self,
        ctx: &CancellationToken,
        mut wait: tokio::sync::broadcast::Receiver<()>,
    ) -> crate::generator::Result<()> {
        let tx = self.common.collector.sender();
        let _ = wait.recv().await;

        let dur = self.common.duration;
        let concurrency = self.common.concurrency;
        let bucket = self.common.bucket.clone();
        let client_id = format!("client-{}", self.common.client_idx);
        let objects = self.objects.lock().unwrap().clone();
        let distrib = self.distrib.clone();
        let common = self.common.clone();

        let sem = Arc::new(tokio::sync::Semaphore::new(concurrency));
        let mut handles = Vec::new();

        for thread_id in 0..concurrency {
            let bucket = bucket.clone();
            let sem = sem.clone();
            let ctx = ctx.clone();
            let tx = tx.clone();
            let client_id = client_id.clone();
            let objects = objects.clone();
            let distrib = distrib.clone();
            let common = common.clone();

            handles.push(tokio::spawn(async move {
                use rand::SeedableRng;
                let mut rng = rand::rngs::SmallRng::from_entropy();
                let deadline = tokio::time::Instant::now() + dur;
                let mut source = (common.source)();

                loop {
                    if ctx.is_cancelled() || tokio::time::Instant::now() >= deadline {
                        break;
                    }
                    let Ok(_permit) = sem.acquire().await else { return; };
                    if ctx.is_cancelled() || tokio::time::Instant::now() >= deadline {
                        break;
                    }

                    let selected_op = distrib.select_op(&mut rng);

                    common.throttle_rps().await;

                    let hidx = common.pick_host_index(thread_id);
                    let client = (common.client_factory)(hidx);
                    let endpoint = common.endpoint_for(hidx);

                    let start = Utc::now();

                    let end;
                    let result_op_type: &str;
                    let file;
                    let err;
                    let size;
                    let first_byte;

                    match selected_op {
                        "GET" => {
                            let key = objects.choose(&mut rng).cloned().unwrap_or_default();
                            let res = client.get_object().bucket(&bucket).key(&key).send().await;
                            result_op_type = "GET";
                            file = key;
                            (err, size, end, first_byte) = match res {
                                Ok(mut resp) => {
                                    let first_byte = Utc::now();
                                    let mut total: i64 = resp.content_length().unwrap_or(0);
                                    if total == 0 {
                                        while let Some(chunk) = resp.body.next().await {
                                            if let Ok(c) = chunk {
                                                total += c.len() as i64;
                                            }
                                        }
                                    } else {
                                        while let Some(chunk) = resp.body.next().await {
                                            let _ = chunk;
                                        }
                                    }
                                    let end = Utc::now();
                                    (String::new(), total, end, Some(first_byte))
                                }
                                Err(e) => (e.to_string(), 0, Utc::now(), None),
                            };
                        }
                        "STAT" => {
                            let key = objects.choose(&mut rng).cloned().unwrap_or_default();
                            let res = client.head_object().bucket(&bucket).key(&key).send().await;
                            end = Utc::now();
                            result_op_type = "STAT";
                            file = key;
                            first_byte = None;
                            (err, size) = match res {
                                Ok(r) => ("".to_string(), r.content_length().unwrap_or(0)),
                                Err(e) => (e.to_string(), 0),
                            };
                        }
                        "PUT" => {
                            let obj = source.object();
                            let key = obj.name.clone();
                            result_op_type = "PUT";
                            file = key.clone();
                            first_byte = None;
                            match crate::bench::body::byte_stream_from_object(obj).await {
                                Ok(body) => {
                                    let res = client
                                        .put_object()
                                        .bucket(&bucket)
                                        .key(&key)
                                        .body(body)
                                        .send()
                                        .await;
                                    end = Utc::now();
                                    (err, size) = match res {
                                        Ok(_) => ("".to_string(), 0i64),
                                        Err(e) => (e.to_string(), 0),
                                    };
                                }
                                Err(e) => {
                                    end = Utc::now();
                                    err = e.to_string();
                                    size = 0;
                                }
                            }
                        }
                        "DELETE" => {
                            let key = objects.choose(&mut rng).cloned().unwrap_or_default();
                            let res = client.delete_object().bucket(&bucket).key(&key).send().await;
                            end = Utc::now();
                            result_op_type = "DELETE";
                            file = key;
                            first_byte = None;
                            (err, size) = match res {
                                Ok(_) => ("".to_string(), 0i64),
                                Err(e) => (e.to_string(), 0),
                            };
                        }
                        _ => {
                            common.release_host_index(hidx);
                            continue;
                        }
                    };

                    common.release_host_index(hidx);

                    let _ = tx.send(Operation {
                        start,
                        end,
                        first_byte,
                        last_byte: None,
                        op_type: result_op_type.to_string(),
                        err,
                        file,
                        client_id: client_id.clone(),
                        endpoint,
                        obj_per_op: 1,
                        size,
                        thread: thread_id as u32,
                        categories: 0,
                    });
                }
            }));
        }

        tokio::select! {
            _ = ctx.cancelled() => {}
            _ = tokio::time::sleep(dur) => {}
        }
        for h in handles { h.abort(); }
        Ok(())
    }

    async fn cleanup(&self, ctx: &CancellationToken) {
        if !self.common.clear {
            return;
        }
        let client = (self.common.client_factory)(0);
        let objects = self.objects.lock().unwrap().clone();
        let sem = Arc::new(tokio::sync::Semaphore::new(self.common.concurrency));
        let mut handles = Vec::new();
        for key in objects {
            if ctx.is_cancelled() {
                break;
            }
            let client = client.clone();
            let bucket = self.common.bucket.clone();
            let sem = sem.clone();
            let ctx = ctx.clone();
            handles.push(tokio::spawn(async move {
                let Ok(_permit) = sem.acquire().await else { return; };
                if !ctx.is_cancelled() {
                    let _ = client.delete_object().bucket(&bucket).key(&key).send().await;
                }
            }));
        }
        for h in handles {
            let _ = h.await;
        }
    }

    fn common(&self) -> &Common {
        &self.common
    }

    fn ops(&self) -> Vec<Operation> {
        self.common.collector.ops()
    }
}
