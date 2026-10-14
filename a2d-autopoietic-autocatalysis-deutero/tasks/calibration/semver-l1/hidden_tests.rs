#[test]
fn hidden_core_empty() { assert_eq!(parse_version(""), Err(ParseError::EmptyInput)); }
#[test]
fn hidden_core_counts() {
    for (s,n) in [("7",1),("2.4.6.8",4),("..0.",4),("bad",1)] {
        assert_eq!(parse_version(s), Err(ParseError::ComponentCount { found:n }));
    }
}
#[test]
fn hidden_core_empty_parts() {
    for (s,i) in [(".8.9",0),("7..9",1),("7.8.",2),("..",0)] {
        assert_eq!(parse_version(s), Err(ParseError::EmptyComponent { index:i }));
    }
}
#[test]
fn hidden_core_zero_major() { assert_eq!(parse_version("01.3.4"), Err(ParseError::LeadingZero { index:0 })); }
#[test]
fn hidden_core_zero_minor_patch() {
    assert_eq!(parse_version("3.00.4"), Err(ParseError::LeadingZero { index:1 }));
    assert_eq!(parse_version("3.4.0002"), Err(ParseError::LeadingZero { index:2 }));
}
#[test]
fn hidden_core_overflow_each() {
    for (s,i) in [("18446744073709551616.0.0",0),("9.18446744073709551616.0",1),("9.8.18446744073709551616",2)] {
        assert_eq!(parse_version(s), Err(ParseError::Overflow { index:i }));
    }
}
#[test]
fn hidden_core_long_overflow() { assert_eq!(parse_version(&format!("6.7.{}", "9".repeat(1000))), Err(ParseError::Overflow { index:2 })); }
#[test]
fn hidden_core_no_trimming() {
    assert_eq!(parse_version(" 4.5.6"), Err(ParseError::InvalidDigit { index:0, offset:0 }));
    assert_eq!(parse_version("4.5.6\n"), Err(ParseError::InvalidDigit { index:2, offset:1 }));
}
#[test]
fn hidden_core_no_signs() {
    for s in ["+4.5.6","-4.5.6","v4.5.6"] { assert_eq!(parse_version(s), Err(ParseError::InvalidDigit { index:0, offset:0 })); }
}
#[test]
fn hidden_core_unicode_offsets() {
    assert_eq!(parse_version("8.12é.3"), Err(ParseError::InvalidDigit { index:1, offset:2 }));
    assert_eq!(parse_version("８.2.3"), Err(ParseError::InvalidDigit { index:0, offset:0 }));
}
#[test]
fn hidden_core_suffix_rejected() {
    assert_eq!(parse_version("4.5.67-rc"), Err(ParseError::InvalidDigit { index:2, offset:2 }));
    assert_eq!(parse_version("4.5.67+b"), Err(ParseError::InvalidDigit { index:2, offset:2 }));
}
#[test]
fn hidden_core_error_priority() {
    assert_eq!(parse_version("00x..7"), Err(ParseError::InvalidDigit { index:0, offset:2 }));
    assert_eq!(parse_version("00.18446744073709551616.z"), Err(ParseError::LeadingZero { index:0 }));
    assert_eq!(parse_version("18446744073709551616..z"), Err(ParseError::Overflow { index:0 }));
}
#[test]
fn hidden_core_nuls_and_internal_space() {
    assert_eq!(parse_version("7.8\0.9"), Err(ParseError::InvalidDigit { index:1, offset:1 }));
    assert_eq!(parse_version("7.8.1 2"), Err(ParseError::InvalidDigit { index:2, offset:1 }));
}
#[test]
fn hidden_core_max_all() {
    let s="18446744073709551615.18446744073709551615.18446744073709551615";
    let v=parse_version(s).unwrap(); assert_eq!((v.major,v.minor,v.patch),(u64::MAX,u64::MAX,u64::MAX));
}
#[test]
fn hidden_core_roundtrips() {
    for i in 1..=250u64 {
        let v=parse_version(&format!("{}.{}.{}",i,i*17,i*65537)).unwrap();
        assert_eq!((v.major,v.minor,v.patch),(i,i*17,i*65537));
    }
}
#[test]
fn hidden_core_leading_zero_before_overflow() {
    assert_eq!(parse_version("2.3.018446744073709551616"), Err(ParseError::LeadingZero { index:2 }));
    assert_eq!(parse_version("2.3.018446744073709551616x"), Err(ParseError::InvalidDigit { index:2, offset:21 }));
}
