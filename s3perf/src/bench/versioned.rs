//! Versioned Benchmark — 版本化对象的混合操作压测。
//!
//! 在 versioned-enabled bucket 中上传对象，然后按分布比例执行
//! GET/STAT/PUT/DELETE 操作。PUT 在已有对象上创建新版本。

use crate::bench::{Benchmark, Common, Operation};
use chrono::Utc;
use rand::seq::SliceRandom;
use rand::Rng;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

// ---------------------------------------------------------------------------
// VersionedDistrib — 操作分布权重
// ---------------------------------------------------------------------------
#[derive(Debug, Clone)]
pub struct VersionedDistrib {
    pub get: f64,
    pub stat: f64,
    pub put: f64,
    pub delete: f64,
}

impl VersionedDistrib {
    /// 构建 1000 条操作池，按分布比例填充后 shuffle。
    /// PUT 权重必须 ≤ DELETE 权重，防止对象无限增长。
    pub fn build_pool(&self) -> crate::generator::Result<Vec<&'static str>> {
        if self.put > self.delete {
            return Err(crate::generator::Error::Bench(
                "PUT distribution must be <= DELETE distribution".into(),
            ));
        }

        let total = self.get + self.stat + self.put + self.delete;
        if total <= 0.0 {
            return Err(crate::generator::Error::Bench(
                "total distribution must be positive".into(),
            ));
        }

        let pool_size = 1000;
        let mut pool = Vec::with_capacity(pool_size);

        let get_count = ((self.get / total) * pool_size as f64).round() as usize;
        let stat_count = ((self.stat / total) * pool_size as f64).round() as usize;
        let put_count = ((self.put / total) * pool_size as f64).round() as usize;

        for _ in 0..get_count {
            pool.push("GET");
        }
        for _ in 0..stat_count {
            pool.push("STAT");
        }
        for _ in 0..put_count {
            pool.push("PUT");
        }
        while pool.len() < pool_size {
            pool.push("DELETE");
        }
        pool.truncate(pool_size);

        let mut rng = rand::thread_rng();
        pool.shuffle(&mut rng);

        Ok(pool)
    }
}

impl Default for VersionedDistrib {
    fn default() -> Self {
        Self {
            get: 0.45,
            stat: 0.30,
            put: 0.15,
            delete: 0.10,
        }
    }
}

// ---------------------------------------------------------------------------
// VersionedBenchmark
// ---------------------------------------------------------------------------
pub struct VersionedBenchmark {
    common: Common,
    objects: Mutex<HashMap<String, Vec<String>>>, // name → version_ids
    dist: VersionedDistrib,
}

impl VersionedBenchmark {
    pub fn new(common: Common, dist: VersionedDistrib) -> Self {
        Self {
            common,
            objects: Mutex::new(HashMap::new()),
            dist,
        }
    }
}

#[async_trait::async_trait]
impl Benchmark for VersionedBenchmark {
    /// Prepare: 启用 bucket versioning，上传 objects 个对象，记录 version_id。
    async fn prepare(&self, ctx: &CancellationToken) -> crate::generator::Result<()> {
        let client = (self.common.client_factory)(0);

        // 启用版本控制
        let versioning_cfg = aws_sdk_s3::types::VersioningConfiguration::builder()
            .status(aws_sdk_s3::types::BucketVersioningStatus::Enabled)
            .build();

        client
            .put_bucket_versioning()
            .bucket(&self.common.bucket)
            .versioning_configuration(versioning_cfg)
            .send()
            .await
            .map_err(|e| crate::generator::Error::S3(e.to_string()))?;

        // 并发上传对象，记录 (name, version_id)
        let sem = Arc::new(tokio::sync::Semaphore::new(self.common.concurrency));
        let mut handles = Vec::new();
        let objects: Arc<Mutex<HashMap<String, Vec<String>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        for _ in 0..self.common.objects {
            if ctx.is_cancelled() {
                break;
            }
            let client = client.clone();
            let bucket = self.common.bucket.clone();
            let sem = sem.clone();
            let ctx = ctx.clone();
            let objects = objects.clone();
            let mut source = (self.common.source)();

            handles.push(tokio::spawn(async move {
                let Ok(_permit) = sem.acquire().await else {
                    return;
                };
                if ctx.is_cancelled() {
                    return;
                }
                let obj = source.object();
                let key = obj.name.clone();
                let body = match crate::bench::body::byte_stream_from_object(obj).await {
                    Ok(b) => b,
                    Err(_) => return,
                };

                match client
                    .put_object()
                    .bucket(&bucket)
                    .key(&key)
                    .body(body)
                    .send()
                    .await
                {
                    Ok(output) => {
                        let version_id = output.version_id().unwrap_or_default().to_string();
                        objects
                            .lock()
                            .unwrap()
                            .entry(key)
                            .or_default()
                            .push(version_id);
                    }
                    Err(_) => {}
                }
            }));
        }

        for h in handles {
            let _ = h.await;
        }
        *self.objects.lock().unwrap() = super::take_from_arc_mutex(objects)
            .map_err(|e| crate::generator::Error::Bench(e.to_string()))?;
        Ok(())
    }

