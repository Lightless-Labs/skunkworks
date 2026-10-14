fn p(name: &str, deps: &[&str]) -> Package {
    Package { name: name.into(), dependencies: deps.iter().map(|s| s.to_string()).collect() }
}
fn names(values: &[&str]) -> Vec<String> { values.iter().map(|s| s.to_string()).collect() }

#[test]
fn hidden_missing_pair_order() {
assert_eq!(install_order(&[p("z", &["a"]), p("b", &["y", "x"])]), Err(ResolveError::MissingDependency { package: "b".into(), dependency: "x".into() }));
}

#[test]
fn hidden_missing_before_cycle() {
assert_eq!(install_order(&[p("a", &["a"]), p("z", &["q"])]), Err(ResolveError::MissingDependency { package: "z".into(), dependency: "q".into() }));
}

#[test]
fn hidden_self_cycle() {
assert_eq!(install_order(&[p("q", &["q", "q"])]), Err(ResolveError::Cycle(names(&["q", "q"]))));
}

#[test]
fn hidden_cycle_rotation() {
assert_eq!(install_order(&[p("a", &["z"]), p("z", &["b"]), p("b", &["c"]), p("c", &["z"])]), Err(ResolveError::Cycle(names(&["b", "c", "z", "b"]))));
}

#[test]
fn hidden_direction_preserved() {
assert_eq!(install_order(&[p("a", &["c"]), p("b", &["a"]), p("c", &["b"])]), Err(ResolveError::Cycle(names(&["a", "c", "b", "a"]))));
}

#[test]
fn hidden_sorted_neighbor_first_cycle() {
assert_eq!(install_order(&[p("a", &["c", "b"]), p("b", &["a"]), p("c", &["a"])]), Err(ResolveError::Cycle(names(&["a", "b", "a"]))));
}

#[test]
fn hidden_sorted_start_first_cycle() {
assert_eq!(install_order(&[p("z", &["z"]), p("c", &["b"]), p("b", &["c"])]), Err(ResolveError::Cycle(names(&["b", "c", "b"]))));
}

#[test]
fn hidden_visited_is_not_gray() {
assert_eq!(install_order(&[p("a", &["b", "c"]), p("b", &["d"]), p("c", &["d"]), p("d", &[])]), Ok(names(&["d", "b", "c", "a"])));
}

#[test]
fn hidden_repeated_edges_valid() {
assert_eq!(install_order(&[p("k", &["z", "z"]), p("z", &[])]), Ok(names(&["z", "k"])));
}

#[test]
fn hidden_ready_update() {
assert_eq!(install_order(&[p("z", &[]), p("a", &["b"]), p("b", &[])]), Ok(names(&["b", "a", "z"])));
}

#[test]
fn hidden_case_sensitive_missing() {
assert_eq!(install_order(&[p("A", &[]), p("x", &["a"])]), Err(ResolveError::MissingDependency { package: "x".into(), dependency: "a".into() }));
}

#[test]
fn hidden_disconnected_cycle() {
assert_eq!(install_order(&[p("a", &[]), p("y", &["z"]), p("z", &["y"])]), Err(ResolveError::Cycle(names(&["y", "z", "y"]))));
}

#[test]
fn hidden_not_shortest_cycle() {
assert_eq!(install_order(&[p("a", &["b", "a"]), p("b", &["a"])]), Err(ResolveError::Cycle(names(&["a", "a"]))));
}

#[test]
fn hidden_tail_excluded() {
assert_eq!(install_order(&[p("a", &["b"]), p("b", &["c"]), p("c", &["d"]), p("d", &["c"])]), Err(ResolveError::Cycle(names(&["c", "d", "c"]))));
}

#[test]
fn hidden_large_valid() {
let ps: Vec<_> = (0..90).rev().map(|n| p(&format!("n{n:03}"), &[])).collect(); let expected: Vec<_> = (0..90).map(|n| format!("n{n:03}")).collect(); assert_eq!(install_order(&ps), Ok(expected));
}

#[test]
fn hidden_missing_duplicate_sorted() {
assert_eq!(install_order(&[p("root", &["two", "one", "one"])]), Err(ResolveError::MissingDependency { package: "root".into(), dependency: "one".into() }));
}
