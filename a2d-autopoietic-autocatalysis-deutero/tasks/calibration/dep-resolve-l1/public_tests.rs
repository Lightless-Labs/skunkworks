fn p(name: &str, deps: &[&str]) -> Package {
    Package { name: name.into(), dependencies: deps.iter().map(|s| s.to_string()).collect() }
}
fn names(values: &[&str]) -> Vec<String> { values.iter().map(|s| s.to_string()).collect() }

#[test]
fn public_chain() {
assert_eq!(install_order(&[p("app", &["lib"]), p("lib", &["base"]), p("base", &[])]), names(&["base", "lib", "app"]));
}

#[test]
fn public_ready_ties() {
assert_eq!(install_order(&[p("z", &[]), p("b", &[]), p("a", &[])]), names(&["a", "b", "z"]));
}

#[test]
fn public_shared_dependency() {
assert_eq!(install_order(&[p("worker", &["core"]), p("cli", &["core"]), p("core", &[])]), names(&["core", "cli", "worker"]));
}

#[test]
fn public_empty() {
assert_eq!(install_order(&[]), Vec::<String>::new());
}
