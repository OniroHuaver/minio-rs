//! DELETE Benchmark — 批量删除对象。

use crate::bench::{Benchmark, Common, Operation};
use chrono::Utc;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

pub struct DeleteBenchmark {
    common: Common,
    objects: Mutex<Vec<String>>,
    batch_size: usize,
}

impl DeleteBenchmark {
    pub fn new(common: Common, batch_size: usize) -> Self {
        Self { common, objects: Mutex::new(Vec::new()), batch_size }
    }
}

#[async_trait::async_trait]
impl Benchmark for DeleteBenchmark {
    async fn prepare(&self, ctx: &CancellationToken) -> crate::generator::Result<()> {
        let client = (self.common.client_factory)(0);

        let _ = client
            .create_bucket()
            .bucket(&self.common.bucket)
            .send()
            .await;

        // 上传对象
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

        for h in handles {
            let _ = h.await;
        }

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
        let chunk_sz = self.batch_size.max(1);
        let common = self.common.clone();

        let sem = Arc::new(tokio::sync::Semaphore::new(concurrency));
        let mut handles = Vec::new();
        let objects = self.objects.lock().unwrap().clone();
        let obj_arc = Arc::new(objects);

        for thread_id in 0..concurrency {
            let bucket = bucket.clone();
            let sem = sem.clone();
            let ctx = ctx.clone();
            let tx = tx.clone();
            let client_id = client_id.clone();
            let obj_arc = obj_arc.clone();
            let chunk_sz = chunk_sz;
            let common = common.clone();

            handles.push(tokio::spawn(async move {
                let deadline = tokio::time::Instant::now() + dur;
                let mut offset = thread_id * chunk_sz;

                loop {
                    if ctx.is_cancelled() || tokio::time::Instant::now() >= deadline {
                        break;
                    }
                    if offset >= obj_arc.len() {
                        break; // 对象删完就停
                    }

                    let Ok(_permit) = sem.acquire().await else { return; };
                    if ctx.is_cancelled() {
                        break;
                    }

                    let batch: Vec<String> = obj_arc
                        .iter()
                        .skip(offset)
                        .take(chunk_sz.min(1000)) // S3 限制最多 1000 个对象/次
                        .cloned()
                        .collect();

                    if batch.is_empty() {
                        break;
                    }

                    common.throttle_rps().await;

                    let hidx = common.pick_host_index(thread_id);
                    let client = (common.client_factory)(hidx);
                    let endpoint = common.endpoint_for(hidx);

                    let start = Utc::now();
                    let objects = batch
                        .iter()
                        .map(|k| {
                            aws_sdk_s3::types::ObjectIdentifier::builder()
                                .key(k)
                                .build()
                                .unwrap()
                        })
                        .collect::<Vec<_>>();

                    let n_objects = objects.len();
                    let delete = aws_sdk_s3::types::Delete::builder()
                        .set_objects(Some(objects))
                        .build()
                        .unwrap();

                    let result = client
                        .delete_objects()
                        .bucket(&bucket)
                        .delete(delete)
                        .send()
                        .await;

                    let end = Utc::now();
                    let err = match &result {
                        Ok(_) => String::new(),
                        Err(e) => e.to_string(),
                    };

                    common.release_host_index(hidx);

                    let _ = tx.send(Operation {
                        start,
                        end,
                        first_byte: None,
                        last_byte: None,
                        op_type: "DELETE".into(),
                        err,
                        file: format!("batch:{n_objects}"),
                        client_id: client_id.clone(),
                        endpoint,
                        obj_per_op: n_objects as u32,
                        size: 0,
                        thread: thread_id as u32,
                        categories: 0,
                    });

                    offset += chunk_sz;
                }
            }));
        }

        tokio::select! {
            _ = ctx.cancelled() => {}
            _ = tokio::time::sleep(dur) => {}
        }

        for h in handles {
            h.abort();
        }

        Ok(())
    }

    async fn cleanup(&self, _ctx: &CancellationToken) {
        // DELETE benchmark 已经删除了大部分对象
        // 这里清理残留
    }

    fn common(&self) -> &Common {
        &self.common
    }

    fn ops(&self) -> Vec<Operation> {
        self.common.collector.ops()
    }
}
