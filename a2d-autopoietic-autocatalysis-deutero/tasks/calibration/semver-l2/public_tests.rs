#[test]
fn public_extended_parse() {
    let v=parse_version("1.2.3-alpha.4+linux.sha").unwrap();
    assert_eq!((v.major,v.minor,v.patch),(1,2,3));
    assert_eq!(v.prerelease,vec!["alpha","4"]); assert_eq!(v.build,vec!["linux","sha"]);
}
#[test]
fn public_release_order() {
    use std::cmp::Ordering::Less;
    assert_eq!(precedence(&parse_version("1.2.3-rc").unwrap(),&parse_version("1.2.3").unwrap()),Less);
    assert_eq!(precedence(&parse_version("1.2.3").unwrap(),&parse_version("2.0.0-alpha").unwrap()),Less);
}
#[test]
fn public_identifier_order() {
    assert!(precedence(&parse_version("1.2.3-4").unwrap(),&parse_version("1.2.3-beta").unwrap()).is_lt());
    assert!(precedence(&parse_version("1.2.3-alpha").unwrap(),&parse_version("1.2.3-alpha.1").unwrap()).is_lt());
}
#[test]
fn public_build_ignored() {
    let a=parse_version("1.2.3+aaa").unwrap(); let b=parse_version("1.2.3+zzz").unwrap();
    assert!(precedence(&a,&b).is_eq()); assert_ne!(a,b);
}
