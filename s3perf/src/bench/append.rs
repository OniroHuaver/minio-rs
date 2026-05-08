//! Append Benchmark — 模拟 AppendObject 语义（覆盖写同一 key，递增 body）。
//!
//! aws-sdk-s3 不支持 MinIO AppendObject API，因此使用 PutObject 覆盖写
//! 同一个 key，累积 body 大小递增，模拟 append 行为：
//! - 第 1 次 PutObject：创建 obj_size 字节对象
//! - 第 N 次 PutObject：覆盖写，obj_size 字节
//! - 满 10000 次后切换新对象
//!
//! ## 技术说明
//! ByteStream API 限制（无法直接包装 Read trait），每轮实际发送固定大小
//! (obj_size) 的 body；Operation 中记录 size = part * obj_size 作为
//! 逻辑累积大小，用于 throughput 统计。

use crate::bench::{Benchmark, Common, Operation};
use bytes::Bytes;
use chrono::Utc;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// 单个 append 链的最大操作次数（之后切换到新对象）
const MAX_APPEND_PARTS: u32 = 10000;

/// 默认每次追加的数据大小 (10 MiB)
const DEFAULT_OBJ_SIZE: i64 = 10 * 1024 * 1024;

pub struct AppendBenchmark {
    common: Common,
    obj_size: i64,
}

impl AppendBenchmark {
    pub fn new(common: Common, obj_size: Option<i64>) -> Self {
        Self {
            common,
            obj_size: obj_size.unwrap_or(DEFAULT_OBJ_SIZE),
        }
    }
}

#[async_trait::async_trait]
impl Benchmark for AppendBenchmark {
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
        let prefix = self.common.prefix();
        let obj_size = self.obj_size;
        let common = self.common.clone();

        let sem = Arc::new(tokio::sync::Semaphore::new(concurrency));
        let mut handles = Vec::new();

        for thread_id in 0..concurrency {
            let bucket = bucket.clone();
            let sem = sem.clone();
            let ctx = ctx.clone();
            let tx = tx.clone();
            let client_id = client_id.clone();
            let prefix = prefix.clone();
            let common = common.clone();

            handles.push(tokio::spawn(async move {
                use rand::Rng;
                use rand::SeedableRng;
                let mut rng = rand::rngs::SmallRng::from_entropy();
                let deadline = tokio::time::Instant::now() + dur;

                // 预生成 obj_size 字节随机数据 buffer，每轮克隆使用（Bytes 克隆是 O(1)）
                let mut buf = vec![0u8; obj_size as usize];
                rng.fill(&mut buf[..]);
                let data_chunk = Bytes::from(buf);

                let mut part: u32 = 1;
                let mut key_name = format!("{}.{:016x}.append", prefix, thread_id);
                let mut obj_counter: u64 = thread_id as u64;

                loop {
                    if ctx.is_cancelled() || tokio::time::Instant::now() >= deadline {
                        break;
                    }

                    let _permit = sem.acquire().await.unwrap();
                    if ctx.is_cancelled() || tokio::time::Instant::now() >= deadline {
                        break;
                    }

                    // 发送 obj_size 字节数据；Operation 中记录逻辑累积大小
                    let body = aws_sdk_s3::primitives::ByteStream::from(data_chunk.clone());
                    let logical_size = (part as i64) * obj_size;

                    common.throttle_rps().await;

                    let hidx = common.pick_host_index(thread_id);
                    let client = (common.client_factory)(hidx);
                    let endpoint = common.endpoint_for(hidx);

                    let start = Utc::now();
                    let result = client
                        .put_object()
                        .bucket(&bucket)
                        .key(&key_name)
                        .body(body)
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
                        last_byte: Some(end),
                        op_type: "APPEND".into(),
                        err,
                        file: key_name.clone(),
                        client_id: client_id.clone(),
                        endpoint,
                        obj_per_op: 1,
                        size: if result.is_ok() { logical_size } else { 0 },
                        thread: thread_id as u32,
                        categories: 0,
                    });

                    part += 1;
                    if part > MAX_APPEND_PARTS {
                        // 达到最大 append 次数，切换到新对象
                        part = 1;
                        obj_counter += 1;
                        key_name = format!("{}.{:016x}.append", prefix, obj_counter);
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
