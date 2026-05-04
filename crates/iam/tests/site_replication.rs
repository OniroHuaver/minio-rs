//! Site Replication 测试: 站点复制检测、Metrics MessagePack、Utils MessagePack
//!
//! 对应 Go: cmd/site-replication_test.go,
//!          cmd/site-replication-metrics_gen_test.go,
//!          cmd/site-replication-utils_gen_test.go

/// 验证 `getMissingSiteNames()`: 检测已退出复制的站点。
///
/// 覆盖: 部分站点缺失、新增站点、非复制状态。
#[test]
#[ignore]
fn test_get_missing_site_names() {
    // Go: 3 个 case:
    //   1. 部分站点缺失 -> 返回缺失名
    //   2. 新增站点 -> 返回空
    //   3. 未启用复制 -> 返回空
    // TODO: implement when site replication is available
}

/// 验证 RStat 的 Marshal/Unmarshal 往返 (MessagePack)。
#[test]
#[ignore]
fn test_marshal_unmarshal_r_stat() {
    // Go: RStat -> MarshalMsg -> UnmarshalMsg -> 相等
    // TODO: implement when RStat + msgpack is available
}

/// 验证 SiteResyncStatus 的 Marshal/Unmarshal 往返 (MessagePack)。
#[test]
#[ignore]
fn test_marshal_unmarshal_site_resync_status() {
    // Go: SiteResyncStatus -> MarshalMsg -> UnmarshalMsg -> 相等
    // TODO: implement when SiteResyncStatus + msgpack is available
}
