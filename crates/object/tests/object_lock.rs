//! Object lock tests
//!
//! Tests object lock configuration parsing, retention modes, legal hold status etc.

/// Verifies ParseMode function.
///
/// Tests parsing of "governance", "complIAnce", "gce" (invalid) etc. retention mode strings.
#[test]
#[ignore]
// TODO: implement when object lock types are available
fn test_parse_mode() {
    // assert_eq!(parse_ret_mode("governance"), RetMode::Governance);
    // assert_eq!(parse_ret_mode("complIAnce"), RetMode::Compliance); // case insensitive
    // assert_eq!(parse_ret_mode("gce"), RetMode::default()); // invalid
}

/// Verifies ParseLegalHoldStatus.
#[test]
#[ignore]
// TODO: implement when object lock types are available
fn test_parse_legal_hold_status() {
    // assert_eq!(parse_legal_hold_status("ON"), LegalHoldStatus::On);
    // assert_eq!(parse_legal_hold_status("Off"), LegalHoldStatus::Off); // case insensitive
    // assert_eq!(parse_legal_hold_status("x"), LegalHoldStatus::default());
}

/// Verifies DefaultRetention XML serialization/deserialization roundtrip.
///
/// Tests various DefaultRetention configurations:
/// - Unknown mode
/// - Missing Days/Years
/// - Valid Days/Years
/// - Both Days and Years specified (invalid)
/// - Days=0 (invalid)
/// - Days exceeds maximum (invalid)
#[test]
#[ignore]
// TODO: implement when object lock types are available
fn test_unmarshal_default_retention() {
    // let test_cases = vec![
    //     (DefaultRetention { mode: "retain".into(), days: None, years: None }, true),
    //     (DefaultRetention { mode: RetGovernance.into(), days: None, years: None }, true),
    //     (DefaultRetention { mode: RetGovernance.into(), days: Some(4), years: None }, false),
    //     (DefaultRetention { mode: RetGovernance.into(), days: None, years: Some(1) }, false),
    //     (DefaultRetention { mode: RetGovernance.into(), days: Some(4), years: Some(1) }, true),
    //     (DefaultRetention { mode: RetGovernance.into(), days: Some(0), years: None }, true),
    // ];
    // for (dr, expect_err) in test_cases {
    //     let xml = quick_xml::to_string(&dr).unwrap();
    //     let result: Result<DefaultRetention, _> = quick_xml::from_str(&xml);
    //     assert_eq!(result.is_err(), expect_err, "test: {dr:?}");
    // }
}

/// Verifies ParseObjectLockConfig.
///
/// Tests object lock configuration XML parsing:
/// - Invalid ObjectLockEnabled value ("yes")
/// - Days=0
/// - Valid configuration
#[test]
#[ignore]
// TODO: implement when object lock types are available
fn test_parse_object_lock_config() {
    // // "yes" -> error
    // // Days=0 -> error
    // // valid -> ok
}

/// Verifies ParseObjectRetention.
///
/// Tests object retention XML parsing:
/// - Unknown mode
/// - Expiry date
/// - Valid GOVERNANCE/COMPLIANCE
/// - Millisecond precision date
#[test]
#[ignore]
// TODO: implement when object lock types are available
fn test_parse_object_retention() {
    // // "string" mode -> ErrUnknownWORMModeDirective
    // // Past date -> ErrPastObjectLockRetainDate
    // // Future date GOVERNANCE -> ok
    // // With milliseconds -> ok
}

/// Verifies IsObjectLockRequested.
#[test]
#[ignore]
// TODO: implement when object lock header helpers are available
fn test_is_object_lock_requested() {
    // // No lock header -> false
    // // With LegalHold header -> true
    // // With RetainUntilDate + Mode -> true
    // // Only BypassGovernance -> false
}

/// Verifies IsObjectLockGovernanceBypassSet.
#[test]
#[ignore]
// TODO: implement when object lock header helpers are available
fn test_is_object_lock_governance_bypass_set() {
    // // Empty value -> false
    // // "true" -> true
    // // Other headers -> false
}

