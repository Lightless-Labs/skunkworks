fn pv(s:&str)->Version {parse_version(s).unwrap()}
fn pr(s:&str)->Range {parse_range(s).unwrap()}
#[test]
fn public_version() {
    let v=pv("2.1.3-rc.1+build.7");
    assert_eq!((v.major,v.minor,v.patch),(2,1,3));
    assert_eq!(v.pre,vec!["rc","1"]);assert_eq!(v.build,vec!["build","7"]);
    assert_eq!(compare_versions(&pv("2.1.3-alpha"),&pv("2.1.3-beta")),std::cmp::Ordering::Less);
}
#[test]
fn public_comparators() {
    let r=pr(">=2.1.0 <3.0.0");assert!(matches(&pv("2.5.0"),&r));assert!(!matches(&pv("3.0.0"),&r));
    assert!(matches(&pv("2.1.3-rc"),&pr(">=2.1.3-beta <2.1.3")));
}
#[test]
fn public_expanded() {
    for s in ["^2.1.0","~2.5.0","2.5.x","2.1 - 2.8"] {assert!(matches(&pv("2.5.1"),&pr(s)));}
    assert!(matches(&pv("4.0.0"),&pr("2.x || 4.0.0")));
}
#[test]
fn public_extrema() {
    let cs=vec!["2.2.0".to_string(),"2.8.0".to_string(),"3.0.0".to_string()];
    let r=pr("2.x");assert_eq!(max_satisfying(&cs,&r),Ok(Some("2.8.0".into())));
    assert_eq!(min_satisfying(&cs,&r),Ok(Some("2.2.0".into())));
}
