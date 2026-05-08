//! Zip benchmark: build a small ZIP archive in memory and upload via PutObject.

use crate::bench::{Benchmark, Common, Operation};
use aws_sdk_s3::primitives::ByteStream;
use chrono::Utc;
use rand::Rng;
use std::io::Write;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

pub struct ZipBenchmark {
    common: Common,
    /// Number of files inside the ZIP.
    entries: usize,
}

impl ZipBenchmark {
    pub fn new(common: Common, entries: usize) -> Self {
        Self { common, entries: entries.max(1) }
    }
}

fn build_zip_bytes(prefix: &str, entries: usize, entry_size: usize, rng: &mut impl Rng) -> std::io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    {
        let mut zw = ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for i in 0..entries {
            let name = format!("{prefix}/f-{i:06}.bin");
            zw.start_file(name, opts)?;
            let mut chunk = vec![0u8; entry_size];
            rng.fill(&mut chunk[..]);
            zw.write_all(&chunk)?;
        }
        zw.finish()?;
    }
    Ok(buf)
}

#[async_trait::async_trait]
impl Benchmark for ZipBenchmark {
    async fn prepare(&self, ctx: &CancellationToken) -> crate::generator::Result<()> {
        let client = (self.common.client_factory)(0);
        if !ctx.is_cancelled() {
            let _ = client.create_bucket().bucket(&self.common.bucket).send().await;
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
        let entries = self.entries;
        let common = self.common.clone();

        let sem = Arc::new(tokio::sync::Semaphore::new(concurrency));
        let mut handles = Vec::new();

        for thread_id in 0..concurrency {
            let bucket = bucket.clone();
            let prefix = prefix.clone();
            let sem = sem.clone();
            let ctx = ctx.clone();
            let tx = tx.clone();
            let client_id = client_id.clone();
            let common = common.clone();

            handles.push(tokio::spawn(async move {
                use rand::SeedableRng;
                let mut rng = rand::rngs::SmallRng::from_entropy();
                let deadline = tokio::time::Instant::now() + dur;

                loop {
                    if ctx.is_cancelled() || tokio::time::Instant::now() >= deadline {
                        break;
                    }

                    let _permit = sem.acquire().await.unwrap();
                    if ctx.is_cancelled() || tokio::time::Instant::now() >= deadline {
                        break;
                    }

                    common.throttle_rps().await;

                    let entry_size = match &common.obj_size {
                        crate::generator::ObjSize::Fixed(s) => (*s).max(1) as usize,
                        crate::generator::ObjSize::Random { max } => {
                            rng.gen_range(1..=(*max).max(1)) as usize
                        }
                        crate::generator::ObjSize::Bucketed { buckets, .. } => {
                            buckets.first().map(|(s, _)| (*s).max(1) as usize).unwrap_or(4096)
                        }
                    };

                    let zip_vec = match build_zip_bytes(&prefix, entries, entry_size, &mut rng) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    let zip_len = zip_vec.len() as i64;
                    let body = ByteStream::from(zip_vec);
                    let key = format!("{}/{}.zip", prefix, uuid::Uuid::new_v4());

                    let hidx = common.pick_host_index(thread_id);
                    let client = (common.client_factory)(hidx);
                    let endpoint = common.endpoint_for(hidx);

                    let start = Utc::now();
                    let result = client
                        .put_object()
                        .bucket(&bucket)
                        .key(&key)
                        .body(body)
                        .send()
                        .await;
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
                        obj_per_op: entries as u32,
                        size: if result.is_ok() { zip_len } else { 0 },
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
        if ctx.is_cancelled() {
            return;
        }
        let Ok(list) = client
            .list_objects_v2()
            .bucket(&self.common.bucket)
            .prefix(self.common.prefix())
            .send()
            .await
        else {
            return;
        };
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

    fn common(&self) -> &Common {
        &self.common
    }

    fn ops(&self) -> Vec<Operation> {
        self.common.collector.ops()
    }
}
