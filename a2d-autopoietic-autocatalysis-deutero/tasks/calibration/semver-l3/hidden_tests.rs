#[test]
fn hidden_range_inclusive_boundaries() {
    let r=parse_range(">=5.6.7 <=5.6.9").unwrap();
    for s in ["5.6.7","5.6.8","5.6.9"] { assert!(satisfies(&parse_version(s).unwrap(),&r)); }
    for s in ["5.6.6","5.6.10"] { assert!(!satisfies(&parse_version(s).unwrap(),&r)); }
}
#[test]
fn hidden_range_exclusive_boundaries() {
    let r=parse_range(">5.6.7 <5.6.9").unwrap();
    assert!(satisfies(&parse_version("5.6.8").unwrap(),&r));
    for s in ["5.6.7","5.6.9"] { assert!(!satisfies(&parse_version(s).unwrap(),&r)); }
}
#[test]
fn hidden_range_build_equality() {
    let r=parse_range("=6.7.8-rc+left").unwrap();
    assert!(satisfies(&parse_version("6.7.8-rc+right").unwrap(),&r));
    assert!(!satisfies(&parse_version("6.7.8").unwrap(),&r));
}
#[test]
fn hidden_range_numeric_precedence() {
    let r=parse_range(">=6.7.8-9 <6.7.8-100").unwrap();
    assert!(satisfies(&parse_version("6.7.8-10").unwrap(),&r));
    assert!(!satisfies(&parse_version("6.7.8-8").unwrap(),&r));
}
#[test]
fn hidden_range_contradiction() {
    let r=parse_range(">=7.0.0 <7.0.0").unwrap(); assert!(!satisfies(&parse_version("7.0.0").unwrap(),&r));
    let r=parse_range("=7.0.0 =8.0.0").unwrap(); assert!(!satisfies(&parse_version("7.0.0").unwrap(),&r));
}
#[test]
fn hidden_range_pre_excluded_without_anchor() {
    let r=parse_range(">4.0.0 <9.0.0").unwrap();
    assert!(!satisfies(&parse_version("5.0.0-alpha").unwrap(),&r));
    assert!(satisfies(&parse_version("5.0.0").unwrap(),&r));
}
#[test]
fn hidden_range_pre_anchor_tuple() {
    let r=parse_range(">=5.6.7-alpha <9.0.0").unwrap();
    assert!(satisfies(&parse_version("5.6.7-z").unwrap(),&r));
    for s in ["5.6.8-z","5.7.7-z","6.6.7-z"] { assert!(!satisfies(&parse_version(s).unwrap(),&r)); }
}
#[test]
fn hidden_range_anchor_not_restricted_to_lower() {
    let r=parse_range(">=5.6.7 <5.6.8-z").unwrap();
    assert!(satisfies(&parse_version("5.6.8-a").unwrap(),&r));
    assert!(!satisfies(&parse_version("5.6.8-z").unwrap(),&r));
}
#[test]
fn hidden_range_no_anchor_leak_across_or() {
    let r=parse_range(">5.6.7-z || >=4.0.0 <6.0.0").unwrap();
    assert!(!satisfies(&parse_version("5.6.7-a").unwrap(),&r));
    assert!(satisfies(&parse_version("5.6.7-zz").unwrap(),&r));
}
#[test]
fn hidden_range_whitespace_and_adjacent_or() {
    let r=parse_range("\t>=8.0.0\n<9.0.0||=10.0.0\r\x0b\x0c").unwrap();
    assert_eq!(r.alternatives.len(),2); assert_eq!(r.alternatives[0].len(),2);
    assert!(satisfies(&parse_version("8.5.0").unwrap(),&r)); assert!(satisfies(&parse_version("10.0.0").unwrap(),&r));
    let r=parse_range(" \t\n").unwrap(); assert_eq!(r.alternatives,vec![vec![]]);
    assert!(!satisfies(&parse_version("0.0.0-0").unwrap(),&r));
}
#[test]
fn hidden_range_empty_alternative_errors() {
    for (s,a) in [("||8.0.0",0),("8.0.0||",1),("8.0.0 || \t ||9.0.0",1)] {
        assert_eq!(parse_range(s),Err(RangeError::EmptyAlternative { alternative:a }));
    }
}
#[test]
fn hidden_range_nested_version_errors() {
    assert_eq!(parse_range(">=8.0.0 || <=9.0.0 >"),Err(RangeError::InvalidVersion { alternative:1,index:1,error:ParseError::EmptyInput }));
    assert_eq!(parse_range("8.0.0 >=09.1.0"),Err(RangeError::InvalidVersion { alternative:0,index:1,error:ParseError::LeadingZero { index:0 } }));
    assert_eq!(parse_range("<9.0.0-a.01"),Err(RangeError::InvalidVersion { alternative:0,index:0,error:ParseError::NumericLeadingZero { index:1 } }));
    assert_eq!(parse_range("=8.9.18446744073709551616"),Err(RangeError::InvalidVersion { alternative:0,index:0,error:ParseError::Overflow { index:2 } }));
    assert_eq!(parse_range("=8.9.10+a..b"),Err(RangeError::InvalidVersion { alternative:0,index:0,error:ParseError::EmptyIdentifier { section:Section::Build,index:1 } }));
    assert_eq!(parse_version("8.9.10-aé"),Err(ParseError::InvalidIdentifier { section:Section::Prerelease,index:0,offset:1 }));
}
#[test]
fn hidden_range_unsupported_tokens_and_error_order() {
    assert_eq!(parse_range("*"),Err(RangeError::InvalidVersion { alternative:0,index:0,error:ParseError::ComponentCount { found:1 } }));
    assert_eq!(parse_range("==8.0.0"),Err(RangeError::InvalidVersion { alternative:0,index:0,error:ParseError::InvalidDigit { index:0,offset:0 } }));
    assert_eq!(parse_range("> 8.0.0 ||"),Err(RangeError::InvalidVersion { alternative:0,index:0,error:ParseError::EmptyInput }));
    assert_eq!(parse_range("8.0.0\u{a0}<9.0.0"),Err(RangeError::InvalidVersion { alternative:0,index:0,error:ParseError::ComponentCount { found:5 } }));
    assert_eq!(parse_range("8.0.0 | 9.0.0"),Err(RangeError::InvalidVersion { alternative:0,index:1,error:ParseError::ComponentCount { found:1 } }));
}
#[test]
fn hidden_range_representation_and_manual_flags() {
    let r=parse_range(">=6.7.8-a+ci <=9.0.0 >=6.7.8-a+ci").unwrap();
    assert_eq!(r.alternatives[0].len(),3); assert_eq!(r.alternatives[0][0].op,Op::Ge);
    assert!(r.alternatives[0][0].allows_prerelease); assert!(!r.alternatives[0][1].allows_prerelease);
    let v=parse_version("6.7.8-b").unwrap();
    let mut manual=Range { alternatives:vec![vec![Comparator { op:Op::Ge,version:parse_version("6.7.8-a").unwrap(),allows_prerelease:false }]] };
    assert!(!satisfies(&v,&manual)); manual.alternatives[0][0].allows_prerelease=true; assert!(satisfies(&v,&manual));
    assert!(!satisfies(&v,&Range { alternatives:vec![] }));
}
#[test]
fn hidden_range_max_core_and_distant_or() {
    let r=parse_range("=0.0.0 || >=18446744073709551615.0.0").unwrap();
    assert!(satisfies(&parse_version("0.0.0").unwrap(),&r));
    assert!(satisfies(&parse_version("18446744073709551615.9.9").unwrap(),&r));
    assert!(!satisfies(&parse_version("18446744073709551614.9.9").unwrap(),&r));
}
#[test]
fn hidden_range_large_intersection_and_union() {
    let s=(0..80).map(|i|format!(">={i}.0.0")).collect::<Vec<_>>().join(" ")+" <81.0.0";
    let r=parse_range(&s).unwrap(); assert!(satisfies(&parse_version("80.0.0").unwrap(),&r)); assert!(!satisfies(&parse_version("78.0.0").unwrap(),&r));
    let s=(0..100).map(|i|format!("={i}.0.0")).collect::<Vec<_>>().join("||");
    let r=parse_range(&s).unwrap(); assert!(satisfies(&parse_version("99.0.0").unwrap(),&r)); assert!(!satisfies(&parse_version("100.0.0").unwrap(),&r));
}
