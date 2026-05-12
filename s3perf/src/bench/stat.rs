//! STAT Benchmark — 并发 StatObject (HeadObject) 操作。

use crate::bench::{Benchmark, Common, Operation};
use chrono::Utc;
use rand::seq::SliceRandom;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

pub struct StatBenchmark {
    common: Common,
    objects: Mutex<Vec<String>>,
}

impl StatBenchmark {
    pub fn new(common: Common) -> Self {
        Self { common, objects: Mutex::new(Vec::new()) }
    }
}

#[async_trait::async_trait]
impl Benchmark for StatBenchmark {
    async fn prepare(&self, ctx: &CancellationToken) -> crate::generator::Result<()> {
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
            let common = common.clone();

            handles.push(tokio::spawn(async move {
                use rand::SeedableRng;
                let mut rng = rand::rngs::SmallRng::from_entropy();
                let deadline = tokio::time::Instant::now() + dur;

                loop {
                    if ctx.is_cancelled() || tokio::time::Instant::now() >= deadline {
                        break;
                    }
                    let Ok(_permit) = sem.acquire().await else { return; };
                    if ctx.is_cancelled() || tokio::time::Instant::now() >= deadline {
                        break;
                    }

                    let key = objects.choose(&mut rng).cloned().unwrap_or_default();

                    common.throttle_rps().await;

                    let hidx = common.pick_host_index(thread_id);
                    let client = (common.client_factory)(hidx);
                    let endpoint = common.endpoint_for(hidx);

                    let start = Utc::now();
                    let result = client.head_object().bucket(&bucket).key(&key).send().await;
                    let end = Utc::now();

                    let (err, size) = match &result {
                        Ok(resp) => ("".to_string(), resp.content_length().unwrap_or(0)),
                        Err(e) => (e.to_string(), 0),
                    };

                    common.release_host_index(hidx);

                    let _ = tx.send(Operation {
                        start,
                        end,
                        first_byte: None,
                        last_byte: None,
                        op_type: "STAT".into(),
                        err,
                        file: key,
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

    async fn cleanup(&self, _ctx: &CancellationToken) {}

    fn common(&self) -> &Common {
        &self.common
    }

    fn ops(&self) -> Vec<Operation> {
        self.common.collector.ops()
    }
}
