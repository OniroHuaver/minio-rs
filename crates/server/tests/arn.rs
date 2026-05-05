//! ARN (Amazon Resource Name) tests
//!
//! Tests ARN construction, parsing, and string formatting.
//! Currently Phase 1 placeholder.

/// Test ARN::String() formatting
#[test]
#[ignore]
fn test_arn_string() {
    // TODO: implement when ARN type available
    //
    // Steps:
    //   ARN{Partition:"minio", Service:"iam", Region:"us-east-1",
    //       ResourceType:"role", ResourceID:"my-role"}
    //     -> "arn:minio:iam:us-east-1::role/my-role"
    //   Empty Service -> "arn:minio::us-east-1::role/my-role"
}

/// Test NewIAMRoleARN creates IAM Role ARN
#[test]
#[ignore]
fn test_new_iam_role_arn() {
    // TODO: implement when NewIAMRoleARN available
    //
    // Steps (5 test cases):
    //   ("my-role", "us-east-1") -> ARN{...}, ok
    //   ("-my-role", "us-east-1") -> ok
    //   ("my-role", "") -> ok (empty region)
    //   ("", "us-east-1") -> error "empty resource ID"
    //   ("=", "us-east-1") -> error "invalid resource ID"
}

/// Test Parse for ARN string
#[test]
#[ignore]
fn test_arn_parse() {
    // TODO: implement when Parse function available
    //
    // Steps (6 test cases):
    //   "arn:minio:iam:us-east-1::role/my-role" -> ok
    //   "arn:minio:iam:us-east-1::role/-my-role" -> ok
    //   "arn:minio:" -> error (invalid length)
    //   "arn:invalid:iam:..." -> error (invalid partition)
    //   "arn:minio:invalid:..." -> error (invalid service)
    //   "arn:minio:iam:us-east-1::invalid" -> error (invalid resource type)
}
