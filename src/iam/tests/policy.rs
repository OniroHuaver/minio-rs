//! IAM policy tests: BucketPolicy evaluation, conversion

/// Verifies BucketPolicy.IsAllowed() permission evaluation.
///
/// Covers: anonymous + allow action, anonymous + resource match,
/// anonymous + deny action, Owner bypass.
#[test]
#[ignore]
fn test_policy_sys_is_allowed() {
    // construct BucketPolicy with GetBucketLocation + PutObject allow statements
    //   test IsAllowed for anonymous/owner combinations
    // TODO: implement when BucketPolicy type is available
}

/// Verifies PolicyToBucketAccessPolicy() conversion (internal format -> minio-go format).
#[test]
#[ignore]
fn test_policy_to_bucket_access_policy() {
    // BucketPolicy -> BucketAccessPolicy
    //   case1: standard -> read access; case2: empty statement; case3: invalid version -> error
    // TODO: implement when PolicyToBucketAccessPolicy equivalent is available
}

/// Verifies BucketAccessPolicyToPolicy() conversion (minio-go format -> internal format).
#[test]
#[ignore]
fn test_bucket_access_policy_to_policy() {
    // BucketAccessPolicy -> BucketPolicy
    //   case1: standard; case2: empty; case3: invalid version -> error
    // TODO: implement when BucketAccessPolicyToPolicy equivalent is available
}
