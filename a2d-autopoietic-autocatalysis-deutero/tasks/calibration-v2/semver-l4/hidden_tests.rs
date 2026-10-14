// R1: this module is compiled with the single-file Rust 2024 implementation.
fn hv(s:&str)->Version {parse_version(s).unwrap()}
fn hr(s:&str)->Range {parse_range(s).unwrap()}
fn strings(xs:&[&str])->Vec<String> {xs.iter().map(|s|s.to_string()).collect()}
fn leaf(s:&str)->SetExpr {SetExpr::Range(s.into())}
fn constraint(label:&str,expr:SetExpr)->Constraint {Constraint{label:label.into(),expr}}
fn package(name:&str,cs:&[&str],constraints:Vec<Constraint>)->Package {Package{name:name.into(),candidates:strings(cs),constraints}}
fn selection(name:&str,v:&str)->Selection {Selection{package:name.into(),version:v.into()}}
#[test]
fn hidden_base_0() {
    // R2–R4, R6, R3, R4, R6, R20
    assert_eq!(parse_version(""),Err(VersionError::Empty));
    for (s,at) in [("7",1),("7.4",3),("7..1",2),("7.4.",4),("7+abc",1),(".2.3",0),("-1.2.3",0),("7.4.-a",4)] {
        assert_eq!(parse_version(s),Err(VersionError::MissingCore{at}),"{s}");
    }
    for (s,at) in [("v7.4.2",0),("7a.4.2",1),("7.4.2 ",5),(" 7.4.2",0),("7.é.2",2),("7.4.2.3",5),("7.4.a",4)] {
        assert_eq!(parse_version(s),Err(VersionError::InvalidChar{at}),"{s}");
    }
}

#[test]
fn hidden_base_1() {
    // R5, R9, R7, R8, R19
    assert_eq!(parse_version("07.1.2"),Err(VersionError::LeadingZero{at:0}));
    assert_eq!(parse_version("3.00.1"),Err(VersionError::LeadingZero{at:2}));
    assert_eq!(parse_version("3.8.012"),Err(VersionError::LeadingZero{at:4}));
    assert_eq!(parse_version("3.18446744073709551616.a"),Err(VersionError::Overflow{at:2}));
    assert_eq!(parse_version("00000000000000000000000000q"),Err(VersionError::LeadingZero{at:0}));
    assert_eq!(parse_version("18446744073709551616q"),Err(VersionError::Overflow{at:0}));
    assert_eq!(hv("18446744073709551615.18446744073709551615.18446744073709551615").patch,u64::MAX);
    for (s,at) in [("7.4.2-",6),("7.4.2+",6),("7.4.2-a..b",8),("7.4.2-a.",8),("7.4.2-+b",6),("7.4.2+a..b",8),("7.4.2+a.",8)] {
        assert_eq!(parse_version(s),Err(VersionError::EmptyIdentifier{at}),"{s}");
    }
}

#[test]
fn hidden_base_2() {
    // R7–R9, R19, R2, R7–R9, R20
    assert_eq!(parse_version("7.4.2-01@+bad!"),Err(VersionError::InvalidChar{at:8}));
    assert_eq!(parse_version("7.4.2-01+bad!"),Err(VersionError::LeadingZero{at:6}));
    assert_eq!(parse_version("7.4.2-a+q+r"),Err(VersionError::InvalidChar{at:9}));
    assert_eq!(parse_version("7.4.2-a.β"),Err(VersionError::InvalidChar{at:8}));
    assert_eq!(parse_version("7.4.2-a_b"),Err(VersionError::InvalidChar{at:7}));
    let v=hv("8.6.4-Z.-.123456789012345678901234567890+000.Q-1");
    assert_eq!(v.pre,strings(&["Z","-","123456789012345678901234567890"]));
    assert_eq!(v.build,strings(&["000","Q-1"]));
    assert!(hv("8.6.4").pre.is_empty());assert!(hv("8.6.4").build.is_empty());
    assert_ne!(hv("8.6.4+a"),hv("8.6.4+b"));
}

