//! LIST Benchmark — 并发 ListObjects 操作。

use crate::bench::{Benchmark, Common, Operation};
use chrono::Utc;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

pub struct ListBenchmark {
    common: Common,
    prefixes: Mutex<Vec<String>>,
    versions: bool,
}

impl ListBenchmark {
    pub fn new(common: Common, versions: bool) -> Self {
        Self { common, prefixes: Mutex::new(Vec::new()), versions }
    }
}

#[async_trait::async_trait]
impl Benchmark for ListBenchmark {
    async fn prepare(&self, ctx: &CancellationToken) -> crate::generator::Result<()> {
        let client = (self.common.client_factory)(0);
        let _ = client.create_bucket().bucket(&self.common.bucket).send().await;

        let n_prefixes = self.common.concurrency;
        let objs_per_prefix = self.common.objects / n_prefixes.max(1);
        let sem = Arc::new(tokio::sync::Semaphore::new(self.common.concurrency));
        let mut handles = Vec::new();
        let prefixes = Arc::new(Mutex::new(Vec::new()));

        for p in 0..n_prefixes {
            if ctx.is_cancelled() { break; }
            let prefix = format!("{}/dir-{:04x}/", self.common.prefix(), p);
            prefixes.lock().unwrap().push(prefix.clone());

            for _j in 0..objs_per_prefix {
                let client = client.clone();
                let bucket = self.common.bucket.clone();
                let sem = sem.clone();
                let ctx = ctx.clone();
                let prefix = prefix.clone();
                let mut source = (self.common.source)();

                handles.push(tokio::spawn(async move {
                    let Ok(_permit) = sem.acquire().await else { return; };
                    if ctx.is_cancelled() { return; }
                    let obj = source.object();
                    let key = format!("{prefix}{}", obj.name);
                    if let Ok(body) = crate::bench::body::byte_stream_from_object(obj).await {
                        let _ = client.put_object().bucket(&bucket).key(&key).body(body).send().await;
                    }
                }));
            }
        }

        for h in handles { let _ = h.await; }
        *self.prefixes.lock().unwrap() = super::take_from_arc_mutex(prefixes)
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
        let prefixes = self.prefixes.lock().unwrap().clone();
        let versions = self.versions;
        let common = self.common.clone();

        let sem = Arc::new(tokio::sync::Semaphore::new(concurrency));
        let mut handles = Vec::new();

        for thread_id in 0..concurrency {
            let bucket = bucket.clone();
            let sem = sem.clone();
            let ctx = ctx.clone();
            let tx = tx.clone();
            let client_id = client_id.clone();
            let prefixes = prefixes.clone();
            let common = common.clone();

            handles.push(tokio::spawn(async move {
                let deadline = tokio::time::Instant::now() + dur;
                let mut prefix_idx = 0usize;

                loop {
                    if ctx.is_cancelled() || tokio::time::Instant::now() >= deadline {
                        break;
                    }
                    let Ok(_permit) = sem.acquire().await else { return; };
                    if ctx.is_cancelled() || tokio::time::Instant::now() >= deadline {
                        break;
                    }

                    let prefix = &prefixes[prefix_idx % prefixes.len()];
                    prefix_idx += 1;

                    common.throttle_rps().await;

                    let hidx = common.pick_host_index(thread_id);
                    let client = (common.client_factory)(hidx);
                    let endpoint = common.endpoint_for(hidx);

                    let start = Utc::now();
                    let (err, obj_count) = if versions {
                        match client
                            .list_object_versions()
                            .bucket(&bucket)
                            .set_prefix(Some(prefix.clone()))
                            .send()
                            .await
                        {
                            Ok(resp) => ("".to_string(), resp.versions().len() as i64),
                            Err(e) => (e.to_string(), 0),
                        }
                    } else {
                        match client
                            .list_objects_v2()
                            .bucket(&bucket)
                            .set_prefix(Some(prefix.clone()))
                            .send()
                            .await
                        {
                            Ok(resp) => ("".to_string(), resp.contents().len() as i64),
                            Err(e) => (e.to_string(), 0),
                        }
                    };
                    let end = Utc::now();

                    common.release_host_index(hidx);

                    let _ = tx.send(Operation {
                        start,
                        end,
                        first_byte: None,
                        last_byte: None,
                        op_type: "LIST".into(),
                        err,
                        file: prefix.clone(),
                        client_id: client_id.clone(),
                        endpoint,
                        obj_per_op: 1,
                        size: obj_count,
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
