//! Snowball 基准测试 — 构建 TAR archive 上传（Snowball Auto-Extract 模式）。
//!
//! 每轮在内存中构建包含 `objs_per` 个随机数据对象的 TAR archive，
//! 使用 PutObject 上传，带 `X-Amz-Meta-Snowball-Auto-Extract: true` 元数据。

use crate::bench::{Benchmark, Common, Operation};
use chrono::Utc;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub struct SnowballBenchmark {
    common: Common,
    obj_size: usize,
    objs_per: usize,
}

impl SnowballBenchmark {
    pub fn new(common: Common, obj_size: usize, objs_per: usize) -> Self {
        Self {
            common,
            obj_size,
            objs_per,
        }
    }
}

/// 构建简化的 TAR archive 字节流。
///
/// 创建 `num_files` 个 entry，每个 entry 引用同一份 `data` 内容（duplicate 模式）。
/// 格式：每文件 512-byte header + 数据（对齐到 512 字节边界）+ 结束标记（1024 零字节）。
fn build_tar(prefix: &str, data: &[u8], num_files: usize) -> Vec<u8> {
    let mut buf = Vec::new();

    for i in 0..num_files {
        let name = format!("{}/obj-{:08}.dat", prefix, i);
        let name_bytes = name.as_bytes();
        let mut header = [0u8; 512];

        // name[0..100]
        let name_len = name_bytes.len().min(100);
        header[..name_len].copy_from_slice(&name_bytes[..name_len]);

        // mode[100..108] — 0644 (八进制)
        header[100..108].copy_from_slice(b"0000644 ");
        // uid[108..116]
        header[108..116].copy_from_slice(b"0000750 ");
        // gid[116..124]
        header[116..124].copy_from_slice(b"0000750 ");
        // size[124..136] — 文件大小（八进制）
        let size_str = format!("{:011o}", data.len());
        header[124..136].copy_from_slice(size_str.as_bytes());
        // mtime[136..148]
        header[136..148].copy_from_slice(b"00000000000");
        // chksum[148..156] — 先填充空格，最后计算
        header[148..156].copy_from_slice(b"        ");
        // typeflag[156] — '0' 表示普通文件
        header[156] = b'0';
        // magic[257..263] + version[263..265]
        header[257..263].copy_from_slice(b"ustar ");
        header[263..265].copy_from_slice(b" \0");

        // 计算 checksum：所有字节相加（checksum 字段已填为空格）
        let checksum: u32 = header.iter().map(|&b| b as u32).sum();
        let chk_str = format!("{:06o}", checksum);
        header[148..154].copy_from_slice(chk_str.as_bytes());
        header[154] = b'\0';
        header[155] = b' ';

        // 写入 header
        buf.extend_from_slice(&header);
        // 写入数据
        buf.extend_from_slice(data);
        // 对齐到 512 字节
        let pad = (512 - (data.len() % 512)) % 512;
        if pad > 0 {
            buf.extend(std::iter::repeat(0u8).take(pad));
        }
    }

    // 结束标记：两个全零 512-byte block
    buf.extend(std::iter::repeat(0u8).take(1024));
    buf
}

#[async_trait::async_trait]
impl Benchmark for SnowballBenchmark {
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
        let prefix = self.common.prefix();
        let client_id = format!("client-{}", self.common.client_idx);
        let obj_size = self.obj_size;
        let objs_per = self.objs_per;
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
                use rand::Rng;
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

                    // 生成随机数据 buffer（所有 entry 共享）
                    let mut data = vec![0u8; obj_size];
                    rng.fill(&mut data[..]);

                    // 构建 TAR archive 字节流
                    let tar_bytes = build_tar(&prefix, &data, objs_per);
                    let _tar_size = tar_bytes.len() as i64;

                    let key = format!("{}/snowball.tar", prefix);
                    let body = aws_sdk_s3::primitives::ByteStream::from(tar_bytes);

                    common.throttle_rps().await;

                    let hidx = common.pick_host_index(thread_id);
                    let client = (common.client_factory)(hidx);
                    let endpoint = common.endpoint_for(hidx);

                    let start = Utc::now();
                    let result = client
                        .put_object()
                        .bucket(&bucket)
                        .key(&key)
                        .body(body)
                        .metadata("X-Amz-Meta-Snowball-Auto-Extract", "true")
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
                        op_type: "PUT".into(),
                        err,
                        file: key,
                        client_id: client_id.clone(),
                        endpoint,
                        obj_per_op: objs_per as u32,
                        size: if result.is_ok() {
                            (objs_per * obj_size) as i64
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
