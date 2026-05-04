//! 冷热分层测试: Tier 配置管理、Metrics
//!
//! 对应 Go: cmd/tier_gen_test.go, cmd/tier_test.go,
//!          cmd/tier-last-day-stats_gen_test.go

/// 验证 TierConfigMgr 的 Marshal/Unmarshal 往返 (MessagePack)。
#[test]
#[ignore]
fn test_marshal_unmarshal_tier_config_mgr() {
    // Go: TierConfigMgr -> MarshalMsg -> UnmarshalMsg -> 相等
    // TODO: implement when TierConfigMgr + msgpack is available
}

/// 验证 TierConfigMgr 的 Encode/Decode 往返。
#[test]
#[ignore]
fn test_encode_decode_tier_config_mgr() {
    // Go: msgp.Encode -> Decode -> 相等
    // TODO: implement when TierConfigMgr + msgpack is available
}

/// 验证 Tier Metrics 观测。
#[test]
#[ignore]
fn test_tier_metrics() {
    // Go: globalTierMetrics.Observe(tier, duration)
    //   验证 success/failure 计数正确
    // TODO: implement when tier metrics are available
}
