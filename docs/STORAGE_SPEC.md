# minio-rs 存储格式规格

> 基于 MinIO XL Storage Format V2 (Major=1, Minor=3)
> 参考：原 Go 版 `cmd/xl-storage-format-v2.go`

---

## 1. xl.meta 二进制格式

### 1.1 Header (8 字节)

```
Offset  Size  Field
0       4     Magic    "XL2 "
4       2     Major    1 (big-endian)
6       2     Minor    3 (big-endian)
```

Rust 实现：`core::format::XlMetaHeader`

### 1.2 Body — MessagePack Array

Header 之后是一个 MessagePack 数组，包含任意数量的版本条目。

每个条目的 MessagePack 结构：

```rust
// Type 1: Object (Version Entry)
{
  "Type":          1,
  "VersionID":     "uuid-string",
  "ModTime":       1700000000000000000i64,  // Unix 纳秒
  "Signature":     [u8; 32],
  "Flags":         0u8,
  "ErasureAlgorithm": 0u8,
  "ErasureM":      4u16,
  "ErasureN":      2u16,
  "ErasureBlockSize": 4194304i64,
  "ErasureDist":   [u8],
  "Parts": [
    {
      "Number":     1u32,
      "ETag":       "d41d8cd98f00b204e9800998ecf8427e",
      "Size":       10485760i64,
      "ActualSize": 10485760i64,
      "Index":      0i32
    }
  ],
  "MetaSys":  {},   // Map<String, Vec<u8>>
  "MetaUser": {},    // Map<String, Vec<u8>>
  // 小文件 (<128KiB) 附加字段:
  "Data":     [u8]   // 内联数据
}

// Type 2: DeleteMarker
{
  "Type":      2,
  "VersionID": "uuid-string",
  "ModTime":   1700000000000000000i64,
  "Signature": [u8; 32],
  "Flags":     0u8
}

// Type 3: Legacy (V1 占位)
{ "Type": 3 }
```

### 1.3 版本签名机制

每个 Version Entry 有一个 `Signature` 字段，是**跨所有磁盘无关字段的确定性哈希**：

**包含在签名计算中**：
- VersionID, ModTime
- Type, Flags
- ErasureAlgorithm, ErasureM, ErasureN, ErasureBlockSize, ErasureDist
- Parts 数组 (全部字段)

**不包含在签名计算中**：
- 磁盘特定偏移
- 文件路径
- 节点标识

**用途**：读取时收集多盘签名 → 多数签名视为正确 → 用正确签名修复不一致磁盘。

---

## 2. 磁盘目录布局

```
disk/
  .minio.sys/
    config/
      format.json              ← 磁盘格式化信息
    tmp/                        ← 临时文件，完成前存于此，原子 Rename 到目标
    multipart/                  ← Multipart upload 的中间分片
  {bucket}/                     ← 每个 S3 Bucket 一个目录
    {object}/                   ← 对象名为目录名 (URL encoded)
      xl.meta                   ← MessagePack 二进制版本日志
      xl.meta.bkp               ← 写入前备份 (原子 Rename 失败恢复)
      {version-uuid}/           ← 每个版本一个子目录
        part.1                  ← EC 编码后的数据分片
        part.2
        ...
      legacy/                   ← V1 格式遗留数据 (从 xl.json 迁移)
        part.1
```

### 2.1 format.json 示例

```json
{
  "version": "1",
  "format": "xl",
  "id": "uuid-of-disk",
  "xl": {
    "version": "3",
    "this": "uuid-of-this-node",
    "sets": [
      ["uuid-disk1", "uuid-disk2", "uuid-disk3", "uuid-disk4"],
      ...
    ],
    "distributionAlgo": "SIPMOD+PARITY"
  }
}
```

---

## 3. EC 分片布局

### 3.1 分片文件名

```
{version-uuid}/part.{1..(M+N)}
```

例如：M=4, N=2 时：
```
{version-uuid}/part.1  ← Data shard 0
{version-uuid}/part.2  ← Data shard 1
{version-uuid}/part.3  ← Data shard 2
{version-uuid}/part.4  ← Data shard 3
{version-uuid}/part.5  ← Parity shard 0
{version-uuid}/part.6  ← Parity shard 1
```

