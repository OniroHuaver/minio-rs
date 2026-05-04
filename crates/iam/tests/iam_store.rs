//! IAM 存储测试: etcd store、object store、路径拆分
//!
//! 对应 Go: cmd/iam-etcd-store_test.go, cmd/iam-object-store_test.go

/// 验证 `extractPathPrefixAndSuffix()` 从路径中提取中间部分。
#[test]
#[ignore]
fn test_extract_prefix_and_suffix() {
    // Go: "config/iam/groups/foo.json" + prefix/suffix -> "foo"
    //   "./" 被清理; "/config.json" 作为后缀
    // TODO: implement when extractPathPrefixAndSuffix equivalent is available
}

/// 验证 `splitPath()`: IAM 路径中分离 listKey 和 item。
#[test]
#[ignore]
fn test_split_path() {
    // Go: 各种 IAM 路径（users/、groups/、policydb/、含 LDAP DN 的路径）
    //   secondIndex 控制 / 的分割策略
    // TODO: implement when splitPath equivalent is available
}
