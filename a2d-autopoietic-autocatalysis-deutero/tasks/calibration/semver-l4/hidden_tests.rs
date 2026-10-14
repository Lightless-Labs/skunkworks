#[test]
fn hidden_caret_zero_rules() {
    for (r,yes,no) in [("^0","0.9.9","1.0.0"),("^0.0","0.0.99","0.1.0"),("^0.2","0.2.99","0.3.0"),("^0.0.3","0.0.3","0.0.4"),("^0.0.0","0.0.0","0.0.1"),("^0.2.3","0.2.9","0.3.0")] {
        let range=parse_range(r).unwrap(); assert!(satisfies(&parse_version(yes).unwrap(),&range),"{r} {yes}"); assert!(!satisfies(&parse_version(no).unwrap(),&range),"{r} {no}");
    }
    for (r,s) in [("^0.0.3","0.0.2"),("^0.2.3","0.2.2"),("^7.4.2","7.4.1")] {
        assert!(!satisfies(&parse_version(s).unwrap(),&parse_range(r).unwrap()));
    }
}
#[test]
fn hidden_tilde_partial_rules() {
    for (r,yes,no) in [("~7","7.99.99","8.0.0"),("~7.4","7.4.99","7.5.0"),("~0.0.8","0.0.9","0.1.0")] {
        let range=parse_range(r).unwrap(); assert!(satisfies(&parse_version(yes).unwrap(),&range)); assert!(!satisfies(&parse_version(no).unwrap(),&range));
    }
    assert!(!satisfies(&parse_version("7.3.99").unwrap(),&parse_range("~7.4").unwrap()));
    assert!(!satisfies(&parse_version("0.0.7").unwrap(),&parse_range("~0.0.8").unwrap()));
}
#[test]
fn hidden_wildcards_and_bare_partials() {
    for r in ["7","7.x","7.X","7.*"] {
        let range=parse_range(r).unwrap(); assert!(satisfies(&parse_version("7.99.2").unwrap(),&range)); assert!(!satisfies(&parse_version("8.0.0").unwrap(),&range));
    }
    for r in ["7.4","7.4.x","7.4.X","7.4.*"] {
        let range=parse_range(r).unwrap(); assert!(satisfies(&parse_version("7.4.99").unwrap(),&range)); assert!(!satisfies(&parse_version("7.5.0").unwrap(),&range));
        assert!(!satisfies(&parse_version("7.3.99").unwrap(),&range));
    }
    for r in ["*","x","X"," \t\n"] {
        let range=parse_range(r).unwrap(); assert!(satisfies(&parse_version("0.0.0").unwrap(),&range)); assert!(!satisfies(&parse_version("0.0.0-0").unwrap(),&range));
    }
}
#[test]
fn hidden_hyphen_partial_upper_bounds() {
    for (r,yes,no) in [("3 - 5","5.99.99","6.0.0"),("3.4 - 5.6","5.6.99","5.7.0"),("3.4.5 - 5.6.7","5.6.7","5.6.8")] {
        let range=parse_range(r).unwrap(); assert!(satisfies(&parse_version(yes).unwrap(),&range)); assert!(!satisfies(&parse_version(no).unwrap(),&range)); assert!(!satisfies(&parse_version("2.99.99").unwrap(),&range));
    }
    let r=parse_range("8 - 7").unwrap(); assert!(!satisfies(&parse_version("8.0.0").unwrap(),&r));
}
#[test]
fn hidden_hyphen_prerelease_endpoints() {
    let r=parse_range("6.7.8-a+lo - 6.7.9-z+hi").unwrap();
    for s in ["6.7.8-b","6.7.8","6.7.9-z+other"] { assert!(satisfies(&parse_version(s).unwrap(),&r)); }
    for s in ["6.7.7-z","6.7.9-zz","6.7.9"] { assert!(!satisfies(&parse_version(s).unwrap(),&r)); }
    let r=parse_range("6.7.8-rc").unwrap(); assert!(satisfies(&parse_version("6.7.8-rc+build").unwrap(),&r));
    for text in ["6.7.8-alpha.x", "6.7.8+build.x", "6.7.8+build.X", "6.7.8-alpha.x+build.x"] {
        let r=parse_range(text).unwrap(); assert!(satisfies(&parse_version(text).unwrap(),&r));
    }
}
#[test]
fn hidden_shorthand_prerelease_anchors() {
    for text in ["^4.5.6-alpha+ci","~4.5.6-alpha+ci"] {
        let r=parse_range(text).unwrap(); assert!(satisfies(&parse_version("4.5.6-beta").unwrap(),&r));
        assert!(!satisfies(&parse_version("4.5.7-beta").unwrap(),&r));
    }
    let r=parse_range("4.5.* >=4.5.7-alpha").unwrap(); assert!(satisfies(&parse_version("4.5.7-beta").unwrap(),&r));
    assert!(!satisfies(&parse_version("4.5.8-beta").unwrap(),&r));
}
#[test]
fn hidden_synthetic_upper_zero_and_no_anchor() {
    for text in ["^4.5.6 >=5.0.0-alpha","~4.5.6 >=4.6.0-alpha","4.x >=5.0.0-alpha","3 - 4.5 || =9.0.0"] {
        let r=parse_range(text).unwrap(); let s=if text.starts_with("~")||text.starts_with("3") {"4.6.0-beta"} else {"5.0.0-beta"};
        assert!(!satisfies(&parse_version(s).unwrap(),&r),"{text}");
    }
    let r=parse_range("^4.5.6").unwrap(); let c=&r.alternatives[0][1];
    assert_eq!(c.op,Op::Lt); assert_eq!((c.version.major,c.version.minor,c.version.patch),(5,0,0));
    assert_eq!(c.version.prerelease,vec!["0"]); assert!(c.version.build.is_empty()); assert!(!c.allows_prerelease);
    assert!(!satisfies(&parse_version("5.0.0-0").unwrap(),&r));
}
#[test]
fn hidden_or_gate_and_intersections() {
    let r=parse_range(">6.7.8-z || 6.x").unwrap(); assert!(!satisfies(&parse_version("6.7.8-a").unwrap(),&r));
    let r=parse_range("^6.0.0 ~6.7 >=6.7.4 <6.7.9 || =9.0.0+left").unwrap();
    for s in ["6.7.4","6.7.8","9.0.0+right"] { assert!(satisfies(&parse_version(s).unwrap(),&r)); }
    for s in ["6.7.3","6.7.9","6.8.0"] { assert!(!satisfies(&parse_version(s).unwrap(),&r)); }
    assert!(!satisfies(&parse_version("6.7.8").unwrap(),&parse_range("^6 ^7").unwrap()));
}
#[test]
fn hidden_bound_overflows_and_large_full_versions() {
    for (s,a,i) in [("18446744073709551615",0,0),("^18446744073709551615.0.0",0,0),("^0.0.18446744073709551615",0,0),("~2.18446744073709551615.0",0,0),("0 - 18446744073709551615",0,2),("1 - 2.18446744073709551615",0,2),("7.x || 2.18446744073709551615.*",1,0)] {
        assert_eq!(parse_range(s),Err(RangeError::BoundOverflow { alternative:a,index:i }));
    }
    let r=parse_range("=18446744073709551615.18446744073709551615.18446744073709551615").unwrap();
    assert!(satisfies(&parse_version("18446744073709551615.18446744073709551615.18446744073709551615").unwrap(),&r));
    let r=parse_range("0 - 18446744073709551615.0.0").unwrap(); assert!(satisfies(&parse_version("18446744073709551615.0.0").unwrap(),&r));
    let r=parse_range("^2.18446744073709551615.18446744073709551615").unwrap(); assert!(satisfies(&parse_version("2.18446744073709551615.18446744073709551615").unwrap(),&r));
}
#[test]
fn hidden_invalid_shorthand_and_hyphen_positions() {
    for s in ["x.1","*.x","1.x.x","1.x.2","1.2.3.x","1.2.x+ci","^*","~7.x","^7.2-a","7.2+ci","07","7.02","7..2","7.2.03","18446744073709551616","(7)","7,8","7.2.3_","^","~"] {
        assert_eq!(parse_range(s),Err(RangeError::InvalidToken { alternative:0,index:0 }),"{s}");
    }
    assert_eq!(parse_range("7.x || ~8 ^x"),Err(RangeError::InvalidToken { alternative:1,index:1 }));
    assert_eq!(parse_range("x - 9"),Err(RangeError::InvalidToken { alternative:0,index:0 }));
    assert_eq!(parse_range("7 - x"),Err(RangeError::InvalidToken { alternative:0,index:2 }));
    assert_eq!(parse_range("7 - 9 >=8.0.0"),Err(RangeError::InvalidToken { alternative:0,index:1 }));
}
#[test]
fn hidden_operator_errors_whitespace_and_empty_branches() {
    assert_eq!(parse_range(">=7.2"),Err(RangeError::InvalidVersion { alternative:0,index:0,error:ParseError::ComponentCount { found:2 } }));
    assert_eq!(parse_range("<*"),Err(RangeError::InvalidVersion { alternative:0,index:0,error:ParseError::ComponentCount { found:1 } }));
    assert_eq!(parse_range("7.x > 8.0.0 ||"),Err(RangeError::InvalidVersion { alternative:0,index:1,error:ParseError::EmptyInput }));
    assert_eq!(parse_range("7.x|| ||8"),Err(RangeError::EmptyAlternative { alternative:1 }));
    assert_eq!(parse_range("||7"),Err(RangeError::EmptyAlternative { alternative:0 }));
    assert_eq!(parse_range("7.0.0\u{a0}<8.0.0"),Err(RangeError::InvalidToken { alternative:0,index:0 }));
    let r=parse_range("\t~7.2\n>=7.2.4\x0c||\r9.X\x0b").unwrap(); assert!(satisfies(&parse_version("7.2.5").unwrap(),&r)); assert!(satisfies(&parse_version("9.4.5").unwrap(),&r));
}
#[test]
fn hidden_maximum_ties_preserve_first() {
    let versions=["7.2.3+first","7.2.3+second","7.1.9"].map(|s|parse_version(s).unwrap());
    let got=max_satisfying(&versions,&parse_range("7.x").unwrap()).unwrap();
    assert!(std::ptr::eq(got,&versions[0])); assert_eq!(got.build,vec!["first"]);
    let versions=["7.2.3-a+first","7.2.3-a+second"].map(|s|parse_version(s).unwrap());
    assert!(std::ptr::eq(max_satisfying(&versions,&parse_range("=7.2.3-a").unwrap()).unwrap(),&versions[0]));
}
#[test]
fn hidden_maximum_empty_and_nonmatching() {
    let r=parse_range("*").unwrap(); assert!(max_satisfying(&[],&r).is_none());
    let versions=["0.0.0-a","7.8.9-z"].map(|s|parse_version(s).unwrap()); assert!(max_satisfying(&versions,&r).is_none());
    assert!(max_satisfying(&versions,&parse_range("^8").unwrap()).is_none());
}
#[test]
fn hidden_maximum_numeric_and_release_precedence() {
    let versions=["5.6.7-9","5.6.7-100","5.6.7-10","5.6.7-z","5.6.7+winner","6.0.0-alpha"].map(|s|parse_version(s).unwrap());
    assert!(std::ptr::eq(max_satisfying(&versions,&parse_range("^5.6.7-0").unwrap()).unwrap(),&versions[4]));
    assert!(std::ptr::eq(max_satisfying(&versions[..3],&parse_range(">=5.6.7-0 <5.6.7").unwrap()).unwrap(),&versions[1]));
}
#[test]
fn hidden_l4_version_validation_and_ordering() {
    assert_eq!(parse_version(""),Err(ParseError::EmptyInput));
    assert_eq!(parse_version("8.9-a"),Err(ParseError::ComponentCount { found:2 }));
    assert_eq!(parse_version("8..10-a"),Err(ParseError::EmptyComponent { index:1 }));
    assert_eq!(parse_version("8.9x.10"),Err(ParseError::InvalidDigit { index:1,offset:1 }));
    assert_eq!(parse_version("8.18446744073709551616.10"),Err(ParseError::Overflow { index:1 }));
    assert_eq!(parse_version("08.9.10-a"),Err(ParseError::LeadingZero { index:0 }));
    assert_eq!(parse_version("8.9.10-a.01+"),Err(ParseError::NumericLeadingZero { index:1 }));
    assert_eq!(parse_version("8.9.10-"),Err(ParseError::EmptyIdentifier { section:Section::Prerelease,index:0 }));
    assert_eq!(parse_version("8.9.10+"),Err(ParseError::EmptyIdentifier { section:Section::Build,index:0 }));
    assert_eq!(parse_version("8.9.10+ab_c"),Err(ParseError::InvalidIdentifier { section:Section::Build,index:0,offset:2 }));
    let a=parse_version(&format!("8.9.10-{}+00","9".repeat(90))).unwrap(); let b=parse_version(&format!("8.9.10-1{}+01","0".repeat(90))).unwrap(); assert!(precedence(&a,&b).is_lt());
}
#[test]
fn hidden_l4_normalization_and_large_lists() {
    let r=parse_range("* ^0.0.7 ~6.2 6.2.9+ci || 8 - 9.4").unwrap();
    assert_eq!(r.alternatives[0].len(),5); assert_eq!(r.alternatives[1].len(),2);
    assert_eq!(r.alternatives[0][0].op,Op::Ge); assert_eq!(r.alternatives[0][1].version.patch,8);
    assert_eq!(r.alternatives[0][4].op,Op::Eq); assert_eq!(r.alternatives[0][4].version.build,vec!["ci"]);
    assert_eq!(r.alternatives[1][1].version.minor,5); assert_eq!(r.alternatives[1][1].version.prerelease,vec!["0"]);
    let mut versions=Vec::new(); for i in (0..400).rev() { versions.push(parse_version(&format!("6.2.{i}+v{i}")).unwrap()); }
    let r=parse_range("~6.2 <6.2.333").unwrap(); let got=max_satisfying(&versions,&r).unwrap(); assert_eq!(got.patch,332); assert_eq!(got.build,vec!["v332"]);
    let r=Range { alternatives:vec![vec![Comparator { op:Op::Ge,version:parse_version("6.2.0-a").unwrap(),allows_prerelease:false }]] };
    assert!(!satisfies(&parse_version("6.2.0-z").unwrap(),&r));
}
