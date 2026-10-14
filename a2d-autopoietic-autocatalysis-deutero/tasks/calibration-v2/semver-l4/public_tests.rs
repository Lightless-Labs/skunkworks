fn pv(s:&str)->Version {parse_version(s).unwrap()}
fn pr(s:&str)->Range {parse_range(s).unwrap()}
#[test]
fn public_versions_ranges_extrema() {
    let v=pv("2.1.3-rc.1+build.7");
    assert_eq!((v.major,v.minor,v.patch),(2,1,3));
    assert_eq!(v.pre,vec!["rc","1"]);assert_eq!(v.build,vec!["build","7"]);
    assert_eq!(compare_versions(&pv("2.1.3-alpha"),&pv("2.1.3-beta")),std::cmp::Ordering::Less);
    let r=pr(">=2.1.0 <3.0.0");assert!(matches(&pv("2.5.0"),&r));assert!(!matches(&pv("3.0.0"),&r));
    assert!(matches(&pv("2.1.3-rc"),&pr(">=2.1.3-beta <2.1.3")));
    for s in ["^2.1.0","~2.5.0","2.5.x","2.1 - 2.8"] {assert!(matches(&pv("2.5.1"),&pr(s)));}
    assert!(matches(&pv("4.0.0"),&pr("2.x || 4.0.0")));
    let cs=vec!["2.2.0".to_string(),"2.8.0".to_string(),"3.0.0".to_string()];
    let r=pr("2.x");assert_eq!(max_satisfying(&cs,&r),Ok(Some("2.8.0".into())));
    assert_eq!(min_satisfying(&cs,&r),Ok(Some("2.2.0".into())));
}
#[test]
fn public_algebra() {
    let r=pr(">=2.1.0 <2.8.0");assert!(intersects(&r,&pr("2.5.x"),false));
    assert_eq!(canonical(&r),"[>=2.1.0;<2.8.0;pre=-]");
    assert!(matches_with(&pv("2.5.0-rc"),&r,true));
}
#[test]
fn public_packages() {
    let ps=vec![Package{name:"demo".into(),candidates:vec!["2.1.0".into(),"2.7.0".into()],constraints:vec![Constraint{label:"range".into(),expr:SetExpr::Range("2.x".into())}]}];
    assert_eq!(select_packages(&ps,Preference::Highest,false),Ok(vec![Selection{package:"demo".into(),version:"2.7.0".into()}]));
    assert_eq!(select_packages(&ps,Preference::Lowest,false).unwrap()[0].version,"2.1.0");
    assert_eq!(select_packages(&ps,Preference::StableFirst,false).unwrap()[0].version,"2.7.0");
}
#[test]
fn public_sets_diagnosis() {
    let p=Package{name:"demo".into(),candidates:vec!["2.1.0".into()],constraints:vec![Constraint{label:"blocked".into(),expr:SetExpr::Intersection(vec![SetExpr::Union(vec![SetExpr::Range("3.x".into()),SetExpr::Range("4.x".into())])])}]};
    assert_eq!(select_packages(&[p],Preference::Highest,false),Err(SelectError::NoSolution{package:"demo".into(),labels:vec!["blocked".into()]}));
}
