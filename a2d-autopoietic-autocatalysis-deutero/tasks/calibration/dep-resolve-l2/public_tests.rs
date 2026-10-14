fn p(name: &str, deps: &[&str]) -> Package {
    Package { name: name.into(), dependencies: deps.iter().map(|s| s.to_string()).collect() }
}
fn names(values: &[&str]) -> Vec<String> { values.iter().map(|s| s.to_string()).collect() }

#[test]
fn public_valid_graph() {
assert_eq!(install_order(&[p("app", &["lib"]), p("lib", &[]), p("z", &[])]), Ok(names(&["lib", "app", "z"])));
}

#[test]
fn public_missing() {
assert_eq!(install_order(&[p("app", &["absent"])]), Err(ResolveError::MissingDependency { package: "app".into(), dependency: "absent".into() }));
}

#[test]
fn public_two_cycle() {
assert_eq!(install_order(&[p("b", &["a"]), p("a", &["b"])]), Err(ResolveError::Cycle(names(&["a", "b", "a"]))));
}

#[test]
fn public_empty() {
assert_eq!(install_order(&[]), Ok(Vec::<String>::new()));
}