#[test]
fn hidden_base_3() {
    // R2, R10, R9–R11, R10, R11
    use std::cmp::Ordering::*;
    assert_eq!(compare_versions(&hv("8.0.0-0"),&hv("7.999.999")),Greater);
    assert_eq!(compare_versions(&hv("7.8.9-a"),&hv("7.8.9")),Less);
    assert_eq!(compare_versions(&hv("7.8.9+a"),&hv("7.8.9+z")),Equal);
    assert_eq!(compare_versions(&hv("7.8.9-a+x"),&hv("7.8.9-a+y")),Equal);
    use std::cmp::Ordering::*;
    for (a,b) in [("9","10"),("999999999999999999999999","1000000000000000000000000"),("23","-"),("0","A")] {
        assert_eq!(compare_versions(&hv(&format!("6.3.1-{a}")),&hv(&format!("6.3.1-{b}"))),Less);
    }
    use std::cmp::Ordering::*;
    for (a,b) in [("A","a"),("alpha","beta"),("alpha","alpha.0"),("alpha.3","alpha.3.0"),("alpha.9","alpha.10")] {
        assert_eq!(compare_versions(&hv(&format!("6.3.1-{a}")),&hv(&format!("6.3.1-{b}"))),Less);
    }
}

#[test]
fn hidden_base_4() {
    // R12, R13, R15, R12, R15, R16
    for (s,at) in [("",0),(" \t\n",0)] {assert_eq!(parse_range(s).unwrap_err(),RangeError::Empty{at});}
    for (s,at) in [("==6.3.1",0),("!=6.3.1",0),("> 6.3.1",0),("6.3.1 &",6),("=",0),("\t<=",1)] {
        assert_eq!(parse_range(s).unwrap_err(),RangeError::Syntax{at},"{s}");
    }
    assert_eq!(parse_range(" \t>=06.3.1 <9.0.0").unwrap_err(),RangeError::Version{at:4,error:VersionError::LeadingZero{at:0}});
    assert_eq!(parse_range("6.3.1 <=6.3").unwrap_err(),RangeError::Version{at:8,error:VersionError::MissingCore{at:3}});
    assert_eq!(parse_range("6.3.1\u{2003}").unwrap_err(),RangeError::Version{at:0,error:VersionError::InvalidChar{at:5}});
    assert_eq!(parse_range("6.3.1, <9.0.0").unwrap_err(),RangeError::Version{at:0,error:VersionError::InvalidChar{at:5}});
}

#[test]
fn hidden_base_5() {
    // R13, R14, R16, R14, R18, R14, R16
    let r=hr("\t>=6.3.1\r\n <=6.5.0  ");
    for s in ["6.3.1","6.4.99","6.5.0"] {assert!(matches(&hv(s),&r));}
    for s in ["6.3.0","6.5.1"] {assert!(!matches(&hv(s),&r));}
    let r=hr(">6.3.1 <6.5.0");
    assert!(!matches(&hv("6.3.1"),&r));assert!(!matches(&hv("6.5.0"),&r));assert!(matches(&hv("6.4.0"),&r));
    let r=hr("=6.3.1+linux");
    assert!(matches(&hv("6.3.1+other"),&r));assert!(matches(&hv("6.3.1"),&r));
    assert!(!matches(&hv("6.3.1-a+linux"),&r));
    assert!(matches(&hv("6.3.1-a+other"),&hr("6.3.1-a+linux")));
    let r=hr(">=6.3.1 >=6.3.1 <6.3.1");
    for s in ["6.3.0","6.3.1","6.3.2"] {assert!(!matches(&hv(s),&r));}
    assert!(matches(&hv("6.3.1"),&hr("6.3.1 =6.3.1")));
}

