//! Site Replication tests: site departure detection, Metrics MessagePack, Utils MessagePack

/// Verifies `getMissingSiteNames()` detects sites that left replication.
///
/// Covers: partial site departure, new sites added, non-replication state.
#[test]
#[ignore]
fn test_get_missing_site_names() {
    // 3 cases:
    //   1. partial sites missing -> return missing names
    //   2. new site added -> return empty
    //   3. replication not enabled -> return empty
    // TODO: implement when site replication is available
}

/// Verifies RStat Marshal/Unmarshal round-trip (MessagePack).
#[test]
#[ignore]
fn test_marshal_unmarshal_r_stat() {
    // RStat -> MarshalMsg -> UnmarshalMsg -> equal
    // TODO: implement when RStat + msgpack is available
}

/// Verifies SiteResyncStatus Marshal/Unmarshal round-trip (MessagePack).
#[test]
#[ignore]
fn test_marshal_unmarshal_site_resync_status() {
    // SiteResyncStatus -> MarshalMsg -> UnmarshalMsg -> equal
    // TODO: implement when SiteResyncStatus + msgpack is available
}
