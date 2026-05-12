//! STS tests: temporary security token service

/// Verifies the complete IAM + STS integration test suite.
///
/// Covers: Root authorization, STS normal flow, privilege escalation vulnerability (CVE-2025-10-15),
/// Deny DeleteVersion, Tag conditions, ServiceAccount, Group policy, Token revocation.
#[test]
#[ignore]
fn test_iam_internal_idp_sts_server_suite() {
    // TestIAMInternalIDPSTSServerSuite
    //   iterates over multiple backends (ErasureSD/Erasure/ErasureSet) + signature versions
    //   runAllIAMSTSTests: TestSTSForRoot, TestSTS, TestSTSPrivilegeEscalationBug,
    //   TestSTSWithDenyDeleteVersion, TestSTSWithTags, TestSTSServiceAccountsWithUsername,
    //   TestSTSWithGroupPolicy, TestSTSTokenRevoke
    // TODO: implement when complete IAM + STS subsystem is available
}
