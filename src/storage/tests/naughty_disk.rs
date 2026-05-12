//! naughtyDisk simulated error disk tests
//!
//! naughtyDisk is a StorageAPI wrapper that allows developers to inject
//! errors at specific call counts, used to simulate disk errors that
//! are hard to reproduce in practice.


/// Tests naughtyDisk error injection mechanism
///
/// Verify:
/// - naughtyDisk calc_error returns programmed errors by call count
/// - Returns default_err when no programmed error matches
/// - Returns Ok when no default_err is set
///
/// Note: naughtyDisk itself is a test helper. This test verifies
/// that naughtyDisk behaves correctly.
#[test]
#[ignore]
fn test_naughty_disk_error_injection() {
    // TODO: implement when naughtyDisk wrapper is available
    // let real_disk = new_local_xl_storage(tmp_dir).unwrap();
    //
    // // Program errors: call 1 returns Err, call 3 returns Err
    // let mut programmed = HashMap::new();
    // programmed.insert(1, Error::DiskNotFound);
    // programmed.insert(3, Error::VolumeNotFound);
    //
    // let naughty = NaughtyDisk::new(real_disk, programmed, None);
    //
    // // Call 1: programmed error
    // let result = naughty.is_online();
    // assert_eq!(result, false); // or whatever is_online returns on error
    //
    // // Call 2: no error (not programmed, no default)
    // let result = naughty.is_online();
    // // should delegate to real disk
    //
    // // Call 3: programmed error
    // // should fail with VolumeNotFound
}
