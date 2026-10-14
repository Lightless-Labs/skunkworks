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
fn public_precedence() {
    assert_eq!(compare_versions(&pv("2.1.3-rc"),&pv("2.1.3")),std::cmp::Ordering::Less);
}
#[test]
fn public_comparators() {
    let r=pr(">=2.1.0 <3.0.0");assert!(matches(&pv("2.5.0"),&r));assert!(!matches(&pv("3.0.0"),&r));
    assert!(matches(&pv("2.1.3-rc"),&pr(">=2.1.3-beta <2.1.3")));
}
#[test]
fn public_exact() {
    assert!(matches(&pv("2.1.3"),&pr("=2.1.3")));assert!(!matches(&pv("2.1.4"),&pr("=2.1.3")));
}
