//! FreeVersion (TierFreeVersion) tests
//!
//! Tests free version creation, management, listing, and cleanup
//! in tiered/transitioned scenarios.


/// Tests FreeVersion full lifecycle
///
/// Scenarios:
/// 1. Add normal version (local content)
/// 2. Add null version and transition to cold storage
/// 3. Overwrite null version -> produces free version (fv1)
/// 4. Delete transitioned version -> produces free version (fv2)
/// 5. Verify free version list contains 2 versions
/// 6. ToFileInfo inclFreeVers=true -> returns latest non-free version
/// 7. After deleting all non-free versions, inclFreeVers=true -> returns free version
/// 8. ToFileInfo inclFreeVers=false -> errFileNotFound
/// 9. Cleanup free versions -> list is empty
/// 10. Add free version to non-tiered version -> should not take effect
#[test]
#[ignore]
fn test_free_version() {
    // TODO: implement when xlMetaV2 with free version support is available
    // let mut xl = xlMetaV2::new();
    // let base_fi = FileInfo {
    //     volume: "volume".into(),
    //     name: "object-name".into(),
    //     version_id: "00000000-0000-0000-0000-000000000001".into(),
    //     is_latest: true,
    //     data_dir: "bffea160-ca7f-465f-98bc-9b4f1c3ba1ef".into(),
    //     ..Default::default()
    // };
    //
    // // Add version with local content
    // xl.add_version(&base_fi).unwrap();
    //
    // // Add null version and transition it
    // let mut tier_fi = base_fi.clone();
    // tier_fi.version_id = String::new();
    // xl.add_version(&tier_fi).unwrap();
    // tier_fi.transition_status = "COMPLETE".into();
    // tier_fi.transitioned_obj_name = uuid::Uuid::new_v4().to_string();
    // tier_fi.transition_tier = "MINIOTIER-1".into();
    // xl.delete_version(&tier_fi).unwrap();
    //
    // // Overwrite null version → free version
    // let fv_ids = vec!["00000000-0000-0000-0000-0000000000f1", "00000000-0000-0000-0000-0000000000f2"];
    // let mut new_tier_fi = tier_fi.clone();
    // new_tier_fi.set_tier_free_version_id(fv_ids[0]);
    // xl.add_free_version(&new_tier_fi).unwrap();
    // xl.add_version(&new_tier_fi).unwrap();
    //
    // // Remove null version
    // new_tier_fi.set_tier_free_version_id(fv_ids[1]);
    // xl.add_free_version(&new_tier_fi).unwrap();
    // // ... test all scenarios
    //
    // // List free versions → should be 2
    // let free = xl.list_free_versions("volume", "object-name").unwrap();
    // assert_eq!(free.len(), 2);
    //
    // // ... additional assertions
}

/// Tests SkipFreeVersion - skip free version creation scenarios
///
/// Scenarios:
/// - Normal Tiers parameter -> InitFreeVersion should create free version
/// - SkipTierFreeVersion set -> InitFreeVersion should skip creation
#[test]
#[ignore]
fn test_skip_free_version() {
    // TODO: implement when InitFreeVersion is available
    // let fi = FileInfo {
    //     volume: "volume".into(),
    //     name: "object-name".into(),
    //     version_id: "00000000-0000-0000-0000-000000000001".into(),
    //     ..Default::default()
    // };
    // fi.set_tier_free_version_id(uuid::Uuid::new_v4().to_string());
    //
    // let mut j = xlMetaV2Object::default();
    // j.meta_sys = HashMap::from([
    //     ("x-minio-internal-tier-name".into(), b"WARM-1".to_vec()),
    //     ("x-minio-internal-tier-status".into(), b"COMPLETE".to_vec()),
    //     ("x-minio-internal-tier-obj-name".into(), b"obj-1".to_vec()),
    // ]);
    //
    // // Should create free version
    // let (_, ok) = j.init_free_version(&fi);
    // assert!(ok, "Expected free version to be created");
    //
    // // With SkipTier set, should skip
    // fi.set_skip_tier_free_version();
    // let (_, ok) = j.init_free_version(&fi);
    // assert!(!ok, "Expected no free version to be created");
}