### 3.2 分片大小计算

```rust
let total_size: i64 = object_data.len();
let shard_size = (total_size + data_blocks - 1) / data_blocks;
// 每个分片 = shard_size 字节
// 最后一个数据分片可能不满 shard_size，用 0 填充
```

### 3.3 小文件优化

当 `total_size < SMALL_FILE_THRESHOLD (128 KiB)`：
- 数据不写独立 `part.N` 文件
- 直接内联到 `xl.meta` 的 `Data` 字段
- 大幅减少小文件的 IOPS

---

## 4. 写入流程

```
1. 分配版本 UUID (v7)
2. 计算 EC 参数 (M, N)
3. Encode: 数据 → M+N 个 shard
4. 构造 xl.meta (包含所有版本条目)
5. 创建 {version-uuid}/ 目录
6. 并行写入 M+N 个 part.N 文件
7. 写入 xl.meta.bkp (备份)
8. Atomic Rename: xl.meta.bkp → xl.meta
9. 检查 WriteQuorum (成功磁盘数 ≥ M+1)
10. 返回成功
```

**关键安全保证**：xl.meta 的写入在数据分片写入之后，且通过备份+原子 Rename 确保 xl.meta 不会被写坏。

---

## 5. 读取流程

```
1. 读取 xl.meta (并行从所有在线磁盘)
2. 比对各盘 Signature → 确定正确版本
3. 修复不一致磁盘 (将正确版本复制到错误磁盘)
4. 选取最新版本条目 (按 ModTime)
5. 并行读取 M+N 个 part.N 文件
6. 如果在线分片数 ≥ ReadQuorum:
     Decode: 可用分片 → 原始数据
     返回数据
7. 如果在线分片数 < ReadQuorum:
     返回 InsufficientReadQuorum 错误
```

---

## 6. 常量速查

| 常量 | 值 | 说明 |
|------|-----|------|
| `XL_HEADER_MAGIC` | `b"XL2 "` | xl.meta 魔数 |
| `XL_VERSION_MAJOR` | `1` | 格式主版本 |
| `XL_VERSION_MINOR` | `3` | 格式次版本 |
| `XL_META_FILE` | `"xl.meta"` | 版本日志文件名 |
| `XL_META_BACKUP_FILE` | `"xl.meta.bkp"` | 备份文件名 |
| `SMALL_FILE_THRESHOLD` | `131072` (128 KiB) | 小文件内联阈值 |
| `BIG_FILE_THRESHOLD` | `134217728` (128 MiB) | 大文件预读阈值 |
| `DEFAULT_BLOCK_SIZE` | `4194304` (4 MiB) | EC 默认块大小 |
| `MINIO_SYS_DIR` | `".minio.sys"` | 系统配置目录 |

---

## 7. MessagePack 类型映射

| Rust 类型 | MessagePack 类型 | 用途 |
|-----------|-----------------|------|
| `u8` | positive fixint / uint 8 | Type, Flags |
| `u16` | uint 16 | ErasureM, ErasureN |
| `u32` | uint 32 | Part Number |
| `i32` | int 32 | Part Index |
| `i64` | int 64 | ModTime, Size, ErasureBlockSize |
| `String` | str 8/16 | VersionID, ETag, 元数据键 |
| `Vec<u8>` | bin 8/16 | Signature, 元数据值, Data |
| `Vec<(K,V)>` | map | MetaSys, MetaUser |
| `Vec<Part>` | array | Parts |

---

## 8. 与 Go 原版的实现差异

| 项 | Go 原版 | Rust 版 |
|----|---------|---------|
| xl.meta 读写 | `msgp.Unmarshal/Marshal` | `rmp-serde` (MessagePack Serde) |
| 整数字节序 | Little-endian (Go 默认) | Big-endian (Header 字段明确) |
| 版本条目顺序 | 从旧到新 | 从旧到新 (保持一致) |
| 签名计算 | SHA256 + 自定义 Canonicalize | SHA256 + 同样排序 |
