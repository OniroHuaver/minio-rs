//! IAM storage tests: etcd store, object store, path splitting

/// Verifies `extractPathPrefixAndSuffix()` extracts middle segment from path.
#[test]
#[ignore]
fn test_extract_prefix_and_suffix() {
    // "config/iam/groups/foo.json" + prefix/suffix -> "foo"
    // "./" stripped; "/config.json" as suffix
    // TODO: implement when extractPathPrefixAndSuffix equivalent is available
}

/// Verifies `splitPath()` separates listKey and item from IAM paths.
#[test]
#[ignore]
fn test_split_path() {
    // various IAM paths (users/, groups/, policydb/, with LDAP DN)
    // secondIndex controls / splitting strategy
    // TODO: implement when splitPath equivalent is available
}