#[test]
fn hidden_base_6() {
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

#[test]
fn hidden_include_switch_bounds() {
    // R36, R37, R26, R35
    let r=hr("7.4");let v=hv("7.4.8-beta");assert!(!matches_with(&v,&r,false));assert!(matches_with(&v,&r,true));
    assert!(!matches_with(&hv("7.5.0-0"),&r,true));
    assert!(!matches_with(&hv("7.4.0-alpha"),&r,true));
    assert_eq!(max_satisfying(&strings(&["7.4.9-rc"]),&r),Ok(None));
    for s in ["7.4.0","7.4.8-beta","7.5.0"] {assert_eq!(matches_with(&hv(s),&r,false),matches(&hv(s),&r));}
}

#[test]
fn hidden_intersect_stable_and_domain() {
    // R38, R39, R40
    assert!(intersects(&hr("7.4"),&hr(">7.4.5 <7.4.7"),false));
    assert!(!intersects(&hr(">7.4.5 <7.4.6"),&hr("*"),false));
    assert!(intersects(&hr(">7.4.5 <7.4.6"),&hr("*"),true));
    assert!(!intersects(&hr("7"),&hr("8"),true));
    let last="18446744073709551615.18446744073709551615.18446744073709551615";
    assert!(!intersects(&hr(&format!(">{last}")),&hr("*"),true));
    assert!(intersects(&hr(&format!(">={last}")),&hr("*"),false));
    let r=hr(">0.0.18446744073709551615 <0.1.0");
    assert!(intersects(&r,&hr("*"),true));assert!(!intersects(&r,&hr("*"),false));
}

#[test]
fn hidden_intersection_common_anchor() {
    // R17, R22, R39
    let a=hr(">=7.4.0-alpha <7.4.0");let b=hr("<7.4.0");
    assert!(!intersects(&a,&b,false));assert!(intersects(&a,&b,true));
    assert!(intersects(&a,&hr(">7.4.0-beta <7.4.0-rc"),false));
    assert!(!intersects(&a,&hr("7.4.0-0 || <7.4.0"),false));
    assert!(intersects(&a,&hr("8.0.0 || 7.4.0-beta"),false));
}

#[test]
fn hidden_discrete_pre_gap() {
    // R40, R45, R11
    for id in ["0","alpha","alpha.99","9999999999999999999999999"] {
        let empty=hr(&format!(">7.4.0-{id} <7.4.0-{id}.0"));
        assert!(!intersects(&empty,&hr("*"),true),"{id}");assert_eq!(canonical(&empty),"EMPTY");
        let nonempty=hr(&format!(">7.4.0-{id} <=7.4.0-{id}.0"));
        assert!(intersects(&nonempty,&hr("*"),true),"{id}");
    }
    assert!(!intersects(&hr("<0.0.0-0"),&hr("*"),true));
}

#[test]
fn hidden_canonical_normalization() {
    // R41–R44
    let r=hr(">=7.4.0+z >=7.3.0 >7.4.0 <=7.6.0 <7.6.0+q");
    assert_eq!(canonical(&r),"[>7.4.0;<7.6.0;pre=-]");
    assert_eq!(canonical(&hr("7.4.0+meta")),"[>=7.4.0;<=7.4.0;pre=-]");
    assert_eq!(canonical(&hr("*")),"[*;*;pre=-]");
    assert_eq!(canonical(&hr("<0.0.0")),"EMPTY");
    assert_eq!(canonical(&hr("7.4.0 || 7.4.0+z || >=8.0.0 <8.0.0")),"[>=7.4.0;<=7.4.0;pre=-]");
}

#[test]
fn hidden_canonical_anchor_sort_union() {
    // R42–R44
    let r=hr(">=9.0.0-a >=10.0.0-b <12.0.0 || 7.4.0 || *");
    assert_eq!(canonical(&r),"[*;*;pre=-] || [>=10.0.0-b;<12.0.0;pre=9.0.0,10.0.0] || [>=7.4.0;<=7.4.0;pre=-]");
    assert_eq!(canonical(&hr("^7.4.0-beta")),"[>=7.4.0-beta;<8.0.0-0;pre=7.4.0]");
    assert_eq!(canonical(&hr("<=0.0.0-0")),"[*;<=0.0.0-0;pre=0.0.0]");
    assert_eq!(canonical(&hr(">7.4.0 <7.4.1")),"EMPTY");
}

#[test]
fn hidden_selection_policies() {
    // R46, R48, R53–R55
    let ps=vec![package("lib",&["9.0.0-alpha","8.0.0","8.1.0"],vec![])];
    assert_eq!(select_packages(&ps,Preference::Highest,false),Ok(vec![selection("lib","9.0.0-alpha")]));
    assert_eq!(select_packages(&ps,Preference::Lowest,false),Ok(vec![selection("lib","8.0.0")]));
    assert_eq!(select_packages(&ps,Preference::StableFirst,false),Ok(vec![selection("lib","8.1.0")]));
    let ps=vec![package("pre",&["9.0.0-alpha","9.0.0-beta"],vec![])];
    assert_eq!(select_packages(&ps,Preference::StableFirst,false),Ok(vec![selection("pre","9.0.0-beta")]));
}

#[test]
fn hidden_selection_ties_and_order() {
    // R48, R53–R55
    let ps=vec![package("z",&["7.4.0+zz","7.4.0+aa","7.4.0+zz"],vec![]),package("a",&["2.0.0-beta+b","2.0.0-beta+a"],vec![])];
    for pref in [Preference::Highest,Preference::Lowest,Preference::StableFirst] {
        assert_eq!(select_packages(&ps,pref,false),Ok(vec![selection("z","7.4.0+zz"),selection("a","2.0.0-beta+b")]));
    }
}

#[test]
fn hidden_nested_set_algebra() {
    // R47, R48, R60
    let e=SetExpr::Intersection(vec![SetExpr::Union(vec![leaf("7.4.0"),leaf("8.0.0")]),SetExpr::Union(vec![leaf("8.0.0"),leaf("9.0.0")])]);
    let ps=vec![package("p",&["7.4.0","9.0.0","8.0.0"],vec![constraint("nested",e)])];
    assert_eq!(select_packages(&ps,Preference::Highest,false),Ok(vec![selection("p","8.0.0")]));
    let ps=vec![package("p",&["7.4.0"],vec![constraint("whole",SetExpr::Intersection(vec![leaf("7.4.0"),leaf("8.0.0")]))])];
    assert_eq!(select_packages(&ps,Preference::Highest,false),Err(SelectError::NoSolution{package:"p".into(),labels:strings(&["whole"])}));
}

#[test]
fn hidden_empty_set_identities() {
    // R47, R48, R56, R58
    let p=package("p",&["7.4.0-a"],vec![constraint("true",SetExpr::Intersection(vec![]))]);
    assert_eq!(select_packages(&[p],Preference::Highest,false),Ok(vec![selection("p","7.4.0-a")]));
    let p=package("p",&["7.4.0"],vec![constraint("false",SetExpr::Union(vec![]))]);
    assert_eq!(select_packages(&[p],Preference::Highest,false),Err(SelectError::NoSolution{package:"p".into(),labels:strings(&["false"])}));
    let p=package("p",&[],vec![constraint("any",leaf("*"))]);
    assert_eq!(select_packages(&[p],Preference::Highest,false),Err(SelectError::NoSolution{package:"p".into(),labels:vec![]}));
}

#[test]
fn hidden_global_name_label_precedence() {
    // R49, R50
    let p=package("same",&["bad"],vec![constraint("x",leaf("bad")),constraint("x",leaf("*"))]);
    assert_eq!(select_packages(&[p.clone(),package("same",&[],vec![])],Preference::Highest,false),Err(SelectError::DuplicatePackage{name:"same".into()}));
    let p2=package("later",&[],vec![constraint("q",leaf("*")),constraint("q",leaf("*"))]);
    assert_eq!(select_packages(&[package("early",&["bad"],vec![]),p2],Preference::Highest,false),Err(SelectError::DuplicateLabel{package:"later".into(),label:"q".into()}));
    assert_eq!(select_packages(&[p],Preference::Highest,false),Err(SelectError::DuplicateLabel{package:"same".into(),label:"x".into()}));
}

#[test]
fn hidden_global_candidate_precedence() {
    // R49, R51, R52
    let ps=vec![package("early",&[],vec![constraint("broken",leaf("bad"))]),package("later",&["3.0.0","3.01.0","bad"],vec![])];
    assert_eq!(select_packages(&ps,Preference::Highest,false),Err(SelectError::InvalidCandidate{package:"later".into(),index:1,error:VersionError::LeadingZero{at:2}}));
    let ps=vec![package("early",&["bad"],vec![]),package("later",&["bad"],vec![])];
    assert_eq!(select_packages(&ps,Preference::Highest,false),Err(SelectError::InvalidCandidate{package:"early".into(),index:0,error:VersionError::InvalidChar{at:0}}));
}

#[test]
fn hidden_range_paths_and_validation() {
    // R49, R52
    let e=SetExpr::Union(vec![SetExpr::Intersection(vec![]),SetExpr::Intersection(vec![SetExpr::Union(vec![leaf("*"),leaf(">=01.0.0")])])]);
    let p=package("p",&["7.4.0"],vec![constraint("check",e)]);
    assert_eq!(select_packages(&[p],Preference::Highest,false),Err(SelectError::InvalidRange{package:"p".into(),label:"check".into(),path:vec![1,0,1],error:RangeError::Version{at:2,error:VersionError::LeadingZero{at:0}}}));
    let ps=vec![package("first",&[],vec![]),package("later",&[],vec![constraint("bad",leaf("||"))])];
    assert_eq!(select_packages(&ps,Preference::Highest,false),Err(SelectError::InvalidRange{package:"later".into(),label:"bad".into(),path:vec![],error:RangeError::Empty{at:0}}));
}

#[test]
fn hidden_singleton_and_lex_conflicts() {
    // R56, R57, R60
    let ps=vec![package("p",&["7.0.0","8.0.0"],vec![constraint("z",leaf(">=9.0.0")),constraint("a",leaf("<6.0.0"))])];
    assert_eq!(select_packages(&ps,Preference::Highest,false),Err(SelectError::NoSolution{package:"p".into(),labels:strings(&["a"])}));
    let ps=vec![package("p",&["7.0.0","8.0.0","9.0.0"],vec![constraint("z",leaf("7.0.0")),constraint("b",leaf("9.0.0")),constraint("a",leaf("8.0.0"))])];
    assert_eq!(select_packages(&ps,Preference::Highest,false),Err(SelectError::NoSolution{package:"p".into(),labels:strings(&["a","b"])}));
}

#[test]
fn hidden_minimum_not_inclusion_minimal() {
    // R56, R57
    let cs=vec![constraint("a",leaf("1.0.0 || 2.0.0")),constraint("b",leaf("2.0.0 || 3.0.0")),constraint("c",leaf("1.0.0 || 3.0.0")),constraint("d",leaf("1.0.0")),constraint("e",leaf("2.0.0"))];
    let p=package("p",&["1.0.0","2.0.0","3.0.0"],cs);
    assert_eq!(select_packages(&[p],Preference::Highest,false),Err(SelectError::NoSolution{package:"p".into(),labels:strings(&["b","d"])}));
}

#[test]
fn hidden_finite_candidate_conflict() {
    // R38, R56, R60
    let a=hr(">=7.0.0 <9.0.0");let b=hr(">7.4.0 <8.5.0");assert!(intersects(&a,&b,false));
    let p=package("p",&["7.2.0","8.7.0"],vec![constraint("a",leaf(">=7.0.0 <9.0.0")),constraint("b",leaf(">7.4.0 <8.5.0"))]);
    assert_eq!(select_packages(&[p],Preference::Highest,false),Err(SelectError::NoSolution{package:"p".into(),labels:strings(&["b"])}));
    let ps=vec![package("first",&["7.0.0"],vec![constraint("x",leaf("8.0.0"))]),package("second",&[],vec![])];
    assert_eq!(select_packages(&ps,Preference::Highest,false),Err(SelectError::NoSolution{package:"first".into(),labels:strings(&["x"])}));
}

#[test]
fn hidden_selection_include_reuse() {
    // R37, R47, R59
    let ps=vec![package("p",&["7.4.0-alpha","7.4.0-beta"],vec![constraint("gate",SetExpr::Intersection(vec![leaf(">=7.4.0-alpha <7.4.0"),leaf("<7.4.0")]))])];
    assert_eq!(select_packages(&ps,Preference::Highest,false),Err(SelectError::NoSolution{package:"p".into(),labels:strings(&["gate"])}));
    assert_eq!(select_packages(&ps,Preference::Highest,true),Ok(vec![selection("p","7.4.0-beta")]));
    assert_eq!(select_packages(&ps,Preference::Lowest,true),Ok(vec![selection("p","7.4.0-alpha")]));
    assert!(select_packages(&ps,Preference::Highest,false).is_err());
}

#[test]
fn hidden_empty_names_labels_packages() {
    // R50, R59
    for pref in [Preference::Highest,Preference::Lowest,Preference::StableFirst] {for inc in [false,true]{assert_eq!(select_packages(&[],pref,inc),Ok(vec![]));}}
    let ps=vec![package("",&["7.4.0"],vec![constraint("",leaf("*"))]),package("A",&["7.4.0"],vec![constraint("",leaf("*"))]),package("a",&["7.4.0"],vec![])];
    assert_eq!(select_packages(&ps,Preference::Highest,false).unwrap().len(),3);
    let ps=vec![package("",&[],vec![]),package("",&[],vec![])];
    assert_eq!(select_packages(&ps,Preference::Highest,false),Err(SelectError::DuplicatePackage{name:"".into()}));
}

#[test]
fn hidden_larger_nested_selection() {
    // R47, R48, R54, R56
    let mut p=package("many",&[],vec![constraint("union",SetExpr::Union(vec![leaf("6.x"),leaf("7.x")])),constraint("cap",leaf("<7.50.0"))]);
    p.candidates=(0..120).map(|i|format!("7.{i}.0")).collect();
    assert_eq!(select_packages(&[p.clone()],Preference::Highest,false),Ok(vec![selection("many","7.49.0")]));
    p.constraints.push(constraint("floor",leaf(">=7.50.0")));
    assert_eq!(select_packages(&[p],Preference::Highest,false),Err(SelectError::NoSolution{package:"many".into(),labels:strings(&["cap","floor"])}));
}
