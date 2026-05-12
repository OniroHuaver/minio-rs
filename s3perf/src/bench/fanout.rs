//! Fanout Benchmark — 模拟 MinIO 专有 PutObjectFanOut API。
//!
//! 流程: 每轮生成 1 个随机对象，扇出到 copies 个目标 key。
//! 模拟方案：对 copies 个目标 key 使用同一数据并发执行 PutObject。

use crate::bench::body::read_object_bytes;
use crate::bench::{Benchmark, Common, Operation};
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

pub struct FanoutBenchmark {
    common: Common,
    copies: usize,
    obj_size: usize,
}

impl FanoutBenchmark {
    pub fn new(common: Common, copies: usize, obj_size: usize) -> Self {
        Self {
            common,
            copies,
            obj_size,
        }
    }
}

#[async_trait::async_trait]
impl Benchmark for FanoutBenchmark {
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
        let tx = self.common.collector.sender();
        let _ = wait.recv().await;

        let dur = self.common.duration;
        let concurrency = self.common.concurrency;
        let bucket = self.common.bucket.clone();
        let client_id = format!("client-{}", self.common.client_idx);
        let copies = self.copies;
        let common = self.common.clone();

        let sem = Arc::new(Semaphore::new(concurrency));
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

                    let Ok(_permit) = sem.acquire().await else { return; };
                    if ctx.is_cancelled() || tokio::time::Instant::now() >= deadline {
                        break;
                    }

                    // 获取源对象（提供 key 前缀和逻辑大小），整段 payload 只读一次
                    let obj = source.object();
                    let size = obj.size;
                    let base_name = obj.name.clone();
                    let bytes = match tokio::task::spawn_blocking(move || read_object_bytes(obj))
                        .await
                    {
                        Ok(Ok(b)) => b,
                        _ => continue,
                    };

                    common.throttle_rps().await;

                    let hidx = common.pick_host_index(thread_id);
                    let client = (common.client_factory)(hidx);
                    let endpoint = common.endpoint_for(hidx);

                    // 扇出写入：copies 个目标 key 使用同一数据，并发执行
                    let start = Utc::now();
                    let mut fanout_handles = Vec::with_capacity(copies);

                    for i in 0..copies {
                        let client = client.clone();
                        let bucket = bucket.clone();
                        let key = format!("{}/copy-{:04}", base_name, i);
                        let body = aws_sdk_s3::primitives::ByteStream::from(bytes.clone());

                        fanout_handles.push(tokio::spawn(async move {
                            client
                                .put_object()
                                .bucket(&bucket)
                                .key(&key)
                                .body(body)
                                .send()
                                .await
                        }));
                    }

                    // 等待所有扇出副本完成
                    let mut ok_count = 0usize;
                    for h in fanout_handles {
                        if let Ok(Ok(_)) = h.await {
                            ok_count += 1;
                        }
                    }
                    let end = Utc::now();

                    common.release_host_index(hidx);

                    let all_ok = ok_count == copies;
                    let err = if all_ok {
                        String::new()
                    } else {
                        format!("{}/{} copies failed", copies - ok_count, copies)
                    };

                    let _ = tx.send(Operation {
                        start,
                        end,
                        first_byte: None,
                        last_byte: Some(end),
                        op_type: "POST".into(),
                        err,
                        file: base_name,
                        client_id: client_id.clone(),
                        endpoint,
                        obj_per_op: copies as u32,
                        size: if all_ok {
                            size * copies as i64
                        } else {
                            0
                        },
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