/// Verifies ParseObjectLockRetentionHeaders.
///
/// Tests parsing object retention parameters from HTTP headers:
/// - Missing headers
/// - Unknown mode
/// - Missing date
/// - Invalid date format
/// - Past date
/// - Valid future date
/// - Millisecond precision date
#[test]
#[ignore]
// TODO: implement when object lock header helpers are available
fn test_parse_object_lock_retention_headers() {
    // for (i, (headers, expected_err)) in test_cases.iter().enumerate() {
    //     let result = parse_object_lock_retention_headers(headers);
    //     match (result, expected_err) {
    //         (Ok(_), None) => {},
    //         (Err(e), Some(expected)) => assert_eq!(e.to_string(), expected.to_string(), "case {i}"),
    //         _ => panic!("unexpected case {i}"),
    //     }
    // }
}

/// Verifies GetObjectRetentionMeta.
#[test]
#[ignore]
// TODO: implement when object lock meta helpers are available
fn test_get_object_retention_meta() {
    // // No lock metadata -> empty
    // // Only Mode -> Mode correct
    // // Only date -> date correct
}

/// Verifies GetObjectLegalHoldMeta.
#[test]
#[ignore]
// TODO: implement when object lock meta helpers are available
fn test_get_object_legal_hold_meta() {
    // // No LegalHold -> empty
    // // ON -> LegalHoldOn
    // // OFF -> LegalHoldOff
    // // Invalid -> empty
}

/// Verifies ParseObjectLegalHold.
#[test]
#[ignore]
// TODO: implement when object lock types are available
fn test_parse_object_legal_hold() {
    // // "string" status -> ErrMalformedXML
    // // "ON" -> ok
    // // ObjectLockLegalHold tag -> ok
    // // Invalid tag -> error
    // // Invalid case "On" -> ErrMalformedXML
}

/// Verifies FilterObjectLockMetadata.
///
/// Filters object lock metadata based on filterRetention/filterLegalHold flags.
#[test]
#[ignore]
// TODO: implement when object lock meta helpers are available
fn test_filter_object_lock_metadata() {
    // let test_cases = vec![
    //     (map!{"Authorization" => "..."}, false, false, map!{"Authorization" => "..."}),
    //     (map!{"x-amz-object-lock-mode" => "governance"}, false, false,
    //      map!{"x-amz-object-lock-mode" => "governance"}),
    //     (map!{"x-amz-object-lock-mode" => "gov", "x-amz-object-lock-retain-until-date" => "2020-02-01"},
    //      true, false, map!{}),
    //     (map!{"x-amz-object-lock-legal-hold" => "off"}, false, true, map!{}),
    //     (map!{"x-amz-object-lock-legal-hold" => "on"}, false, false,
    //      map!{"x-amz-object-lock-legal-hold" => "on"}),
    //     (map!{"x-amz-object-lock-legal-hold" => "on", "x-amz-object-lock-mode" => "gov",
    //           "x-amz-object-lock-retain-until-date" => "2020-02-01"},
    //      false, false,
    //      map!{"x-amz-object-lock-legal-hold" => "on", "x-amz-object-lock-mode" => "gov",
    //            "x-amz-object-lock-retain-until-date" => "2020-02-01"}),
    // ];
    // for (i, (metadata, filter_ret, filter_lh, expected)) in test_cases.iter().enumerate() {
    //     let result = filter_object_lock_metadata(metadata.clone(), *filter_ret, *filter_lh);
    //     assert_eq!(result, *expected, "case {i}");
    // }
}

/// Verifies Config Display implementation.
#[test]
#[ignore]
// TODO: implement when object lock config types are available
fn test_object_lock_config_to_string() {
    // assert_eq!(Config { object_lock_enabled: "Enabled".into(), .. }.to_string(), "Enabled: true");
    // assert_eq!(Config { object_lock_enabled: "Disabled".into(), .. }.to_string(), "Enabled: false");
    // // With default retention
}
