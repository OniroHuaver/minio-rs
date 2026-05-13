//! PUT Benchmark — 并发上传随机数据对象。

use crate::bench::checksum::ChecksumType;
use crate::bench::{Benchmark, Common, Operation};
use aws_sdk_s3::primitives::ByteStream;
use base64::Engine;
use chrono::Utc;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub struct PutBenchmark {
    common: Common,
    md5: bool,
    checksum: Option<String>,
    use_post: bool,
}

impl PutBenchmark {
    pub fn new(common: Common, md5: bool, checksum: Option<String>, use_post: bool) -> Self {
        Self {
            common,
            md5,
            checksum,
            use_post,
        }
    }
}

#[async_trait::async_trait]
impl Benchmark for PutBenchmark {
    async fn prepare(&self, ctx: &CancellationToken) -> crate::generator::Result<()> {
        let client = (self.common.client_factory)(0);
        if !ctx.is_cancelled() {
            let _ = client
                .create_bucket()
                .bucket(&self.common.bucket)
                .send()
                .await;
        }
        Ok(())
    }

    async fn start(
        &self,
        ctx: &CancellationToken,
        mut wait: tokio::sync::broadcast::Receiver<()>,
    ) -> crate::generator::Result<()> {
        if self.use_post {
            tracing::warn!("--post requests PostObject which is unimplemented; using PutObject");
        }

        let tx = self.common.collector.sender();
        let _ = wait.recv().await;

        let dur = self.common.duration;
        let concurrency = self.common.concurrency;
        let bucket = self.common.bucket.clone();
        let client_id = format!("client-{}", self.common.client_idx);
        let common = self.common.clone();
        let want_md5 = self.md5
            || matches!(common.checksum, Some(ChecksumType::MD5))
            || self
                .checksum
                .as_deref()
                .map(|s| s.eq_ignore_ascii_case("md5") || s.eq_ignore_ascii_case("md5cs"))
                .unwrap_or(false);

        let sem = Arc::new(tokio::sync::Semaphore::new(concurrency));
        let mut handles = Vec::new();

        for thread_id in 0..concurrency {
            let bucket = bucket.clone();
            let sem = sem.clone();
            let ctx = ctx.clone();
            let tx = tx.clone();
            let client_id = client_id.clone();
            let mut source = (common.source)();
            let common = common.clone();

            handles.push(tokio::spawn(async move {
                let deadline = tokio::time::Instant::now() + dur;

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

                    common.throttle_rps().await;

                    let obj = source.object();
                    let key = obj.name.clone();
                    let size = obj.size;

                    let hidx = common.pick_host_index(thread_id);
                    let client = (common.client_factory)(hidx);
                    let endpoint = common.endpoint_for(hidx);

                    let start = Utc::now();
                    let result = if want_md5 {
                        let bytes = match tokio::task::spawn_blocking(move || {
                            crate::bench::body::read_object_bytes(obj)
                        })
                        .await
                        {
                            Ok(Ok(b)) => b,
                            _ => {
                                common.release_host_index(hidx);
                                continue;
                            }
                        };
                        let digest = md5::compute(&bytes);
                        let content_md5 =
                            base64::engine::general_purpose::STANDARD.encode(digest.0);
                        let body = ByteStream::from(bytes);
                        let req = common.sse.apply_to_put_request(
                            client
                                .put_object()
                                .bucket(&bucket)
                                .key(&key)
                                .body(body)
                                .content_md5(content_md5),
                        );
                        req.send().await
                    } else {
                        let body = match crate::bench::body::byte_stream_from_object(obj).await {
                            Ok(b) => b,
                            Err(_) => {
                                common.release_host_index(hidx);
                                continue;
                            }
                        };
                        let req = common.sse.apply_to_put_request(
                            client.put_object().bucket(&bucket).key(&key).body(body),
                        );
                        req.send().await
                    };
                    let end = Utc::now();

                    common.release_host_index(hidx);

                    let err = match &result {
                        Ok(_) => String::new(),
                        Err(e) => e.to_string(),
                    };

                    let _ = tx.send(Operation {
                        start,
                        end,
                        first_byte: None,
                        last_byte: Some(end),
                        op_type: "PUT".into(),
                        err,
                        file: key,
                        client_id: client_id.clone(),
                        endpoint,
                        obj_per_op: 1,
                        size: if result.is_ok() { size } else { 0 },
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

        for h in handles {
            h.abort();
        }

        Ok(())
    }

    async fn cleanup(&self, ctx: &CancellationToken) {
        if !self.common.clear {
            return;
        }
        let client = (self.common.client_factory)(0);
        if !ctx.is_cancelled() {
            let result = client
                .list_objects_v2()
                .bucket(&self.common.bucket)
                .prefix(self.common.prefix())
                .send()
                .await;
            if let Ok(list) = result {
                if let Some(contents) = list.contents {
                    for obj in contents {
                        if ctx.is_cancelled() {
                            break;
                        }
                        if let Some(key) = obj.key {
                            let _ = client
                                .delete_object()
                                .bucket(&self.common.bucket)
                                .key(&key)
                                .send()
                                .await;
                        }
                    }
                }
            }
        }
    }

    fn common(&self) -> &Common {
        &self.common
    }

    fn ops(&self) -> Vec<Operation> {
        self.common.collector.ops()
    }
}
