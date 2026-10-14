#[test]
fn public_comparator_intersection() {
    let r=parse_range(">=1.2.3 <2.0.0").unwrap();
    assert!(satisfies(&parse_version("1.5.0").unwrap(),&r));
    assert!(!satisfies(&parse_version("2.0.0").unwrap(),&r));
    assert!(precedence(&parse_version("1.5.0").unwrap(),&parse_version("2.0.0").unwrap()).is_lt());
}
#[test]
fn public_or_and_equality() {
    let r=parse_range("=1.2.3 || >3.0.0 <=4.0.0").unwrap();
    assert!(satisfies(&parse_version("1.2.3+ci").unwrap(),&r));
    assert!(satisfies(&parse_version("4.0.0").unwrap(),&r));
    assert!(!satisfies(&parse_version("3.0.0").unwrap(),&r));
}
#[test]
fn public_prerelease_opt_in() {
    let r=parse_range(">=1.2.3-alpha <2.0.0").unwrap();
    assert!(satisfies(&parse_version("1.2.3-beta").unwrap(),&r));
    assert!(!satisfies(&parse_version("1.3.0-beta").unwrap(),&r));
}
#[test]
fn public_bare_and_empty_ranges() {
    let r=parse_range("1.2.3").unwrap(); assert!(satisfies(&parse_version("1.2.3").unwrap(),&r));
    let any=parse_range("").unwrap(); assert!(satisfies(&parse_version("9.9.9").unwrap(),&any));
    assert!(!satisfies(&parse_version("9.9.9-rc").unwrap(),&any));
}
