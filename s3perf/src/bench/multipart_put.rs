//! MultipartPut Benchmark — 并发执行 MultipartUpload 生命周期。
//!
//! 流程: CreateMultipartUpload → 并发 UploadPart → CompleteMultipartUpload
//! 每个外层 worker 独立循环完整的 multipart 生命周期。
//! CreateMultipartUpload 和 CompleteMultipartUpload 不计入 Operation。

use crate::bench::{Benchmark, Common, Operation};
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

pub struct MultipartPutBenchmark {
    common: Common,
    parts: usize,
    part_size: usize,
    part_concurrency: usize,
}

impl MultipartPutBenchmark {
    pub fn new(common: Common, parts: usize, part_size: usize, part_concurrency: usize) -> Self {
        Self {
            common,
            parts,
            part_size,
            part_concurrency,
        }
    }
}

#[async_trait::async_trait]
impl Benchmark for MultipartPutBenchmark {
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
        let parts = self.parts;
        let part_size = self.part_size;
        let part_concurrency = self.part_concurrency;
        let common = self.common.clone();

        let sem = Arc::new(Semaphore::new(concurrency));
        let mut handles = Vec::new();

        for thread_id in 0..concurrency {
            let bucket = bucket.clone();
            let sem = sem.clone();
            let ctx = ctx.clone();
            let tx = tx.clone();
            let client_id = client_id.clone();
            let common = common.clone();

            handles.push(tokio::spawn(async move {
                let deadline = tokio::time::Instant::now() + dur;
                let mut upload_counter = 0u64;

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

                    let hidx = common.pick_host_index(thread_id);
                    let client = (common.client_factory)(hidx);
                    let endpoint = common.endpoint_for(hidx);

                    // Step 1: CreateMultipartUpload
                    let key = format!("{}.upload-{:04x}", thread_id, upload_counter);
                    let create_res = client
                        .create_multipart_upload()
                        .bucket(&bucket)
                        .key(&key)
                        .send()
                        .await;

                    let upload_id = match create_res {
                        Ok(resp) => match resp.upload_id {
                            Some(id) => id,
                            None => {
                                common.release_host_index(hidx);
                                upload_counter += 1;
                                continue;
                            }
                        },
                        Err(_) => {
                            common.release_host_index(hidx);
                            upload_counter += 1;
                            continue;
                        }
                    };

                    // Step 2: 并发上传 parts
                    let inner_sem = Arc::new(Semaphore::new(part_concurrency));
                    let mut part_handles = Vec::new();

                    for part_number in 1..=parts {
                        if ctx.is_cancelled() || tokio::time::Instant::now() >= deadline {
                            break;
                        }

                        let Ok(permit) = inner_sem.clone().acquire_owned().await else {
                            return;
                        };
                        if ctx.is_cancelled() || tokio::time::Instant::now() >= deadline {
                            break;
                        }

                        let client = client.clone();
                        let bucket = bucket.clone();
                        let key = key.clone();
                        let upload_id = upload_id.clone();
                        let tx = tx.clone();
                        let endpoint = endpoint.clone();
                        let client_id = client_id.clone();
                        let inner_worker_index = part_number - 1;

                        part_handles.push(tokio::spawn(async move {
                            let _permit = permit;
                            let body_data = vec![0u8; part_size];
                            let body = aws_sdk_s3::primitives::ByteStream::from(body_data);

                            let start = Utc::now();
                            let result = client
                                .upload_part()
                                .bucket(&bucket)
                                .key(&key)
                                .upload_id(&upload_id)
                                .part_number(part_number as i32)
                                .body(body)
                                .send()
                                .await;
                            let end = Utc::now();

                            let err = match &result {
                                Ok(_) => String::new(),
                                Err(e) => e.to_string(),
                            };
                            let etag = result
                                .as_ref()
                                .ok()
                                .and_then(|r| r.e_tag.clone())
                                .unwrap_or_default();

                            let _ = tx.send(Operation {
                                start,
                                end,
                                first_byte: None,
                                last_byte: Some(end),
                                op_type: "PUTPART".into(),
                                err,
                                file: key,
                                client_id: client_id.clone(),
                                endpoint,
                                obj_per_op: 1,
                                size: if result.is_ok() { part_size as i64 } else { 0 },
                                thread: (thread_id * part_concurrency + inner_worker_index) as u32,
                                categories: 0,
                            });

                            (part_number, etag, result.is_ok())
                        }));
                    }

                    // 等待所有 part 上传完成
                    let mut completed_parts = Vec::new();
                    let mut all_ok = true;
                    for h in part_handles {
                        match h.await {
                            Ok((pn, etag, ok)) => {
                                if ok && !etag.is_empty() {
                                    let cp = aws_sdk_s3::types::CompletedPart::builder()
                                        .part_number(pn as i32)
                                        .e_tag(&etag)
                                        .build();
                                    completed_parts.push(cp);
                                } else {
                                    all_ok = false;
                                }
                            }
                            Err(_) => {
                                all_ok = false;
                            }
                        }
                    }

                    // Step 3: CompleteMultipartUpload 或 Abort
                    if all_ok && !completed_parts.is_empty() {
                        let multipart_upload =
                            aws_sdk_s3::types::CompletedMultipartUpload::builder()
                                .set_parts(Some(completed_parts))
                                .build();

                        let _ = client
                            .complete_multipart_upload()
                            .bucket(&bucket)
                            .key(&key)
                            .upload_id(&upload_id)
                            .multipart_upload(multipart_upload)
                            .send()
                            .await;
                    } else {
                        let _ = client
                            .abort_multipart_upload()
                            .bucket(&bucket)
                            .key(&key)
                            .upload_id(&upload_id)
                            .send()
                            .await;
                    }

                    common.release_host_index(hidx);
                    upload_counter += 1;
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
