//! GET Benchmark — 并发下载随机对象，记录 TTFB。

use crate::bench::{Benchmark, Common, Operation};
use chrono::Utc;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

pub struct GetBenchmark {
    common: Common,
    objects: Mutex<Vec<String>>,
    range: Option<(i64, i64)>,
    list_existing: bool,
}

impl GetBenchmark {
    pub fn new(common: Common, range: Option<(i64, i64)>, list_existing: bool) -> Self {
        Self { common, objects: Mutex::new(Vec::new()), range, list_existing }
    }
}

#[async_trait::async_trait]
impl Benchmark for GetBenchmark {
    async fn prepare(&self, ctx: &CancellationToken) -> crate::generator::Result<()> {
        let client = (self.common.client_factory)(0);

        let _ = client
            .create_bucket()
            .bucket(&self.common.bucket)
            .send()
            .await;

        if self.list_existing {
            let prefix = self.common.prefix();
            let mut keys = Vec::new();
            let mut token: Option<String> = None;
            loop {
                if ctx.is_cancelled() {
                    break;
                }
                let mut req = client
                    .list_objects_v2()
                    .bucket(&self.common.bucket)
                    .prefix(&prefix);
                if let Some(ref t) = token {
                    req = req.continuation_token(t);
                }
                match req.send().await {
                    Ok(resp) => {
                        for o in resp.contents() {
                            if let Some(k) = o.key() {
                                keys.push(k.to_string());
                            }
                        }
                        if !resp.is_truncated().unwrap_or(false) {
                            break;
                        }
                        token = resp
                            .next_continuation_token()
                            .map(|s| s.to_string());
                    }
                    Err(_) => break,
                }
            }
            *self.objects.lock().unwrap() = keys;
            return Ok(());
        }

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
                match crate::bench::body::byte_stream_from_object(obj).await {
                    Ok(body) => {
                        let _ = client
                            .put_object()
                            .bucket(&bucket)
                            .key(&key)
                            .body(body)
                            .send()
                            .await;
                    }
                    Err(_) => return,
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
        let range = self.range;
        let common = self.common.clone();

        let objects = Arc::new(self.objects.lock().unwrap().clone());

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

                    common.throttle_rps().await;

                    let hidx = common.pick_host_index(thread_id);
                    let client = (common.client_factory)(hidx);
                    let endpoint = common.endpoint_for(hidx);

                    let key = objects.choose(&mut rng).cloned().unwrap_or_default();
                    let start = Utc::now();

                    let mut req = client.get_object().bucket(&bucket).key(&key);
                    if let Some((start_b, end_b)) = range {
                        req = req.range(format!("bytes={start_b}-{end_b}"));
                    }
                    let req = common.sse.apply_to_get_request(req);

                    let result = req.send().await;

                    let (err, size, first_byte, end) = match result {
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
                            (String::new(), total, Some(first_byte), end)
                        }
                        Err(e) => (e.to_string(), 0, None, Utc::now()),
                    };

                    common.release_host_index(hidx);

                    let _ = tx.send(Operation {
                        start,
                        end,
                        first_byte,
                        last_byte: None,
                        op_type: "GET".into(),
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

    async fn cleanup(&self, ctx: &CancellationToken) {
        if !self.common.clear { return; }
        let client = (self.common.client_factory)(0);
        let objects = self.objects.lock().unwrap().clone();
        let sem = Arc::new(tokio::sync::Semaphore::new(self.common.concurrency));
        let mut handles = Vec::new();

        for key in objects {
            if ctx.is_cancelled() { break; }
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
        for h in handles { let _ = h.await; }
    }

    fn common(&self) -> &Common {
        &self.common
    }

    fn ops(&self) -> Vec<Operation> {
        self.common.collector.ops()
    }
}
