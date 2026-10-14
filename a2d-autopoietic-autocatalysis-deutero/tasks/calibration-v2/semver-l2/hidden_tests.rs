// R1: this module is compiled with the single-file Rust 2024 implementation.
fn hv(s:&str)->Version {parse_version(s).unwrap()}
fn hr(s:&str)->Range {parse_range(s).unwrap()}
fn strings(xs:&[&str])->Vec<String> {xs.iter().map(|s|s.to_string()).collect()}
#[test]
fn hidden_empty_missing() {
    // R2–R4, R6
    assert_eq!(parse_version(""),Err(VersionError::Empty));
    for (s,at) in [("7",1),("7.4",3),("7..1",2),("7.4.",4),("7+abc",1),(".2.3",0),("-1.2.3",0),("7.4.-a",4)] {
        assert_eq!(parse_version(s),Err(VersionError::MissingCore{at}),"{s}");
    }
}

#[test]
fn hidden_bad_core_bytes() {
    // R3, R4, R6, R20
    for (s,at) in [("v7.4.2",0),("7a.4.2",1),("7.4.2 ",5),(" 7.4.2",0),("7.é.2",2),("7.4.2.3",5),("7.4.a",4)] {
        assert_eq!(parse_version(s),Err(VersionError::InvalidChar{at}),"{s}");
    }
}

#[test]
fn hidden_number_errors() {
    // R5, R9
    assert_eq!(parse_version("07.1.2"),Err(VersionError::LeadingZero{at:0}));
    assert_eq!(parse_version("3.00.1"),Err(VersionError::LeadingZero{at:2}));
    assert_eq!(parse_version("3.8.012"),Err(VersionError::LeadingZero{at:4}));
    assert_eq!(parse_version("3.18446744073709551616.a"),Err(VersionError::Overflow{at:2}));
    assert_eq!(parse_version("00000000000000000000000000q"),Err(VersionError::LeadingZero{at:0}));
    assert_eq!(parse_version("18446744073709551616q"),Err(VersionError::Overflow{at:0}));
    assert_eq!(hv("18446744073709551615.18446744073709551615.18446744073709551615").patch,u64::MAX);
}

#[test]
fn hidden_empty_ids() {
    // R7, R8, R19
    for (s,at) in [("7.4.2-",6),("7.4.2+",6),("7.4.2-a..b",8),("7.4.2-a.",8),("7.4.2-+b",6),("7.4.2+a..b",8),("7.4.2+a.",8)] {
        assert_eq!(parse_version(s),Err(VersionError::EmptyIdentifier{at}),"{s}");
    }
}

#[test]
fn hidden_identifier_precedence() {
    // R7–R9, R19
    assert_eq!(parse_version("7.4.2-01@+bad!"),Err(VersionError::InvalidChar{at:8}));
    assert_eq!(parse_version("7.4.2-01+bad!"),Err(VersionError::LeadingZero{at:6}));
    assert_eq!(parse_version("7.4.2-a+q+r"),Err(VersionError::InvalidChar{at:9}));
    assert_eq!(parse_version("7.4.2-a.β"),Err(VersionError::InvalidChar{at:8}));
    assert_eq!(parse_version("7.4.2-a_b"),Err(VersionError::InvalidChar{at:7}));
}

#[test]
fn hidden_retained_identifiers() {
    // R2, R7–R9, R20
    let v=hv("8.6.4-Z.-.123456789012345678901234567890+000.Q-1");
    assert_eq!(v.pre,strings(&["Z","-","123456789012345678901234567890"]));
    assert_eq!(v.build,strings(&["000","Q-1"]));
    assert!(hv("8.6.4").pre.is_empty());assert!(hv("8.6.4").build.is_empty());
    assert_ne!(hv("8.6.4+a"),hv("8.6.4+b"));
}

#[test]
fn hidden_core_release_build() {
    // R2, R10
    use std::cmp::Ordering::*;
    assert_eq!(compare_versions(&hv("8.0.0-0"),&hv("7.999.999")),Greater);
    assert_eq!(compare_versions(&hv("7.8.9-a"),&hv("7.8.9")),Less);
    assert_eq!(compare_versions(&hv("7.8.9+a"),&hv("7.8.9+z")),Equal);
    assert_eq!(compare_versions(&hv("7.8.9-a+x"),&hv("7.8.9-a+y")),Equal);
}

#[test]
fn hidden_numeric_pre_order() {
    // R9–R11
    use std::cmp::Ordering::*;
    for (a,b) in [("9","10"),("999999999999999999999999","1000000000000000000000000"),("23","-"),("0","A")] {
        assert_eq!(compare_versions(&hv(&format!("6.3.1-{a}")),&hv(&format!("6.3.1-{b}"))),Less);
    }
}

#[test]
fn hidden_ascii_and_prefix() {
    // R10, R11
    use std::cmp::Ordering::*;
    for (a,b) in [("A","a"),("alpha","beta"),("alpha","alpha.0"),("alpha.3","alpha.3.0"),("alpha.9","alpha.10")] {
        assert_eq!(compare_versions(&hv(&format!("6.3.1-{a}")),&hv(&format!("6.3.1-{b}"))),Less);
    }
}

