#[test]
fn hidden_version_missing_sections() {
    let v=parse_version("7.8.9").unwrap(); assert!(v.prerelease.is_empty()); assert!(v.build.is_empty());
}
#[test]
fn hidden_version_empty_and_core_priority() {
    assert_eq!(parse_version(""),Err(ParseError::EmptyInput));
    assert_eq!(parse_version("01.8.9-+"),Err(ParseError::LeadingZero { index:0 }));
    assert_eq!(parse_version("7.8-rc+"),Err(ParseError::ComponentCount { found:2 }));
    assert_eq!(parse_version("7..9-rc"),Err(ParseError::EmptyComponent { index:1 }));
}
#[test]
fn hidden_version_core_overflow_and_digits() {
    assert_eq!(parse_version("7.18446744073709551616.9-a"),Err(ParseError::Overflow { index:1 }));
    assert_eq!(parse_version("7.8.9 -a"),Err(ParseError::InvalidDigit { index:2, offset:1 }));
    assert_eq!(parse_version("+7.8.9"),Err(ParseError::ComponentCount { found:1 }));
}
#[test]
fn hidden_version_empty_prerelease() {
    for (s,i) in [("7.8.9-",0),("7.8.9-.a",0),("7.8.9-a..b",1),("7.8.9-a.",1)] {
        assert_eq!(parse_version(s),Err(ParseError::EmptyIdentifier { section:Section::Prerelease,index:i }));
    }
}
#[test]
fn hidden_version_empty_build() {
    for (s,i) in [("7.8.9+",0),("7.8.9+.a",0),("7.8.9+a..b",1),("7.8.9+a.",1)] {
        assert_eq!(parse_version(s),Err(ParseError::EmptyIdentifier { section:Section::Build,index:i }));
    }
}
#[test]
fn hidden_version_identifier_byte_errors() {
    assert_eq!(parse_version("7.8.9-ok.aé"),Err(ParseError::InvalidIdentifier { section:Section::Prerelease,index:1,offset:1 }));
    assert_eq!(parse_version("7.8.9+ab_c"),Err(ParseError::InvalidIdentifier { section:Section::Build,index:0,offset:2 }));
    assert_eq!(parse_version("7.8.9+a+b"),Err(ParseError::InvalidIdentifier { section:Section::Build,index:0,offset:1 }));
}
#[test]
fn hidden_version_numeric_zero_rules() {
    assert_eq!(parse_version("7.8.9-ok.00"),Err(ParseError::NumericLeadingZero { index:1 }));
    let v=parse_version("7.8.9-0.00a+00.001").unwrap();
    assert_eq!(v.prerelease,vec!["0","00a"]); assert_eq!(v.build,vec!["00","001"]);
}
#[test]
fn hidden_version_hyphen_and_split_order() {
    let v=parse_version("7.8.9--.a-b+build--.a-b").unwrap();
    assert_eq!(v.prerelease,vec!["-","a-b"]); assert_eq!(v.build,vec!["build--","a-b"]);
    let v=parse_version("7.8.9+meta-rc").unwrap(); assert!(v.prerelease.is_empty()); assert_eq!(v.build,vec!["meta-rc"]);
}
#[test]
fn hidden_version_error_ordering() {
    assert_eq!(parse_version("7.8.9-01+bad_"),Err(ParseError::NumericLeadingZero { index:0 }));
    assert_eq!(parse_version("7.8.9-00_"),Err(ParseError::InvalidIdentifier { section:Section::Prerelease,index:0,offset:2 }));
    assert_eq!(parse_version("7.8.9-a..01+"),Err(ParseError::EmptyIdentifier { section:Section::Prerelease,index:1 }));
}
#[test]
fn hidden_order_numeric_not_lexical() {
    assert!(precedence(&parse_version("5.6.7-9").unwrap(),&parse_version("5.6.7-10").unwrap()).is_lt());
    assert!(precedence(&parse_version("5.6.7-99").unwrap(),&parse_version("5.6.7-100").unwrap()).is_lt());
}
#[test]
fn hidden_order_unbounded_numeric() {
    let a=parse_version(&format!("5.6.7-{}", "9".repeat(100))).unwrap();
    let b=parse_version(&format!("5.6.7-1{}", "0".repeat(100))).unwrap();
    assert!(precedence(&a,&b).is_lt());
    assert!(precedence(&b,&parse_version("5.6.7-z").unwrap()).is_lt());
}
#[test]
fn hidden_order_canonical_chain() {
    let chain=["3.4.5-alpha","3.4.5-alpha.1","3.4.5-alpha.beta","3.4.5-beta","3.4.5-beta.2","3.4.5-beta.11","3.4.5-rc.1","3.4.5"];
    for pair in chain.windows(2) { let a=parse_version(pair[0]).unwrap(); let b=parse_version(pair[1]).unwrap(); assert!(precedence(&a,&b).is_lt()); assert!(precedence(&b,&a).is_gt()); }
}
#[test]
fn hidden_order_ascii_and_mixed() {
    let chain=["6.7.8-12","6.7.8--","6.7.8-A","6.7.8-Z","6.7.8-a","6.7.8-a0","6.7.8-z"];
    for pair in chain.windows(2) { assert!(precedence(&parse_version(pair[0]).unwrap(),&parse_version(pair[1]).unwrap()).is_lt()); }
}
#[test]
fn hidden_order_core_priority() {
    for (a,b) in [("9.0.0-z","10.0.0-a"),("10.9.100","10.10.0-a"),("10.10.9","10.10.10-a")] {
        assert!(precedence(&parse_version(a).unwrap(),&parse_version(b).unwrap()).is_lt());
    }
}
#[test]
fn hidden_order_prefix_and_build() {
    assert!(precedence(&parse_version("8.9.10-a.b").unwrap(),&parse_version("8.9.10-a.b.0").unwrap()).is_lt());
    let a=parse_version("8.9.10-a.b+1").unwrap(); let b=parse_version("8.9.10-a.b+0002").unwrap();
    assert!(precedence(&a,&b).is_eq()); assert_ne!(a,b);
}
#[test]
fn hidden_version_large_vectors_and_equal() {
    let ids=(0..150).map(|i|format!("r{i}")).collect::<Vec<_>>().join(".");
    let v=parse_version(&format!("9.8.7-{ids}+same.same")).unwrap();
    assert_eq!(v.prerelease.len(),150); assert_eq!(v.build,vec!["same","same"]); assert!(precedence(&v,&v).is_eq());
}
