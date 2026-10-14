#[test]
fn public_core_values() {
    let v = parse_version("12.34.56").unwrap();
    assert_eq!((v.major, v.minor, v.patch), (12,34,56));
}
#[test]
fn public_core_limits() {
    assert_eq!(parse_version("0.0.0").unwrap(), Version { major: 0, minor: 0, patch: 0 });
    assert_eq!(parse_version("18446744073709551615.1.2").unwrap().major, u64::MAX);
}
#[test]
fn public_core_count_error() {
    assert_eq!(parse_version("1.2"), Err(ParseError::ComponentCount { found: 2 }));
}
#[test]
fn public_core_digit_error() {
    assert_eq!(parse_version("1.2.a"), Err(ParseError::InvalidDigit { index: 2, offset: 0 }));
}
