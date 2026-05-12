//! Multipart benchmark — single large object via multipart upload, then concurrent ranged GETs.

use crate::bench::{Benchmark, Common, Operation};
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
use chrono::Utc;
use rand::Rng;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub struct MultipartBenchmark {
    common: Common,
    part_size: usize,   // --part.size default 5MiB
    parts: usize,       // --parts default 200
    obj_name: String,   // --obj.name default `s3perf-multipart.bin`
}

impl MultipartBenchmark {
    pub fn new(common: Common, part_size: usize, parts: usize, obj_name: String) -> Self {
        Self { common, part_size, parts, obj_name }
    }
}

#[async_trait::async_trait]
impl Benchmark for MultipartBenchmark {
    async fn prepare(&self, ctx: &CancellationToken) -> crate::generator::Result<()> {
        let client = (self.common.client_factory)(0);

        // Create target bucket
        let _ = client
            .create_bucket()
            .bucket(&self.common.bucket)
            .send()
            .await;

        if ctx.is_cancelled() {
            return Ok(());
        }

        // Initiate multipart upload session
        let create_resp = client
            .create_multipart_upload()
            .bucket(&self.common.bucket)
            .key(&self.obj_name)
            .send()
            .await
            .map_err(|e| crate::generator::Error::S3(e.to_string()))?;

        let upload_id = create_resp
            .upload_id()
            .ok_or_else(|| crate::generator::Error::S3("missing upload_id".into()))?
            .to_string();

        // Fan out part uploads concurrently
        let sem = Arc::new(tokio::sync::Semaphore::new(self.common.concurrency));
        let etags = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut handles = Vec::new();

        let prepare_ctx = ctx.clone();
        for part_number in 1..=self.parts {
            if prepare_ctx.is_cancelled() {
                break;
            }
            let client = client.clone();
            let bucket = self.common.bucket.clone();
            let obj_name = self.obj_name.clone();
            let upload_id = upload_id.clone();
            let sem = sem.clone();
            let etags = etags.clone();
            let part_size = self.part_size;
            let ctx = prepare_ctx.clone();

            handles.push(tokio::spawn(async move {
                let Ok(_permit) = sem.acquire().await else { return; };
                if ctx.is_cancelled() {
                    return;
                }

                let mut data = vec![0u8; part_size];
                rand::thread_rng().fill(&mut data[..]);
                let body = ByteStream::from(data);

                let resp = client
                    .upload_part()
                    .bucket(&bucket)
                    .key(&obj_name)
                    .upload_id(&upload_id)
                    .part_number(part_number as i32)
                    .body(body)
                    .send()
                    .await;

                let etag = match &resp {
                    Ok(r) => r.e_tag().unwrap_or_default().to_string(),
                    Err(_) => String::new(),
                };
                etags.lock().unwrap().push((part_number, etag));
            }));
        }

        for h in handles {
            let _ = h.await;
        }

        if ctx.is_cancelled() {
            return Ok(());
        }

        // Complete multipart upload
        let parts: Vec<CompletedPart> = etags
            .lock()
            .unwrap()
            .iter()
            .map(|(pn, etag)| {
                CompletedPart::builder()
                    .e_tag(etag.as_str())
                    .part_number(*pn as i32)
                    .build()
            })
            .collect();

        let completed_upload = CompletedMultipartUpload::builder()
            .set_parts(Some(parts))
            .build();

        let _ = client
            .complete_multipart_upload()
            .bucket(&self.common.bucket)
            .key(&self.obj_name)
            .upload_id(&upload_id)
            .multipart_upload(completed_upload)
            .send()
            .await;

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
        let obj_name = self.obj_name.clone();
        let parts = self.parts;
        let _part_size = self.part_size as i64;
        let common = self.common.clone();

        let sem = Arc::new(tokio::sync::Semaphore::new(concurrency));
        let mut handles = Vec::new();

        for thread_id in 0..concurrency {
            let bucket = bucket.clone();
            let sem = sem.clone();
            let ctx = ctx.clone();
            let tx = tx.clone();
            let client_id = client_id.clone();
            let obj_name = obj_name.clone();
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

                    let part_number = rng.gen_range(1..=parts);

                    common.throttle_rps().await;

                    let hidx = common.pick_host_index(thread_id);
                    let client = (common.client_factory)(hidx);
                    let endpoint = common.endpoint_for(hidx);

                    let start = Utc::now();
                    let req = client
                        .get_object()
                        .bucket(&bucket)
                        .key(&obj_name)
                        .part_number(part_number as i32);
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
                        file: format!("{}:part{}", obj_name, part_number),
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

        if !ctx.is_cancelled() {
            let _ = client
                .delete_bucket()
                .bucket(&self.common.bucket)
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