    /// Start: 按操作分布百分比混压。concurrency 个 worker 从 1000 条操作池中
    /// round-robin 取操作。GET/STAT 选取随机对象 + 随机版本，PUT 创建新版本，
    /// DELETE 删除指定版本。
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
        let pool = Arc::new(self.dist.build_pool()?);

        // 共享对象池，PUT 新增版本时会写入
        let objects = Arc::new(Mutex::new(self.objects.lock().unwrap().clone()));

        let sem = Arc::new(tokio::sync::Semaphore::new(concurrency));
        let mut handles = Vec::new();
        let common = self.common.clone();

        for thread_id in 0..concurrency {
            let bucket = bucket.clone();
            let sem = sem.clone();
            let ctx = ctx.clone();
            let tx = tx.clone();
            let client_id = client_id.clone();
            let objects = objects.clone();
            let pool = pool.clone();
            let common = common.clone();

            handles.push(tokio::spawn(async move {
                use rand::SeedableRng;
                let mut rng = rand::rngs::SmallRng::from_entropy();
                let deadline = tokio::time::Instant::now() + dur;
                let pool_size = pool.len();
                let mut pool_idx = thread_id;
                let mut source = (common.source)();

                loop {
                    if ctx.is_cancelled() || tokio::time::Instant::now() >= deadline {
                        break;
                    }
                    let Ok(_permit) = sem.acquire().await else {
                        return;
                    };
                    if ctx.is_cancelled() || tokio::time::Instant::now() >= deadline {
                        break;
                    }

                    let selected_op = pool[pool_idx % pool_size];
                    pool_idx += 1;

                    // 从共享 HashMap 随机选 (key, version_id)
                    let (key, version_id) = {
                        let guard = objects.lock().unwrap();
                        if guard.is_empty() {
                            continue;
                        }
                        let keys: Vec<&String> = guard.keys().collect();
                        let k = keys[rng.gen_range(0..keys.len())].clone();
                        let versions = &guard[k.as_str()];
                        let vid = versions[rng.gen_range(0..versions.len())].clone();
                        (k, vid)
                    };

                    common.throttle_rps().await;

                    let hidx = common.pick_host_index(thread_id);
                    let client = (common.client_factory)(hidx);
                    let endpoint = common.endpoint_for(hidx);
                    let start = Utc::now();

                    match selected_op {
                        "GET" => {
                            let result = client
                                .get_object()
                                .bucket(&bucket)
                                .key(&key)
                                .version_id(&version_id)
                                .send()
                                .await;
                            let (err, size, end, first_byte) = match result {
                                Ok(mut r) => {
                                    let first_byte = Utc::now();
                                    let mut total: i64 = r.content_length().unwrap_or(0);
                                    if total == 0 {
                                        while let Some(chunk) = r.body.next().await {
                                            if let Ok(c) = chunk {
                                                total += c.len() as i64;
                                            }
                                        }
                                    } else {
                                        while let Some(chunk) = r.body.next().await {
                                            let _ = chunk;
                                        }
                                    }
                                    let end = Utc::now();
                                    (String::new(), total, end, Some(first_byte))
                                }
                                Err(e) => (e.to_string(), 0, Utc::now(), None),
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
                        "STAT" => {
                            let result = client
                                .head_object()
                                .bucket(&bucket)
                                .key(&key)
                                .version_id(&version_id)
                                .send()
                                .await;
                            let end = Utc::now();
                            let (err, size) = match result {
                                Ok(r) => ("".to_string(), r.content_length().unwrap_or(0)),
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
                        "PUT" => {
                            let payload = source.object();
                            let result =
                                match crate::bench::body::byte_stream_from_object(payload).await {
                                    Ok(body) => {
                                        client
                                            .put_object()
                                            .bucket(&bucket)
                                            .key(&key)
                                            .body(body)
                                            .send()
                                            .await
                                    }
                                    Err(_) => {
                                        common.release_host_index(hidx);
                                        continue;
                                    }
                                };
                            let end = Utc::now();
                            let (err, size, new_vid) = match result {
                                Ok(output) => {
                                    let vid = output.version_id().unwrap_or_default().to_string();
                                    ("".to_string(), 0i64, Some(vid))
                                }
                                Err(e) => (e.to_string(), 0, None),
                            };
                            // 记录新版本
                            if let Some(vid) = new_vid {
                                let mut guard = objects.lock().unwrap();
                                guard.entry(key.clone()).or_default().push(vid);
                            }
                            common.release_host_index(hidx);
                            let _ = tx.send(Operation {
                                start,
                                end,
                                first_byte: None,
                                last_byte: None,
                                op_type: "PUT".into(),
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
                        "DELETE" => {
                            let result = client
                                .delete_object()
                                .bucket(&bucket)
                                .key(&key)
                                .version_id(&version_id)
                                .send()
                                .await;
                            let end = Utc::now();
                            let (err, size) = match result {
                                Ok(_) => ("".to_string(), 0),
                                Err(e) => (e.to_string(), 0),
                            };
                            common.release_host_index(hidx);
                            let _ = tx.send(Operation {
                                start,
                                end,
                                first_byte: None,
                                last_byte: None,
                                op_type: "DELETE".into(),
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
                        _ => {
                            common.release_host_index(hidx);
                            continue;
                        }
                    }
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

    /// Cleanup: 列出所有对象版本并逐一删除。
    async fn cleanup(&self, _ctx: &CancellationToken) {
        let client = (self.common.client_factory)(0);
        let bucket = &self.common.bucket;

        let mut to_delete: Vec<(String, String)> = Vec::new();
        let mut key_marker: Option<String> = None;
        let mut version_id_marker: Option<String> = None;

        // 分页列出所有版本和删除标记
        loop {
            let mut req = client.list_object_versions().bucket(bucket);
            if let Some(ref km) = key_marker {
                req = req.key_marker(km);
            }
            if let Some(ref vm) = version_id_marker {
                req = req.version_id_marker(vm);
            }

            match req.send().await {
                Ok(output) => {
                    for v in output.versions() {
                        if let (Some(k), Some(vid)) = (v.key(), v.version_id()) {
                            to_delete.push((k.to_string(), vid.to_string()));
                        }
                    }
                    for dm in output.delete_markers() {
                        if let (Some(k), Some(vid)) = (dm.key(), dm.version_id()) {
                            to_delete.push((k.to_string(), vid.to_string()));
                        }
                    }

                    if !output.is_truncated().unwrap_or(false) {
                        break;
                    }
                    key_marker = output.next_key_marker().map(|s| s.to_string());
                    version_id_marker = output.next_version_id_marker().map(|s| s.to_string());
                }
                Err(_) => break,
            }
        }

        for (key, version_id) in &to_delete {
            let _ = client
                .delete_object()
                .bucket(bucket)
                .key(key)
                .version_id(version_id)
                .send()
                .await;
        }
    }

    fn common(&self) -> &Common {
        &self.common
    }

    fn ops(&self) -> Vec<Operation> {
        self.common.collector.ops()
    }
}
