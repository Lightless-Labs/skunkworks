fn p(name: &str, deps: &[&str]) -> Package {
    Package { name: name.into(), dependencies: names(deps), optional_dependencies: vec![], features: vec![] }
}
fn names(values: &[&str]) -> Vec<String> { values.iter().map(|s| s.to_string()).collect() }
fn f(name: &str, deps: &[&str]) -> Feature { Feature { name: name.into(), dependencies: names(deps) } }
fn r(name: &str, features: &[&str]) -> Request { Request { package: name.into(), features: names(features) } }
fn opt(name: &str, deps: &[&str], optional: &[&str], features: Vec<Feature>) -> Package {
    Package { name: name.into(), dependencies: names(deps), optional_dependencies: names(optional), features }
}

#[test]
fn public_root_closure() {
assert_eq!(resolve(&[p("app", &["core"]), p("core", &[]), p("unused", &[])], &[r("app", &[])]), Ok(names(&["core", "app"])));
}

#[test]
fn public_enable_optional() {
let ps = vec![opt("app", &[], &["extra"], vec![f("full", &["extra"])]), p("extra", &[])]; assert_eq!(resolve(&ps, &[r("app", &["full"])]), Ok(names(&["extra", "app"])));
}

#[test]
fn public_unknown_feature() {
assert_eq!(resolve(&[p("app", &[])], &[r("app", &["oops"])]), Err(ResolveError::UnknownFeature { package: "app".into(), feature: "oops".into() }));
}

#[test]
fn public_missing_and_cycle() {
assert_eq!(resolve(&[], &[r("absent", &[])]), Err(ResolveError::MissingPackage("absent".into()))); assert_eq!(resolve(&[p("a", &["b"]), p("b", &["a"])], &[r("a", &[])]), Err(ResolveError::Cycle(names(&["a", "b", "a"]))));
}
