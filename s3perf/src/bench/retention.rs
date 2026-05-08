//! Retention Benchmark — 测试 Object Lock PutObjectRetention 操作。

use crate::bench::{Benchmark, Common, Operation};
use aws_sdk_s3::types::{
    BucketVersioningStatus, DefaultRetention, ObjectLockConfiguration,
    ObjectLockEnabled, ObjectLockRetention, ObjectLockRetentionMode,
    ObjectLockRule, VersioningConfiguration,
};
use chrono::Utc;
use rand::Rng;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

pub struct RetentionBenchmark {
    common: Common,
    obj_versions: Mutex<Vec<(String, String)>>,
}

impl RetentionBenchmark {
    pub fn new(common: Common) -> Self {
        Self { common, obj_versions: Mutex::new(Vec::new()) }
    }
}

#[async_trait::async_trait]
impl Benchmark for RetentionBenchmark {
    async fn prepare(&self, ctx: &CancellationToken) -> crate::generator::Result<()> {
        let client = (self.common.client_factory)(0);

        let _ = client
            .create_bucket()
            .bucket(&self.common.bucket)
            .object_lock_enabled_for_bucket(true)
            .send()
            .await;

        let lock_cfg = ObjectLockConfiguration::builder()
            .object_lock_enabled(ObjectLockEnabled::Enabled)
            .rule(
                ObjectLockRule::builder()
                    .default_retention(
                        DefaultRetention::builder()
                            .mode(ObjectLockRetentionMode::Governance)
                            .days(1)
                            .build(),
                    )
                    .build(),
            )
            .build();

        let _ = client
            .put_object_lock_configuration()
            .bucket(&self.common.bucket)
            .object_lock_configuration(lock_cfg)
            .send()
            .await;

        let versioning_cfg = VersioningConfiguration::builder()
            .status(BucketVersioningStatus::Enabled)
            .build();

        let _ = client
            .put_bucket_versioning()
            .bucket(&self.common.bucket)
            .versioning_configuration(versioning_cfg)
            .send()
            .await;

        let sem = Arc::new(tokio::sync::Semaphore::new(self.common.concurrency));
        let mut handles = Vec::new();
        let obj_versions = Arc::new(Mutex::new(Vec::new()));
        let tx = self.common.collector.sender();
        let host = self.common.hosts.first().cloned().unwrap_or_default();
        let client_id = format!("client-{}", self.common.client_idx);

        for _i in 0..self.common.objects {
            if ctx.is_cancelled() { break; }
            let client = client.clone();
            let bucket = self.common.bucket.clone();
            let sem = sem.clone();
            let ctx = ctx.clone();
            let obj_versions = obj_versions.clone();
            let tx = tx.clone();
            let mut source = (self.common.source)();
            let host = host.clone();
            let client_id = client_id.clone();
            let versions = self.common.versions;

            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                if ctx.is_cancelled() { return; }
                let mut first_obj = Some(source.object());
                let key = first_obj.as_ref().unwrap().name.clone();

                for v in 0..versions {
                    if ctx.is_cancelled() { break; }
                    let body_obj = if v == 0 {
                        first_obj.take().expect("first version")
                    } else {
                        source.object()
                    };
                    let body = match crate::bench::body::byte_stream_from_object(body_obj).await {
                        Ok(b) => b,
                        Err(_) => continue,
                    };
                    let start = Utc::now();
                    let result = client
                        .put_object().bucket(&bucket).key(&key).body(body).send().await;
                    let end = Utc::now();

                    let (err, version_id) = match &result {
                        Ok(resp) => (String::new(), resp.version_id().unwrap_or_default().to_string()),
                        Err(e) => (e.to_string(), String::new()),
                    };

                    if !version_id.is_empty() {
                        obj_versions.lock().unwrap().push((key.clone(), version_id));
                    }

                    let _ = tx.send(Operation {
                        start, end,
                        first_byte: None, last_byte: Some(end),
                        op_type: "PUT".into(), err,
                        file: key.clone(),
                        client_id: client_id.clone(),
                        endpoint: host.clone(),
                        obj_per_op: 1, size: 0, thread: 0, categories: 0,
                    });
                }
            }));
        }

        for h in handles { let _ = h.await; }
        *self.obj_versions.lock().unwrap() =
            Arc::try_unwrap(obj_versions).unwrap().into_inner().unwrap();
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

        let obj_versions: Vec<(String, String)> = self.obj_versions.lock().unwrap().clone();
        let shared_versions = Arc::new(obj_versions);
        let sem = Arc::new(tokio::sync::Semaphore::new(concurrency));
        let mut handles = Vec::new();
        let common = self.common.clone();

        for thread_id in 0..concurrency {
            let bucket = bucket.clone();
            let sem = sem.clone();
            let ctx = ctx.clone();
            let tx = tx.clone();
            let client_id = client_id.clone();
            let shared_versions = shared_versions.clone();
            let common = common.clone();

            handles.push(tokio::spawn(async move {
                use rand::SeedableRng;
                let mut rng = rand::rngs::SmallRng::from_entropy();
                let deadline = tokio::time::Instant::now() + dur;

                loop {
                    if ctx.is_cancelled() || tokio::time::Instant::now() >= deadline { break; }
                    let _permit = sem.acquire().await.unwrap();
                    if ctx.is_cancelled() || tokio::time::Instant::now() >= deadline { break; }

                    if shared_versions.is_empty() {
                        continue;
                    }
                    let idx = rng.gen_range(0..shared_versions.len());
                    let (ref name, ref version_id) = shared_versions[idx];
                    if name.is_empty() { continue; }

                    common.throttle_rps().await;

                    let hidx = common.pick_host_index(thread_id);
                    let client = (common.client_factory)(hidx);
                    let endpoint = common.endpoint_for(hidx);

                    let retain_until = Utc::now() + chrono::Duration::hours(24);
                    let retention = ObjectLockRetention::builder()
                        .mode(ObjectLockRetentionMode::Governance)
                        .retain_until_date(
                            aws_sdk_s3::primitives::DateTime::from_secs(retain_until.timestamp()),
                        )
                        .build();

                    let start = Utc::now();
                    let result = client
                        .put_object_retention()
                        .bucket(&bucket)
                        .key(name)
                        .version_id(version_id)
                        .retention(retention)
                        .bypass_governance_retention(true)
                        .send()
                        .await;
                    let end = Utc::now();

                    let err = match &result {
                        Ok(_) => String::new(),
                        Err(e) => e.to_string(),
                    };

                    common.release_host_index(hidx);

                    let _ = tx.send(Operation {
                        start, end,
                        first_byte: None, last_byte: None,
                        op_type: "RETENTION".into(), err,
                        file: name.clone(),
                        client_id: client_id.clone(),
                        endpoint,
                        obj_per_op: 1, size: 0,
                        thread: thread_id as u32, categories: 0,
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
        let obj_versions = self.obj_versions.lock().unwrap().clone();
        let sem = Arc::new(tokio::sync::Semaphore::new(self.common.concurrency));
        let mut handles = Vec::new();

        for (name, version_id) in &obj_versions {
            if ctx.is_cancelled() { break; }
            let client = client.clone();
            let bucket = self.common.bucket.clone();
            let sem = sem.clone();
            let ctx = ctx.clone();
            let name = name.clone();
            let version_id = version_id.clone();

            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                if !ctx.is_cancelled() {
                    let _ = client.delete_object()
                        .bucket(&bucket).key(&name).version_id(&version_id)
                        .send().await;
                }
            }));
        }
        for h in handles { let _ = h.await; }
    }

    fn common(&self) -> &Common { &self.common }

    fn ops(&self) -> Vec<Operation> { self.common.collector.ops() }
}
