fn pv(s:&str)->Version {parse_version(s).unwrap()}
fn pr(s:&str)->Range {parse_range(s).unwrap()}
#[test]
fn public_version_comparators() {
    let v=pv("2.1.3-rc.1+build.7");
    assert_eq!((v.major,v.minor,v.patch),(2,1,3));
    assert_eq!(v.pre,vec!["rc","1"]);assert_eq!(v.build,vec!["build","7"]);
    assert_eq!(compare_versions(&pv("2.1.3-alpha"),&pv("2.1.3-beta")),std::cmp::Ordering::Less);
    let r=pr(">=2.1.0 <3.0.0");assert!(matches(&pv("2.5.0"),&r));assert!(!matches(&pv("3.0.0"),&r));
    assert!(matches(&pv("2.1.3-rc"),&pr(">=2.1.3-beta <2.1.3")));
}
#[test]
fn public_expanded_extrema() {
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
fn public_canonical_empty() {
    assert_eq!(canonical(&pr(">=2.0.0 <2.0.0")),"EMPTY");assert!(!intersects(&pr("2.x"),&pr("3.x"),true));
}
