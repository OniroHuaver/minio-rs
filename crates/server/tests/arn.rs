//! ARN (Amazon Resource Name) 测试
//!
//! 对应 Go: internal/arn/arn_test.go
//!
//! 测试 ARN 的构造、解析、字符串格式化。
//! 当前 Phase 1 仅作占位。

/// 测试 ARN.String() 格式化
///
/// Go: TestARN_String
#[test]
#[ignore]
fn test_arn_string() {
    // TODO: implement when ARN type available
    //
    // Go 逻辑:
    //   ARN{Partition:"minio", Service:"iam", Region:"us-east-1",
    //       ResourceType:"role", ResourceID:"my-role"}
    //     → "arn:minio:iam:us-east-1::role/my-role"
    //   空 Service → "arn:minio::us-east-1::role/my-role"
}

/// 测试 NewIAMRoleARN 创建 IAM Role ARN
///
/// Go: TestNewIAMRoleARN
#[test]
#[ignore]
fn test_new_iam_role_arn() {
    // TODO: implement when NewIAMRoleARN available
    //
    // Go 逻辑 (5 test cases):
    //   ("my-role", "us-east-1") → ARN{...}, ok
    //   ("-my-role", "us-east-1") → ok
    //   ("my-role", "") → ok (空 region)
    //   ("", "us-east-1") → error "empty resource ID"
    //   ("=", "us-east-1") → error "invalid resource ID"
}

/// 测试 Parse 解析 ARN 字符串
///
/// Go: TestParse
#[test]
#[ignore]
fn test_arn_parse() {
    // TODO: implement when Parse function available
    //
    // Go 逻辑 (6 test cases):
    //   "arn:minio:iam:us-east-1::role/my-role" → ok
    //   "arn:minio:iam:us-east-1::role/-my-role" → ok
    //   "arn:minio:" → error (invalid length)
    //   "arn:invalid:iam:..." → error (invalid partition)
    //   "arn:minio:invalid:..." → error (invalid service)
    //   "arn:minio:iam:us-east-1::invalid" → error (invalid resource type)
}
