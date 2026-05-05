//! AWS time format tests
//!
//! Tests ISO8601 formatting and time parsing.

/// Test ISO8601 formatting (with milliseconds)
#[test]
#[ignore]
fn test_iso8601_format() {
    // TODO: implement when ISO8601Format function available
    //
    // Steps:
    //   time::Date(2009, 11, 13, 4, 51, 1, 940303531, UTC)
    //     -> "2009-11-13T04:51:01.940Z"
    //   time::Date(2009, 11, 13, 4, 51, 1, 901303531, UTC)
    //     -> "2009-11-13T04:51:01.901Z"
    //   time::Date(2009, 11, 13, 4, 51, 1, 900303531, UTC)
    //     -> "2009-11-13T04:51:01.900Z"
    //   time::Date(2009, 11, 13, 4, 51, 1, 941303531, UTC)
    //     -> "2009-11-13T04:51:01.941Z"
    //
    // Key: nanosecond -> millisecond (truncation, not rounding), trailing "Z"
}

/// Test Parse for AWS time format
#[test]
#[ignore]
fn test_amztime_parse() {
    // TODO: implement when amztime::Parse function available
    //
    // Steps:
    //   "Tue Sep  6 07:10:23 PM PDT 2022" -> ErrMalformedDate
    //   "Tue, 10 Nov 2009 23:00:00 UTC" -> time::Date(2009, 11, 10, 23, 0, 0, 0, UTC)
}
