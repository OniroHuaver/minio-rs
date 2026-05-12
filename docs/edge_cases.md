# MinIO 极端 Case 处理策略 (Rust 重写版)

> 基于原 MinIO Go `master` 分支 (2026-05) 翻译整理
> Rust 重写项目: minio-rs | API 参考见 `./API_REFERENCE.md`
> 涵盖存储、网络、并发、安全各层面

---

## 目录

1. [磁盘故障 & 静默损坏](#1-磁盘故障--静默损坏)
2. [网络分区 & Quorum 丢失](#2-网络分区--quorum-丢失)
3. [并发写入冲突](#3-并发写入冲突)
4. [数据恢复 & 修复边界](#4-数据恢复--修复边界)
5. [格式迁移 & 向后兼容](#5-格式迁移--向后兼容)
6. [认证 & 安全边界](#6-认证--安全边界)
7. [极端文件大小](#7-极端文件大小)
8. [Multipart Upload 边界](#8-multipart-upload-边界)
9. [复制冲突 & 一致性](#9-复制冲突--一致性)
10. [解配安全](#10-解配安全)

---

## 1. 磁盘故障 & 静默损坏

### 1.1 磁盘离线检测

**问题**: 磁盘突然离线 (拔出、电源故障、SAN 断连) 时，如何保证数据不丢失？

**处理策略**:

写入路径使用 `tokio::task::JoinSet` 并行写入所有 M+N 磁盘。部分磁盘写入失败时，只要满足 WriteQuorum 即认为成功，失败磁盘的分片标记为缺失 — 后台修复负责补全。

读取路径并行从在线磁盘读取分片：在线数 >= ReadQuorum 时通过 Reed-Solomon 解码恢复缺失分片；在线数 < ReadQuorum 时返回 `SlowDownRead` (503)。

```rust
// erasure/src/quorum.rs
pub fn default_write_quorum(data_blocks: usize, parity_blocks: usize) -> usize {
    if data_blocks == parity_blocks { data_blocks } else { data_blocks + 1 }
}

pub fn default_read_quorum(total_disks: usize, parity_blocks: usize) -> usize {
    total_disks - parity_blocks
}

// erasure/src/object.rs — 并行分片写入
pub async fn put_erasure_shards(
    disks: &[StorageAPI],
    shards: Vec<Vec<u8>>,
    data_blocks: usize,
) -> Result<(), ErasureError> {
    let write_quorum = default_write_quorum(data_blocks, disks.len() - data_blocks);
    let mut results = Vec::new();
    for (i, disk) in disks.iter().enumerate() {
        results.push(tokio::spawn(async move {
            disk.write_all(&shards[i]).await
        }));
    }
    let success_count = futures::future::join_all(results)
        .await
        .into_iter()
        .filter(|r| r.is_ok())
        .count();
    if success_count >= write_quorum {
        Ok(())
    } else {
        Err(ErasureError::InsufficientWriteQuorum)
    }
}
```

---

### 1.2 静默数据损坏 (Bitrot)

**问题**: 磁盘静默损坏 (bit flipping、校验错误) 未报告 I/O 错误，数据已破坏但系统无感知。

**处理策略**:

每个 EC 分片写入后计算 HighwayHash256 哈希，存入 xl.meta 的 Parts 条目。读取时重新计算并与存储的哈希对比；不匹配时标记该分片为损坏，从其他盘通过 Reed-Solomon 解码恢复，成功恢复后自动回写正确数据 (在线修复)。

扫描器 (DataScanner) 以概率 `1/1024` 抽查对象，发现任何分片哈希不匹配即入队修复。

```rust
// storage/src/bitrot.rs
use highway::{HighwayHash, Key};

pub fn compute_bitrot_hash(data: &[u8]) -> [u8; 32] {
    let key = Key([1, 2, 3, 4]); // 固定 key，用于一致性校验
    let mut hasher = highway::PortableHash::new(key);
    hasher.absorb(data);
    hasher.finalize256()
}

pub fn verify_shard(data: &[u8], expected_hash: &[u8; 32]) -> bool {
    let actual = compute_bitrot_hash(data);
    &actual == expected_hash
}
```

```rust
// erasure/src/recovery.rs — 读取时自动恢复损坏分片
pub async fn read_with_recovery(
    disks: &[StorageAPI], shard_sizes: &[usize],
    data_blocks: usize, parity_blocks: usize,
    bitrot_hashes: &[[u8; 32]],
) -> Result<Vec<u8>, ErasureError> {
    let mut shards = vec![None; disks.len()];
    for (i, disk) in disks.iter().enumerate() {
        let data = disk.read_shard(shard_sizes[i]).await?;
        if verify_shard(&data, &bitrot_hashes[i]) {
            shards[i] = Some(data);
        }
        // 哈希不匹配: 保持 None，由 RS 恢复
    }
    let present: usize = shards.iter().filter(|s| s.is_some()).count();
    if present < data_blocks {
        return Err(ErasureError::InsufficientReadQuorum);
    }
    // Reed-Solomon 重建缺失分片
    let reconstructed = reed_solomon_erasure::reconstruct(&mut shards, data_blocks)?;
    // 自动回写修复损坏的分片
    for (i, shard) in shards.iter().enumerate() {
        if shard.is_none() && disks[i].is_online() {
            tokio::spawn(async move { disks[i].write_shard(reconstructed[i].clone()).await });
        }
    }
    Ok(join_data_shards(&shards[..data_blocks]))
}
```

---

### 1.3 磁盘满 (ENOSPC)

**问题**: 写入时磁盘空间不足。

**处理策略**: 采用"写临时文件 → 原子 Rename"模式 (`write-tmp-then-rename`)。`CreateFile()` 返回 `ENOSPC` 时，该磁盘被记为写入失败。如果剩余磁盘满足 WriteQuorum 则写入成功；否则返回 `InsufficientWriteQuorum`。失败时的临时文件由后台清理器处理。

```rust
// storage/src/xl_storage.rs
pub async fn write_all(&self, path: &Path, data: &[u8]) -> Result<(), StorageError> {
    let tmp_path = self.tmp_dir().join(uuid::Uuid::new_v4().to_string());
    // 写入临时文件
    match tokio::fs::write(&tmp_path, data).await {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::StorageFull => {
            return Err(StorageError::DiskFull); // 不阻塞其他盘
        }
        Err(e) => return Err(StorageError::Io(e)),
    }
    // 原子 rename 覆盖目标
    tokio::fs::rename(&tmp_path, path).await.map_err(|e| {
        let _ = tokio::fs::remove_file(&tmp_path); // 清理临时文件
        StorageError::Io(e)
    })
}
```

---

### 1.4 磁盘 I/O 错误 (EIO)

单次 EIO 重试最多 3 次，持续 EIO 将磁盘标记为 Faulty 并触发后台格式化检查。在 Rust 中使用重试包装器:

```rust
// storage/src/retry.rs
pub async fn with_retry<F, T>(f: F, max_retries: u32) -> Result<T, StorageError>
where F: Fn() -> F
{
    let mut last_err = None;
    for attempt in 0..max_retries {
        match f().await {
            Ok(val) => return Ok(val),
            Err(e) if is_io_error(&e) && attempt < max_retries - 1 => {
                tokio::time::sleep(Duration::from_millis(50 * (1 << attempt))).await;
                last_err = Some(e);
            }
            Err(e) => return Err(e),
        }
    }
    Err(last_err.unwrap())
}
```

---

## 2. 网络分区 & Quorum 丢失

### 2.1 分布式写 Quorum 丢失

**场景**: 12 节点集群，网络分区将集群分为各 6 节点的 A、B 两区。

**处理**: 每个分区节点数 (6) < WriteQuorum (EC:4 时为 7)，因此两个分区都无法写入 → 防止脑裂。客户端收到 `SlowDownWrite` (503) 后应重试。

```rust
// erasure/src/object.rs
pub async fn put_object_distributed(
    pool: &ServerPool, bucket: &str, object: &str, data: &[u8],
) -> Result<ObjectInfo, S3Error> {
    let set = pool.pick_set(bucket, object);
    let disks = set.get_online_disks().await;
    let wq = default_write_quorum(set.data_blocks, set.parity_blocks);
    if disks.len() < wq {
        return Err(S3Error::SlowDownWrite("Insufficient write quorum"));
    }
    // ... 继续写入
}
```

---

### 2.2 分布式读 Quorum 丢失

**场景**: 8 盘 EC:4，4 盘同时离线。4 盘在线 == ReadQuorum，刚好满足 — 可读取 (M=4 可恢复)。3 盘在线 < ReadQuorum 时，部分对象可能仍可读 (如果数据块恰好在在线盘中)，但不保证全部可读，返回 503。

```rust
// erasure/src/object.rs
pub async fn get_object_distributed(
    pool: &ServerPool, bucket: &str, object: &str,
) -> Result<Vec<u8>, S3Error> {
    let set = pool.pick_set(bucket, object);
    let disks = set.get_online_disks().await;
    let rq = default_read_quorum(set.drive_count(), set.parity_blocks);
    if disks.len() < rq {
        // 尝试部分读取 — 若有足够数据块则仍可恢复
        return try_partial_read(set, object).await;
    }
    // ... 完整读取
}
```

---

### 2.3 节点间 RPC 超时

**场景**: 远程磁盘 RPC 调用超时 (网络抖动、GC 停滞)。

**处理**: 默认超时 30s (连接超时 10s，TLS 握手超时 10s)。单次超时指数退避重试，持续超时标记磁盘为 Offline，后台 goroutine 周期性重连。在 Rust 中使用 `tokio::time::timeout` 包装 RPC 调用:

```rust
// grid/src/client.rs
const RPC_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

pub async fn call_remote<T: Serialize + DeserializeOwned>(
    client: &TcpStream, request: &RpcRequest,
) -> Result<T, GridError> {
    let mut attempts = 0;
    loop {
        match tokio::time::timeout(RPC_TIMEOUT, async {
            send_request(client, request).await?;
            recv_response::<T>(client).await
        }).await {
            Ok(Ok(val)) => return Ok(val),
            Ok(Err(e)) if attempts < 3 && is_retryable(&e) => {
                let delay = Duration::from_millis(100 * (1 << attempts));
                tokio::time::sleep(delay).await;
                attempts += 1;
            }
            Ok(Err(e)) => return Err(e),
            Err(_elapsed) => return Err(GridError::Timeout),
        }
    }
}
```

---

## 3. 并发写入冲突

### 3.1 同对象并发 PUT

**场景**: 两个客户端同时对 `bucket/object` 执行 PUT。

**处理 — 分布式命名空间锁**:

```rust
// object/src/locker.rs
pub struct NamespaceLocker {
    inner: Arc<dashmap::DashMap<(String, String), tokio::sync::RwLock<()>>>,
}

impl NamespaceLocker {
    pub async fn lock_write(&self, bucket: &str, object: &str) -> LockGuard {
        let key = (bucket.to_string(), object.to_string());
        let lock = self.inner.entry(key).or_insert_with(|| tokio::sync::RwLock::new(()));
        // 获取写锁 (互斥)
        let guard = lock.write().await;
        // Phase 2: 扩展为 dsync 分布式锁，当前为本地 tokio::sync::RwLock
        LockGuard(Box::new(guard))
    }

    pub async fn lock_read(&self, bucket: &str, object: &str) -> LockGuard {
        let key = (bucket.to_string(), object.to_string());
        let lock = self.inner.entry(key).or_insert_with(|| tokio::sync::RwLock::new(()));
        // 获取读锁 (允许并发读)
        let guard = lock.read().await;
        LockGuard(Box::new(guard))
    }
}
```

**锁模式**:
- **读锁**: ListObjects, GetObject, HeadObject → 允许并发读
- **写锁**: PutObject, DeleteObject, CopyObject → 互斥

**死锁防护**: 超时自动释放 + 随机 jitter。在 Rust 中使用 `tokio::time::timeout`:

```rust
const NS_LOCK_TIMEOUT: Duration = Duration::from_secs(30);

pub async fn try_lock_write(&self, bucket: &str, object: &str) -> Result<LockGuard, S3Error> {
    tokio::time::timeout(NS_LOCK_TIMEOUT, self.lock_write(bucket, object))
        .await
        .map_err(|_| S3Error::LockTimeout)
}
```

---

### 3.2 Versions Journal 并发合并

**场景**: 同一对象同时写入两个不同版本 (多客户端或版本控制开启)。

**处理**: `RenameData()` 使用读取-合并-写入的原子模式:

1. 读取当前 `xl.meta` → 获取现有版本数组
2. 构建新版本 Entry
3. 合并: `append(existing_versions, new_version)`
4. 写新 `xl.meta` 到临时文件
5. 原子 Rename 覆盖原 `xl.meta`
6. 如果 Rename 失败 (并发冲突): 重新读取、重新合并、重新 Rename (最多重试 3 次)

```rust
// base/src/xl_meta.rs
pub async fn append_version(
    path: &Path, new_version: VersionEntry,
) -> Result<(), StorageError> {
    let max_retries = 3;
    for attempt in 0..max_retries {
        let current = read_xl_meta(path).await.unwrap_or_default();
        let mut versions = current.versions;
        versions.push(new_version.clone());
        let encoded = rmp_serde::to_vec(&versions)
            .map_err(|e| StorageError::Serialization(e))?;
        let tmp = path.with_extension("meta.tmp");
        tokio::fs::write(&tmp, &encoded).await?;
        match tokio::fs::rename(&tmp, path).await {
            Ok(()) => return Ok(()),
            Err(_) if attempt < max_retries - 1 => continue,
            Err(e) => return Err(StorageError::Io(e)),
        }
    }
    Err(StorageError::ConflictRetryExceeded)
}
```

**关键**: xl.meta 是追加式日志，新版本追加到数组末尾，不会覆盖已有版本。

---

### 3.3 Multipart 并发分片

**场景**: 多个客户端并发上传同一 Multipart Upload 的不同分片。

**处理**: 分片独立存储为 `part.1`, `part.2`, ... 文件，不同分片不冲突。同一分片号并发 PUT 采用 last-write-wins。CompleteMultipartUpload 时获取写锁，验证所有分片存在并合并为一个版本写入 xl.meta。

```rust
// s3/src/handlers/multipart.rs
pub async fn complete_multipart_upload(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    Query(params): Query<CompleteParams>,
    XmlBody(parts): XmlBody<CompleteMultipartUpload>,
) -> Result<XmlResponse<CompleteResult>, S3Error> {
    let _lock = state.locker.lock_write(&bucket, &key).await;
    for part in &parts.parts {
        if !state.object.part_exists(&bucket, &key, &params.upload_id, part.number).await? {
            return Err(S3Error::InvalidPart { number: part.number });
        }
    }
    let info = state.object.complete_multipart(
        &bucket, &key, &params.upload_id, parts.parts,
    ).await?;
    Ok(XmlResponse(CompleteResult { etag: info.etag }))
}
```

---

## 4. 数据恢复 & 修复边界

### 4.1 部分修复 (Partial Heal)

**场景**: 修复中途 MinIO 进程被 kill，磁盘再次故障。

**处理**: 后台修复是幂等的。修复前读取各磁盘 xl.meta 获取版本签名，比较签名确定哪些磁盘缺少/损坏哪些版本，Reed-Solomon 重建后写入缺失磁盘。中途中断时，下次扫描重新发现不一致并再次修复。写入是原子的 (tmp → rename)，不会产生"修复到一半"的状态。

```rust
// object/src/heal.rs
pub async fn heal_object(
    disks: &[StorageAPI], bucket: &str, object: &str,
) -> Result<HealResult, S3Error> {
    // 1. 读取所有磁盘的 xl.meta，获取版本签名
    let mut signatures = Vec::new();
    for disk in disks {
        let sig = disk.read_xl_meta_signature(bucket, object).await;
        signatures.push(sig);
    }
    // 2. 选取多数派签名作为正确版本
    let correct_sig = majority_signature(&signatures)?;
    // 3. 对每个缺失/损坏的磁盘，用 RS 重建
    for (i, disk) in disks.iter().enumerate() {
        if signatures[i] != correct_sig && disk.is_online() {
            let shards = read_reconstruct(disks, correct_sig).await?;
            disk.write_shard(shards[i]).await?;    // 原子写入
            disk.write_xl_meta(bucket, object, correct_sig).await?; // 原子写入
        }
    }
    Ok(HealResult { fixed: true, .. })
}
```

---

### 4.2 N-1 磁盘同时故障

**场景**: 8 盘 EC:4，3 盘同时物理损坏。数学上 M=4, N=4:
- 丢失 3 盘 < N(=4): Reed-Solomon 可完全恢复
- 丢失 4 盘 == N: 理论可恢复，但如果数据块恰好分布在损坏的 4 盘中则不可恢复
- 丢失 5 盘 > N: 不可完整恢复，部分对象可能丢失

**MinIO 的做法**: 即使超过 N 个磁盘损坏，仍然返回能恢复的数据，并在响应头标记缺失部分。Rust 实现遵循相同策略:

```rust
// erasure/src/recovery.rs
pub fn can_recover(missing: usize, parity: usize) -> bool {
    missing <= parity
}

pub fn recover_shards(shards: &mut [Option<Vec<u8>>], data_blocks: usize, parity_blocks: usize)
    -> Result<(), ErasureError>
{
    let missing = shards.iter().filter(|s| s.is_none()).count();
    if missing > parity_blocks {
        return Err(ErasureError::TooManyFailures {
            missing, max_recoverable: parity_blocks,
        });
    }
    reed_solomon_erasure::reconstruct(shards, data_blocks)?;
    Ok(())
}
```

---

### 4.3 xl.meta 自身损坏

**场景**: 磁盘上的 `xl.meta` 文件损坏 (部分写入、磁盘坏道)。

**恢复策略**:

1. 读取 xl.meta 时验证 "XL2 " 魔数 (4 字节) 和版本号 (Major/Minor 各 2 字节)，MessagePack 反序列化。任何步骤失败 → 标记该磁盘此对象为损坏。
2. 每次写入 xl.meta 前，先复制到 `xl.meta.bkp`。当 xl.meta 损坏时，尝试读取备份文件，通过版本签名对比确定备份是否完整。
3. 跨磁盘恢复: 从其他磁盘的 xl.meta 读取正确版本日志，Reed-Solomon 重建缺失数据分片并回写。

```rust
// base/src/xl_meta.rs
const XL2_MAGIC: &[u8; 4] = b"XL2 ";

#[repr(C, packed)]
struct XlMetaHeader {
    magic: [u8; 4],     // "XL2 "
    major: u16,          // BE
    minor: u16,          // BE
}
// Total: 8 字节

pub fn parse_xl_meta(raw: &[u8]) -> Result<Vec<VersionEntry>, MetaError> {
    if raw.len() < 8 || &raw[0..4] != XL2_MAGIC {
        return Err(MetaError::InvalidMagic);
    }
    let major = u16::from_be_bytes([raw[4], raw[5]]);
    let minor = u16::from_be_bytes([raw[6], raw[7]]);
    if major > SUPPORTED_MAJOR_VERSION {
        return Err(MetaError::UnsupportedMajorVersion(major));
    }
    // major 已知则兼容读取 (minor 仅用于向后兼容提示)
    rmp_serde::from_slice(&raw[8..])
        .map_err(|e| MetaError::Deserialization(e))
}

pub async fn read_xl_meta_safe(
    primary: &Path, backup: &Path,
) -> Result<Vec<VersionEntry>, MetaError> {
    let primary_data = tokio::fs::read(primary).await?;
    match parse_xl_meta(&primary_data) {
        Ok(versions) => return Ok(versions),
        Err(e) => {
            tracing::warn!(path = %primary, error = %e, "xl.meta corrupt, trying backup");
            // 尝备份
            let backup_data = tokio::fs::read(backup).await?;
            parse_xl_meta(&backup_data)
        }
    }
}
```

---

### 4.4 修复时磁盘再次离线

**场景**: 修复某磁盘的分片时，磁盘再次离线。

**处理**: 每个分片写入是原子的。磁盘离线时 `CreateFile` 返回 `errDiskNotFound`，修复器跳过该分片，继续修复其他分片/对象。下次扫描重新发现并重试，连续失败超阈值时标记磁盘为 Faulty。

```rust
// object/src/heal.rs
pub async fn heal_disk(disk: &StorageAPI, missing_objects: &[ObjectRef]) -> HealStats {
    let mut stats = HealStats::default();
    for obj in missing_objects {
        match heal_single_object(disk, obj).await {
            Ok(()) => stats.fixed += 1,
            Err(StorageError::DiskNotFound) => {
                stats.skipped += 1;  // 磁盘再离线，跳过
                disk.mark_faulty_if_exceeded();
            }
            Err(_) => stats.failed += 1,
        }
    }
    stats
}
```

---

## 5. 格式迁移 & 向后兼容

### 5.1 V1 (xl.json) → V2 (xl.meta) 迁移

**场景**: 旧集群磁盘上有 V1 格式数据 (xl.json)，升级到 V2 后需兼容。

**处理**: `RenameData()` 检测磁盘上是否存在 `xl.json`。如果存在且 `format_legacy = true`，读取 `xl.json` 数据，转换为 `LegacyType` Entry 写入 `xl.meta`。下次写入时用 `ObjectType` 覆盖。新版本 MinIO 始终能读取 V1 格式数据，直到该对象被覆盖写入。

```rust
// base/src/xl_meta.rs
const OBJECT_TYPE: u8 = 1;   // V2 Object
const DELETE_TYPE: u8 = 2;   // V2 Delete Marker
const LEGACY_TYPE: u8 = 3;   // V1 占位 (xl.json)

pub async fn migrate_legacy_if_needed(
    dir: &Path, xl_json_path: &Path,
) -> Result<(), StorageError> {
    if !xl_json_path.exists() {
        return Ok(());  // 已是 V2 格式
    }
    let legacy_data = tokio::fs::read(xl_json_path).await?;
    let legacy_entry = VersionEntry {
        version_type: LEGACY_TYPE,
        // 从 xl.json 解析兼容字段 ...
        ..Default::default()
    };
    let xl_meta_path = dir.join("xl.meta");
    write_xl_meta(&xl_meta_path, &[legacy_entry]).await?;
    // 保留 xl.json 直到下次覆盖写入
    Ok(())
}
```

---

### 5.2 xl.meta 版本号不匹配

**场景**: 不同节点运行不同版本的 MinIO。

**处理**: Header 格式 `"XL2 " + major(2B) + minor(2B)`。major 未知时拒绝读取 (不兼容的格式)。minor 高于已知时兼容读取 (minor 是向后兼容的)。minor 低于已知时正常读取。

```rust
// base/src/xl_meta.rs
const SUPPORTED_MAJOR_VERSION: u16 = 2;

pub fn check_compatibility(major: u16, minor: u16) -> Result<(), MetaError> {
    if major != SUPPORTED_MAJOR_VERSION {
        return Err(MetaError::UnsupportedMajorVersion(major));
    }
    // minor: 高于已知 → 兼容读取（部分新字段可能忽略）
    // minor: 低于已知 → 正常读取
    Ok(())
}
```

---

### 5.3 部署 ID 不匹配

**场景**: 将磁盘从集群 A 移动到集群 B。

**处理**: `format.json` 包含 `deployment_id`。启动时检测: 磁盘 `deployment_id` != 集群 `deployment_id` 时，如果磁盘为空则重新格式化，如果有数据则拒绝挂载。

```rust
// base/src/format.rs
pub async fn validate_disk_format(
    disk_path: &Path, cluster_deployment_id: &str,
) -> Result<DiskState, FormatError> {
    let format = read_format_json(disk_path).await?;
    if format.deployment_id != cluster_deployment_id {
        if format.is_empty() {
            // 空盘: 重新格式化
            write_format_json(disk_path, cluster_deployment_id).await?;
            Ok(DiskState::Reformatted)
        } else {
            // 有数据: 拒绝
            Err(FormatError::DeploymentIdMismatch {
                disk: format.deployment_id,
                cluster: cluster_deployment_id.to_string(),
            })
        }
    } else {
        Ok(DiskState::Matched)
    }
}
```

---

## 6. 认证 & 安全边界

### 6.1 签名时间偏差攻击

**场景**: 攻击者截获有效签名请求后重放。

**防御**: SigV4 签名包含 `X-Amz-Date` 和请求体哈希。服务器校验时间偏差不超过 15 分钟 (`globalMaxSkewTime`)。重放攻击需要同时在 15 分钟内、相同请求体、相同签名头。

```rust
// base/src/auth.rs
const MAX_SKEW_TIME: Duration = Duration::from_secs(15 * 60);

pub fn check_time_skew(request_time: &DateTime<Utc>, server_time: &DateTime<Utc>) -> Result<(), AuthError> {
    let diff = (request_time - server_time).abs();
    if diff > MAX_SKEW_TIME {
        return Err(AuthError::RequestTimeTooSkewed {
            diff: diff.num_seconds(),
            max_skew: MAX_SKEW_TIME.as_secs(),
        });
    }
    Ok(())
}
```

**Presigned URL**: 签名包含过期时间 (`X-Amz-Expires`, 最长 7 天)，过期后自动失效。注意 Presigned URL 设计上等同于持有临时权限，非安全漏洞。

---

### 6.2 JWT 令牌过期

**场景**: STS 凭证过期后的处理。

**处理**:

- 请求携带过期 JWT: `get_claims_from_token()` 检查 `exp` Claim，`exp < now` 时返回 `ErrAccessDenied`
- 令牌即将过期 (距过期 < 5 分钟): 响应头添加 `X-Minio-Warning: token expires soon`
- 令牌在请求处理中途过期: 认证中间件在请求开始时验证，长时间运行的请求不会中途中止

```rust
// iam/src/sts/token.rs
pub fn verify_jwt(token: &str, jwks: &JwkSet) -> Result<Claims, StsError> {
    let token_data = jsonwebtoken::decode::<Claims>(
        token,
        &jsonwebtoken::DecodingKey::from_jwk(jwks)?,
        &jsonwebtoken::Validation::default(),
    )?;
    if token_data.claims.exp < Utc::now().timestamp() as usize {
        return Err(StsError::ExpiredToken);
    }
    let remaining = Duration::seconds(
        token_data.claims.exp as i64 - Utc::now().timestamp()
    );
    if remaining < Duration::from_secs(300) {
        // 触发即将过期的警告 header
    }
    Ok(token_data.claims)
}
```

---

### 6.3 LDAP 暴力破解防护

**场景**: 攻击者对 LDAP STS 端点进行暴力猜测。

**速率限制**: 两维度独立限流 — 按源 IP (防止分布式攻击) 和按用户名 (防止多 IP 攻击同一账户)。令牌桶算法: 每 15 分钟补充 1 个令牌，最大爆发 10 个。

```rust
// iam/src/sts/rate_limit.rs
use governor::{Quota, RateLimiter};
use std::num::NonZeroU32;

pub struct LdapRateLimiter {
    // 按源 IP 限流
    per_ip: Arc<dashmap::DashMap<IpAddr, RateLimiter<IpAddr>>>,
    // 按用户名限流
    per_user: Arc<dashmap::DashMap<String, RateLimiter<String>>>,
}

impl LdapRateLimiter {
    pub fn new() -> Self {
        Self {
            per_ip: Arc::new(DashMap::new()),
            per_user: Arc::new(DashMap::new()),
        }
    }

    pub fn check(&self, ip: &IpAddr, username: &str) -> bool {
        let quota = Quota::with_period(Duration::from_secs(900)) // 15min
            .unwrap()
            .allow_burst(NonZeroU32::new(10).unwrap());
        let ip_ok = self.per_ip
            .entry(*ip)
            .or_insert_with(|| RateLimiter::direct(quota))
            .check()
            .is_ok();
        let user_ok = self.per_user
            .entry(username.to_string())
            .or_insert_with(|| RateLimiter::direct(quota))
            .check()
            .is_ok();
        ip_ok && user_ok
    }
}
```

**可信代理**: 通过 `MINIO_IDENTITY_LDAP_STS_TRUSTED_PROXIES` 配置。来自可信代理的请求，使用 `X-Forwarded-For` 获取真实源 IP。

---

### 6.4 SSE-C 密钥处理

**场景**: 客户端提供密钥进行服务端加密。

**安全要求**:

1. 必须 TLS: `if !req.is_tls() { return Err(S3Error::InsecureSSECustomerRequest); }`
2. 密钥不持久化 — 仅在内存中保留，解密时需要客户端再次提供相同密钥
3. 密钥长度必须恰好 32 字节 (AES-256)，base64 标准编码
4. Key MD5 校验 (`x-amz-server-side-encryption-customer-key-MD5`) 验证密钥传输完整性

```rust
// base/src/sse.rs
pub struct SseCustomerKey {
    key: [u8; 32],
    key_md5: [u8; 16],
}

impl SseCustomerKey {
    pub fn from_headers(headers: &HeaderMap) -> Result<Option<Self>, S3Error> {
        let key_b64 = match headers.get("x-amz-server-side-encryption-customer-key") {
            Some(v) => v.to_str().map_err(|_| S3Error::InvalidEncryptionMethod)?,
            None => return Ok(None),
        };
        let key = BASE64_STANDARD.decode(key_b64)
            .map_err(|_| S3Error::InvalidEncryptionMethod)?;
        if key.len() != 32 {
            return Err(S3Error::InvalidEncryptionMethod);
        }
        let mut result = [0u8; 32];
        result.copy_from_slice(&key);

        let md5_b64 = headers.get("x-amz-server-side-encryption-customer-key-MD5")
            .ok_or(S3Error::InvalidEncryptionMethod)?;
        // 验证 MD5 匹配
        let computed_md5 = md5::compute(&key);
        if BASE64_STANDARD.encode(computed_md5.0) != md5_b64.to_str().unwrap_or("") {
            return Err(S3Error::BadDigest);
        }
        let mut md5_res = [0u8; 16];
        md5_res.copy_from_slice(&computed_md5.0);
        Ok(Some(SseCustomerKey { key: result, key_md5: md5_res }))
    }
}
```

---

### 6.5 根凭证生成 (无 KMS 时)

**场景**: 部署时未配置外部 KMS，如何安全生成初始管理员凭证。

**处理**: 优先使用环境变量 `MINIO_ROOT_USER` / `MINIO_ROOT_PASSWORD`。如果未设置且存在本地 KMS，生成随机凭证并用 KMS 加密存储。如果两者都不可用，启动失败报错 "root credentials not set"。

---

## 7. 极端文件大小

### 7.1 0 字节对象

**场景**: PUT 空 body，Content-Length: 0。

**处理**: xl.meta 中不存储 Part 数据 (无分片)，仅包含元数据 (MetaSys + MetaUser)。后续 GET 返回空 body + 正确元数据。正常参与版本控制。

```rust
// object/src/erasure_objects.rs
pub async fn put_object(
    &self, bucket: &str, object: &str, data: &[u8], metadata: ObjectMeta,
) -> Result<ObjectInfo, S3Error> {
    let version_id = Uuid::now_v7();
    let parts = if data.is_empty() {
        vec![]  // 0 字节: 无分片
    } else {
        // EC 编码并写入分片
        let shards = self.erasure.encode(data)?;
        self.write_shards(bucket, object, &version_id, &shards).await?;
        vec![PartInfo { number: 1, etag: md5(&data), size: data.len() as u64 }]
    };
    let entry = VersionEntry {
        version_id: version_id.to_string(),
        meta_sys: metadata.system,
        meta_user: metadata.user,
        parts,
        ..Default::default()
    };
    self.write_xl_meta(bucket, object, &[entry]).await?;
    Ok(ObjectInfo { version_id: version_id.to_string(), .. })
}
```

---

### 7.2 小文件 (≤ 128 KiB)

**场景**: 大量小文件 (缩略图、配置文件、JSON)。

**优化**: 数据内联到 xl.meta 的 MetaSys 中，省去 `part.N` 文件创建 + 目录项，减少磁盘 inode 消耗。读取时一次 IO 获取元数据+数据。EC 编码仍然执行以保障持久性。

```rust
// base/src/xl_meta.rs
const INLINE_THRESHOLD: usize = 128 * 1024; // 128 KiB

pub fn should_inline(data_size: usize) -> bool {
    data_size <= INLINE_THRESHOLD
}

// VersionEntry 中的 MetaSys 字段存储内联数据:
// meta_sys["inline-data"] = data (EC 编码后的分片或原始数据)
```

---

### 7.3 大文件 (> 128 MiB)

**场景**: 单对象 100 GiB。

**优化**: 启用预读流水线。EC 编码以 blockSize (默认 1 MiB) 为单位流式处理，不会将整个对象加载到内存。Rust 中使用 `tokio::io::AsyncRead` 流式 encode:

```rust
// erasure/src/stream.rs
pub struct ErasureEncoder<R> {
    reader: R,
    buf: Vec<u8>,
    block_size: usize,
    encoder: ReedSolomon,
}

impl<R: AsyncRead + Unpin> ErasureEncoder<R> {
    pub async fn encode_block(&mut self) -> Option<Vec<Vec<u8>>> {
        self.buf.clear();
        // 读取一个 block (默认 1 MiB)
        let n = read_exact_or_eof(&mut self.reader, &mut self.buf).await;
        if n == 0 { return None; }
        // 填充到 block_size
        self.buf.resize(self.block_size, 0);
        // Reed-Solomon 编码
        let shards = self.encoder.encode(&self.buf).ok()?;
        Some(shards)
    }
}
```

**风险**: EC 编码需要 CPU (每个 Block 的矩阵运算)，大文件修复耗时长。解决方案: 使用对象分段模式代替巨型单文件。

---

### 7.4 超大文件 (> 5 TiB)

**场景**: 超过 Multipart 10000 分片 × 5 GiB 的单 PUT。

**MinIO 限制**: 单 PUT 最大 5 TiB，Multipart 10000 分片 × 5 GiB = 50 TiB。超出返回 `ErrEntityTooLarge` (HTTP 400)。

---

## 8. Multipart Upload 边界

### 8.1 分片数极端值

- **1 个分片**: 正常完成，等价于单 PUT (路径为 multipart → complete)
- **10000 个分片 (上限)**: Parts 数组存储在 xl.meta 中 (MessagePack 编码)，每个分片有独立的 `part.N` 文件。CompleteMultipartUpload 时验证分片号唯一、ETag 匹配。极端情况下 xl.meta 大小 ≤ 1 MB (仍在合理范围)。

---

### 8.2 分片乱序到达

**场景**: 客户端并发上传导致完成顺序 ≠ 分片号顺序。

**处理**: MinIO 不关心上传顺序。每个分片独立存储到 `part.{partNumber}`。ListParts 按分片号排序返回。CompleteMultipartUpload 按客户端 XML 中 `<PartNumber>` 的顺序合并。

---

### 8.3 分片丢失

**场景**: 客户端声称上传了 3 个分片但第 2 个分片写入失败。

**处理**: CompleteMultipartUpload 验证每个 `part.N` 文件存在且 ETag 匹配。缺少分片时返回 `ErrInvalidPart` (HTTP 400)，客户端应重新上传该分片后重试 Complete。

```rust
// s3/src/handlers/multipart.rs
pub async fn complete_multipart_upload(...) -> Result<..., S3Error> {
    let mut expected_parts = parts.parts.clone();
    expected_parts.sort_by_key(|p| p.number);
    for part in &expected_parts {
        let actual = state.object.get_part(
            &bucket, &key, &upload_id, part.number,
        ).await?;
        if actual.etag != part.etag {
            return Err(S3Error::InvalidPart { number: part.number });
        }
    }
    // ... 合并
}
```

---

### 8.4 废弃的 Multipart Upload

**场景**: 客户端初始化 Upload 后崩溃，分片数据残留在磁盘。

**清理**: 主动 AbortMultipartUpload 立即清理。ILM Scanner 周期性扫描，超过配置时间的未完成 Upload 自动清理 (默认 7 天)。大量废弃 Upload 堆积时，通过 `ListMultipartUploads` + `AbortMultipartUpload` 批量清理。

---

### 8.5 最小分片大小违反

**场景**: 客户端上传的分片小于 5 MiB (最后一分片除外)。

**处理**: PutObjectPart 不限制分片大小 (接受所有大小)。CompleteMultipartUpload 不验证分片大小 — 兼容某些 SDK 发送小于 5 MiB 分片的行为。但小于 5 MiB 会产生大量小文件，可能影响性能。

---

## 9. 复制冲突 & 一致性

### 9.1 双向复制冲突

**场景**: 两个集群都开启了到对方的复制规则，同对象同时被修改。

**处理**: MinIO 检测到源和目标的 URL 完全相同时拒绝创建 (`ErrBucketRemoteIdenticalToSource`)。复制拓扑中检测到循环时配置验证不通过。接收到的复制对象带有 `ReplicaStatus` 元数据，不会再被复制回源。

---

### 9.2 复制延迟下的读一致性

**场景**: 对象写入源集群后立即在目标集群读取。

**处理**: Bucket Replication 是异步的 (最终一致性)。MinIO 提供:
- `ReplicationStatus`: PENDING → COMPLETED
- `mc replicate resync`: 手动全量同步
- Batch Replication: 批量补齐

实时性要求高的场景建议使用 Site Replication (同步复制 IAM/Bucket 配置)。

---

### 9.3 复制失败重试

**场景**: 目标集群暂时不可达。

**处理**: 首次失败设置 `ReplicationStatus → FAILED`。MRF (Most Recent Failure) 队列在内存中保存最近失败记录，后台 Worker 指数退避重试: 1s → 2s → 4s → ... → max 15min。持久化失败记录在 `replication-reset.bin`，重启后恢复重试。无上限持续重试直到成功或手动取消。

```rust
// object/src/replication/mrf.rs
pub struct MrfQueue {
    inner: Arc<tokio::sync::Mutex<VecDeque<ReplicationFailure>>>,
}

impl MrfQueue {
    pub async fn push(&self, failure: ReplicationFailure) {
        self.inner.lock().await.push_back(failure);
    }

    pub fn start_worker(self, state: AppState) {
        tokio::spawn(async move {
            let mut backoff = Duration::from_secs(1);
            loop {
                tokio::time::sleep(backoff).await;
                if let Some(failure) = self.inner.lock().await.pop_front() {
                    match retry_replication(&state, &failure).await {
                        Ok(()) => { backoff = Duration::from_secs(1); }
                        Err(_) => {
                            self.inner.lock().await.push_back(failure);
                            backoff = (backoff * 2).min(Duration::from_secs(900)); // max 15min
                        }
                    }
                } else {
                    backoff = Duration::from_secs(1);
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        });
    }
}
```

---

### 9.4 站点复制中的冲突解决

**场景**: 两个站点同时修改同一 IAM 用户。

**处理**: Site Replication 使用时间戳冲突解决。每个操作携带 `UpdatedAt` 时间戳。接收方比较: 本地时间戳 < 远程时间戳时接受更新，本地 > 远程时拒绝 (本地更新更新)，相等时按确定性规则选择 (如按站点名排序)。需要 NTP 时间同步。

```rust
// iam/src/site_replication.rs
pub fn resolve_conflict(local: &UserInfo, remote: &UserInfo) -> ConflictResolution {
    match local.updated_at.cmp(&remote.updated_at) {
        std::cmp::Ordering::Less => ConflictResolution::AcceptRemote,
        std::cmp::Ordering::Greater => ConflictResolution::KeepLocal,
        std::cmp::Ordering::Equal => {
            // 时间戳相同: 按站点名确定性选择
            if local.site_name < remote.site_name {
                ConflictResolution::AcceptRemote
            } else {
                ConflictResolution::KeepLocal
            }
        }
    }
}
```

---

## 10. 解配安全

### 10.1 解配中故障

**场景**: 池 A 正在解配到池 B，池 A 的部分磁盘故障。

**处理**: Decommission 操作逐对象迁移，使用 Version Walk (按版本顺序)。遇到磁盘故障时，若仍有 ReadQuorum 则正常读取并迁移，若 ReadQuorum 丢失则跳过。解配完成后报告成功/失败计数。用户可通过 `mc admin heal` 尝试恢复。

---

### 10.2 解配取消后状态

**场景**: 解配进行到 50% 被取消。

**处理**: 状态变为 `Draining(Canceled)`。池不可回到 Active (已部分迁移)。读取仍可达 (原池数据仍在)，新写入不发送到该池。不可再次开始解配。解决方案: 只能完成解配 (重新开始从断点继续) 或物理移除池。

---

### 10.3 解配过程中版本化 Bucket

**场景**: 版本化对象的旧版本仍在原池，新版本已写入新池。

**处理**: 按 VersionID 时间升序遍历所有版本 (oldest → newest) 迁移。最新版本迁移完成后删除原池数据。迁移过程中对象被更新时，新版本写入新池，迁移器跳过已在新池的版本。

```rust
// object/src/decommission.rs
pub async fn decommission_pool(
    source_pool: &Pool, target_pool: &Pool,
) -> Result<DecommissionResult, S3Error> {
    let mut stats = DecommissionResult::default();
    for bucket in source_pool.list_buckets().await? {
        for object in source_pool.list_objects(&bucket).await? {
            // Version Walk: 从 oldest 到 newest
            let versions = source_pool.list_versions(&bucket, &object).await?;
            for version in &versions {
                if target_pool.has_version(&bucket, &object, &version.id).await? {
                    continue; // 已在新池
                }
                match source_pool.get_version(&bucket, &object, &version.id).await {
                    Ok(data) => {
                        target_pool.put_version(&bucket, &object, &version.id, &data).await?;
                        stats.migrated += 1;
                    }
                    Err(S3Error::SlowDownRead) => {
                        stats.skipped += 1; // Quorum 丢失，跳过
                    }
                    Err(e) => return Err(e),
                }
            }
        }
    }
    Ok(stats)
}
```

---

## 11. Rust 实现优先级矩阵

| 编号 | Case | 优先级 | Phase | 说明 |
|------|------|--------|-------|------|
| 1.1 | 磁盘离线 & Quorum | **P0 必须** | **P1** | EC 核心写入/读取路径，Phase 1 直接依赖 |
| 1.2 | 静默数据损坏 (Bitrot) | **P0 必须** | **P1** | 写入路径即需哈希校验，读取时自动恢复 |
| 1.3 | 磁盘满 (ENOSPC) | **P0 必须** | **P1** | 原子写入模式是基础设计 |
| 1.4 | 磁盘 I/O 错误 (EIO) | **P0 必须** | **P1** | 错误重试与降级策略 |
| 3.1 | 并发写入冲突 (锁) | **P0 必须** | **P1** | 单机也需要锁机制防止数据竞争 |
| 4.2 | N-1 磁盘故障 | **P0 必须** | **P1** | EC 容错核心边界条件 |
| 7.1 | 0 字节对象 | **P0 必须** | **P1** | 基本边界 Case，写入路径必测 |
| 7.2 | 小文件内联 (≤128 KiB) | **P0 必须** | **P1** | 优化存储路径，Phase 1 即应支持 |
| 7.3 | 大文件流式处理 | **P0 必须** | **P1** | 流式 EC 编码是核心能力 |
| 4.1 | 部分修复 (幂等性) | **P1 重要** | **P1** | 修复操作需幂等，写入即原子 |
| 4.4 | 修复时磁盘离线 | **P1 重要** | **P1** | 容错修复逻辑 |
| 3.2 | Versions Journal 并发 | **P1 重要** | **P2** | 版本控制开启后必需 |
| 4.3 | xl.meta 自身损坏 | **P1 重要** | **P2** | 需 xl.meta.bkp 备份机制 |
| 5.1 | V1 → V2 格式迁移 | **P1 重要** | **P2** | 兼容旧格式数据 |
| 5.2 | xl.meta 版本号不匹配 | **P1 重要** | **P2** | 跨版本兼容读取 |
| 8.x | Multipart 边界 (全部) | **P1 重要** | **P2** | Phase 2 Multipart 实现时覆盖 |
| 2.1 | 写 Quorum 丢失 | **P2 尽量** | **P2** | 分布式模式下必需 |
| 2.2 | 读 Quorum 丢失 | **P2 尽量** | **P2** | 分布式读取降级 |
| 2.3 | 节点间 RPC 超时 | **P2 尽量** | **P2** | 分布式通信可靠性 |
| 3.3 | Multipart 并发分片 | **P2 尽量** | **P2** | Phase 2 Multipart 实现 |
| 5.3 | 部署 ID 不匹配 | **P2 尽量** | **P2** | 安全防护 |
| 7.4 | 超大文件 (> 5 TiB) | **P2 尽量** | **P2** | 边界限制校验 |
| 6.1 | 签名时间偏差 | **P3 按需** | **P3** | Phase 3 认证实现时覆盖 |
| 6.2 | JWT 令牌过期 | **P3 按需** | **P3** | STS 实现时必需 |
| 6.3 | LDAP 暴力破解防护 | **P3 按需** | **P3** | LDAP STS 实现时必需 |
| 6.4 | SSE-C 密钥处理 | **P3 按需** | **P3** | 加密功能实现时必需 |
| 6.5 | 根凭证生成 | **P3 按需** | **P3** | 启动流程 + IAM 实现时必需 |
| 9.x | 复制冲突 & 一致性 | **P4 延后** | **P4** | 复制功能实现时覆盖 |
| 10.x | 解配安全 | **P4 延后** | **P4** | 池管理实现时覆盖 |
| 8.4 | 废弃 Multipart 清理 | **P3 按需** | **P4** | ILM 扫描器实现时覆盖 |

### 优先级说明

- **P0 必须**: Phase 1 必须正确处理，否则核心写入/读取路径不可用
- **P1 重要**: 应在对应 Phase 内覆盖，否则功能不完整或存在数据风险
- **P2 尽量**: 分布式或多用户场景下应覆盖
- **P3 按需**: 认证或高级功能实现时自然覆盖
- **P4 延后**: 高级特性的附属 Case

---

> 跨文档参考: API 路由定义见 `./API_REFERENCE.md` | 架构与分层设计见 `./ARCHITECTURE.md` | 分阶段计划见 `./PLAN.md`
