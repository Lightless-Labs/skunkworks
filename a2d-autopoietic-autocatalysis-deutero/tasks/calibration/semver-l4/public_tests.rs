#[test]
fn public_caret_tilde_and_wildcard() {
    for (s,yes,no) in [("^1.2.3","1.9.0","2.0.0"),("~1.2.3","1.2.9","1.3.0"),("1.2.x","1.2.4","1.3.0")] {
        let r=parse_range(s).unwrap(); assert!(satisfies(&parse_version(yes).unwrap(),&r)); assert!(!satisfies(&parse_version(no).unwrap(),&r));
    }
}
#[test]
fn public_hyphen_partial_and_or() {
    let r=parse_range("1.2.3 - 2.0.0 || 4.2").unwrap();
    for s in ["1.2.3","2.0.0","4.2.7"] { assert!(satisfies(&parse_version(s).unwrap(),&r)); }
    assert!(!satisfies(&parse_version("4.3.0").unwrap(),&r));
}
#[test]
fn public_full_comparators_and_prerelease() {
    let r=parse_range(">=1.2.3-alpha <2.0.0").unwrap();
    assert!(satisfies(&parse_version("1.2.3-beta").unwrap(),&r));
    assert!(!satisfies(&parse_version("1.4.0-beta").unwrap(),&r));
    assert!(satisfies(&parse_version("1.5.0+ci").unwrap(),&r));
    assert!(precedence(&parse_version("1.2.3-alpha").unwrap(),&parse_version("1.2.3-beta").unwrap()).is_lt());
}
#[test]
fn public_maximum() {
    let versions=["1.2.4","3.0.0","1.8.0","1.1.0"].map(|s|parse_version(s).unwrap());
    let r=parse_range("^1.2.3").unwrap(); assert_eq!(max_satisfying(&versions,&r),Some(&versions[2]));
    assert!(max_satisfying(&versions,&parse_range("9.x").unwrap()).is_none());
}