#[test]
fn hidden_comparator_syntax() {
    // R12, R13, R15
    for (s,at) in [("",0),(" \t\n",0)] {assert_eq!(parse_range(s).unwrap_err(),RangeError::Empty{at});}
    for (s,at) in [("==6.3.1",0),("!=6.3.1",0),("> 6.3.1",0),("6.3.1 &",6),("=",0),("\t<=",1)] {
        assert_eq!(parse_range(s).unwrap_err(),RangeError::Syntax{at},"{s}");
    }
}

#[test]
fn hidden_wrapped_positions() {
    // R12, R15, R16
    assert_eq!(parse_range(" \t>=06.3.1 <9.0.0").unwrap_err(),RangeError::Version{at:4,error:VersionError::LeadingZero{at:0}});
    assert_eq!(parse_range("6.3.1 <=6.3").unwrap_err(),RangeError::Version{at:8,error:VersionError::MissingCore{at:3}});
    assert_eq!(parse_range("6.3.1\u{2003}").unwrap_err(),RangeError::Version{at:0,error:VersionError::InvalidChar{at:5}});
    assert_eq!(parse_range("6.3.1, <9.0.0").unwrap_err(),RangeError::Version{at:0,error:VersionError::InvalidChar{at:5}});
}

#[test]
fn hidden_inclusive_strict() {
    // R13, R14, R16
    let r=hr("\t>=6.3.1\r\n <=6.5.0  ");
    for s in ["6.3.1","6.4.99","6.5.0"] {assert!(matches(&hv(s),&r));}
    for s in ["6.3.0","6.5.1"] {assert!(!matches(&hv(s),&r));}
    let r=hr(">6.3.1 <6.5.0");
    assert!(!matches(&hv("6.3.1"),&r));assert!(!matches(&hv("6.5.0"),&r));assert!(matches(&hv("6.4.0"),&r));
}

#[test]
fn hidden_metadata_equal() {
    // R14, R18
    let r=hr("=6.3.1+linux");
    assert!(matches(&hv("6.3.1+other"),&r));assert!(matches(&hv("6.3.1"),&r));
    assert!(!matches(&hv("6.3.1-a+linux"),&r));
    assert!(matches(&hv("6.3.1-a+other"),&hr("6.3.1-a+linux")));
}

#[test]
fn hidden_contradiction_duplicates() {
    // R14, R16
    let r=hr(">=6.3.1 >=6.3.1 <6.3.1");
    for s in ["6.3.0","6.3.1","6.3.2"] {assert!(!matches(&hv(s),&r));}
    assert!(matches(&hv("6.3.1"),&hr("6.3.1 =6.3.1")));
}

#[test]
fn hidden_admission_scope() {
    // R17, R18
    let r=hr(">=6.3.1-alpha <6.5.0");
    assert!(matches(&hv("6.3.1-beta"),&r));assert!(matches(&hv("6.3.1"),&r));
    assert!(!matches(&hv("6.3.1-0"),&r));assert!(!matches(&hv("6.4.0-beta"),&r));
    assert!(!matches(&hv("6.3.1-beta"),&hr(">=6.3.0 <6.5.0")));
    assert!(!matches(&hv("6.3.1-beta"),&hr(">=6.3.0+beta <6.5.0")));
}

#[test]
fn hidden_or_errors_scope() {
    // R22, R23, R17
    assert_eq!(parse_range(" || 6.3.1").unwrap_err(),RangeError::Empty{at:1});
    assert_eq!(parse_range("6.3.1 || \t").unwrap_err(),RangeError::Empty{at:10});
    assert_eq!(parse_range("6.3.1 || || 8.0.0").unwrap_err(),RangeError::Empty{at:9});
    assert_eq!(parse_range("=bad ||").unwrap_err(),RangeError::Syntax{at:0});
    assert_eq!(parse_range("6.3.1||8.0.0").unwrap_err(),RangeError::Version{at:0,error:VersionError::InvalidChar{at:5}});
    let r=hr("6.3.1-alpha || >=6.3.1 <6.4.0");
    assert!(!matches(&hv("6.3.1-beta"),&r));assert!(matches(&hv("6.3.1-alpha"),&r));
}

#[test]
fn hidden_wildcard_semantics() {
    // R24, R26, R17
    for range in ["7","7.x","7.X.*"] {let r=hr(range);assert!(matches(&hv("7.9.4"),&r));assert!(!matches(&hv("8.0.0"),&r));assert!(!matches(&hv("7.9.4-a"),&r));}
    for range in ["7.4","7.4.x","7.4.*"] {let r=hr(range);assert!(matches(&hv("7.4.999"),&r));assert!(!matches(&hv("7.5.0"),&r));}
    for range in ["*","x","X.*.*"] {assert!(matches(&hv("99.0.0"),&hr(range)));assert!(!matches(&hv("0.0.0-a"),&hr(range)));}
}

