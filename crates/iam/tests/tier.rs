//! Tier lifecycle tests: Tier config management, Metrics

/// Verifies TierConfigMgr Marshal/Unmarshal round-trip (MessagePack).
#[test]
#[ignore]
fn test_marshal_unmarshal_tier_config_mgr() {
    // TierConfigMgr -> MarshalMsg -> UnmarshalMsg -> equal
    // TODO: implement when TierConfigMgr + msgpack is available
}

/// Verifies TierConfigMgr Encode/Decode round-trip.
#[test]
#[ignore]
fn test_encode_decode_tier_config_mgr() {
    // msgp.Encode -> Decode -> equal
    // TODO: implement when TierConfigMgr + msgpack is available
}

/// Verifies Tier Metrics observation.
#[test]
#[ignore]
fn test_tier_metrics() {
    // globalTierMetrics.Observe(tier, duration)
    //   verify success/failure counts
    // TODO: implement when tier metrics are available
}
