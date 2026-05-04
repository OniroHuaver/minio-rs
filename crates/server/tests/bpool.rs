//! 字节池测试
//!
//! 对应 Go: internal/bpool/bpool_test.go
//!
//! 测试 BytePoolCap 固定大小字节缓冲池。

/// 测试 BytePool 功能
///
/// Go: TestBytePool
#[test]
#[ignore]
fn test_byte_pool() {
    // TODO: implement when BytePoolCap type available
    //
    // Go 逻辑:
    //   NewBytePoolCap(size=4, width=1024, capWidth=2048)
    //
    //   1. Width() == 1024
    //   2. WidthCap() == 2048
    //   3. Get() → len=1024, cap=2048
    //   4. Put → pool 回收
    //   5. 填充超出 size × 2 个 buffer → Get 仍正常
    //   6. len(c) == size (pool 不超额)
    //   7. 尝试 Put 容量不匹配的 buffer → 被拒绝 (len(c) == 0)
    //   8. Put 短 slice ([:2]) 但 cap 足够 → 被接受, Get 后 len 恢复
    //   9. close(c) 正常
}