#[test]
fn hidden_partial_error_modes() {
    // R25, R27, R33
    for s in ["7..x","07","7.04","x.4","7.*.1","7.x.x.x","18446744073709551616"] {assert_eq!(parse_range(s).unwrap_err(),RangeError::Syntax{at:0},"{s}");}
    assert_eq!(parse_range("7.04.1").unwrap_err(),RangeError::Version{at:0,error:VersionError::LeadingZero{at:2}});
    assert_eq!(parse_range("^7.4-a").unwrap_err(),RangeError::Version{at:1,error:VersionError::MissingCore{at:3}});
    assert_eq!(parse_range(">=7.4").unwrap_err(),RangeError::Version{at:2,error:VersionError::MissingCore{at:3}});
    assert_eq!(parse_range("<=7.*").unwrap_err(),RangeError::Version{at:2,error:VersionError::InvalidChar{at:2}});
    assert_eq!(parse_range("7.4 &").unwrap_err(),RangeError::Syntax{at:4});
}

#[test]
fn hidden_caret_zero_modes() {
    // R28, R17
    for (range,yes,no) in [("^0","0.9.9","1.0.0"),("^0.0","0.0.9","0.1.0"),("^0.0.0","0.0.0","0.0.1"),("^0.4.7","0.4.99","0.5.0"),("^7.4.2","7.99.0","8.0.0")] {
        let r=hr(range);assert!(matches(&hv(yes),&r),"{range}");assert!(!matches(&hv(no),&r),"{range}");
    }
    let r=hr("^0.0.4-beta");assert!(matches(&hv("0.0.4-rc"),&r));assert!(!matches(&hv("0.0.5-beta"),&r));
    assert_eq!(parse_range("^x").unwrap_err(),RangeError::Syntax{at:0});
}

#[test]
fn hidden_tilde_modes() {
    // R29, R33
    for (range,yes,no) in [("~8","8.99.99","9.0.0"),("~8.4","8.4.99","8.5.0"),("~8.4.7","8.4.8","8.5.0")] {let r=hr(range);assert!(matches(&hv(yes),&r));assert!(!matches(&hv(no),&r));}
    assert!(!matches(&hv("8.4.6"),&hr("~8.4.7")));
    assert_eq!(parse_range("~*").unwrap_err(),RangeError::Syntax{at:0});
    assert!(matches(&hv("8.4.8"),&hr("~8.4 >=8.4.7 <8.4.9")));
}

#[test]
fn hidden_hyphen_semantics_errors() {
    // R30, R31
    let r=hr("5.2 - 7.4");assert!(matches(&hv("5.2.0"),&r));assert!(matches(&hv("7.4.999"),&r));assert!(!matches(&hv("7.5.0"),&r));
    let r=hr("5 - 7.4.2");assert!(matches(&hv("7.4.2+z"),&r));assert!(!matches(&hv("7.4.3"),&r));
    assert!(matches(&hv("7.4.2-beta"),&hr("7.4.2-alpha - 7.4.2-rc")));
    assert_eq!(parse_range("bad - 7.4 >=5.0.0").unwrap_err(),RangeError::Syntax{at:4});
    assert_eq!(parse_range("* - 7.4").unwrap_err(),RangeError::Syntax{at:0});
    assert_eq!(parse_range("* - bad").unwrap_err(),RangeError::Syntax{at:4});
}

#[test]
fn hidden_expansion_overflows() {
    // R25, R28–R32
    for (s,at) in [("18446744073709551615",0),("~18446744073709551615",1),("^18446744073709551615.0.0",1),("~2.18446744073709551615.0",1),("^0.0.18446744073709551615",1),("1 - 18446744073709551615",4)] {assert_eq!(parse_range(s).unwrap_err(),RangeError::Syntax{at},"{s}");}
    assert!(parse_range("18446744073709551615.0.0").is_ok());
}

#[test]
fn hidden_extrema_validation() {
    // R21, R34
    let r=hr("7.x");let cs=strings(&["8.0.0","bad","7.0.0"]);
    assert_eq!(max_satisfying(&cs,&r),Err(CandidateError{index:1,error:VersionError::InvalidChar{at:0}}));
    assert_eq!(min_satisfying(&strings(&["7.0.0","7.00.0"]),&r),Err(CandidateError{index:1,error:VersionError::LeadingZero{at:2}}));
    assert_eq!(max_satisfying(&[],&r),Ok(None));assert_eq!(min_satisfying(&strings(&["8.0.0"]),&r),Ok(None));
}

#[test]
fn hidden_extrema_ties_admission() {
    // R35, R17
    let cs=strings(&["7.4.0+first","7.2.0+early","7.4.0+second","7.2.0+late","7.9.0-a"]);
    assert_eq!(max_satisfying(&cs,&hr("7")),Ok(Some("7.4.0+first".into())));
    assert_eq!(min_satisfying(&cs,&hr("7")),Ok(Some("7.2.0+early".into())));
    assert_eq!(max_satisfying(&strings(&["7.4.0-alpha","7.4.0-beta"]),&hr(">=7.4.0-alpha <7.4.0")),Ok(Some("7.4.0-beta".into())));
}
