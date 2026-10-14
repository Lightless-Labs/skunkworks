fn p(name: &str, deps: &[&str]) -> Package {
    Package { name: name.into(), dependencies: deps.iter().map(|s| s.to_string()).collect() }
}
fn names(values: &[&str]) -> Vec<String> { values.iter().map(|s| s.to_string()).collect() }

#[test]
fn hidden_new_ready_preempts() {
assert_eq!(install_order(&[p("z", &[]), p("b", &[]), p("a", &["b"])]), names(&["b", "a", "z"]));
}

#[test]
fn hidden_repeated_edge() {
assert_eq!(install_order(&[p("x", &["q", "q", "q"]), p("q", &[])]), names(&["q", "x"]));
}

#[test]
fn hidden_case_order() {
assert_eq!(install_order(&[p("a", &[]), p("Z", &[]), p("A", &[])]), names(&["A", "Z", "a"]));
}

#[test]
fn hidden_opaque_names() {
assert_eq!(install_order(&[p("tool-2", &[]), p("tool-10", &[]), p("tool!", &[])]), names(&["tool!", "tool-10", "tool-2"]));
}

#[test]
fn hidden_singleton() {
assert_eq!(install_order(&[p("solo", &[])]), names(&["solo"]));
}

#[test]
fn hidden_isolated_with_chain() {
assert_eq!(install_order(&[p("a", &["c"]), p("b", &[]), p("c", &[]), p("d", &[])]), names(&["b", "c", "a", "d"]));
}

#[test]
fn hidden_diamond() {
assert_eq!(install_order(&[p("a", &["c", "b"]), p("b", &["d"]), p("c", &["d"]), p("d", &[])]), names(&["d", "b", "c", "a"]));
}

#[test]
fn hidden_all_dependencies_needed() {
assert_eq!(install_order(&[p("a", &["b", "z"]), p("b", &[]), p("c", &[]), p("z", &[])]), names(&["b", "c", "z", "a"]));
}

#[test]
fn hidden_permutation_invariance() {
let a = vec![p("n", &["k"]), p("k", &[]), p("m", &[])]; let mut b = a.clone(); b.reverse(); assert_eq!(install_order(&a), names(&["k", "m", "n"])); assert_eq!(install_order(&a), install_order(&b));
}

#[test]
fn hidden_dependency_permutation() {
let a = vec![p("a", &["z", "c", "b"]), p("b", &[]), p("c", &[]), p("z", &[])]; assert_eq!(install_order(&a), names(&["b", "c", "z", "a"]));
}

#[test]
fn hidden_unicode() {
assert_eq!(install_order(&[p("é", &[]), p("Ω", &[]), p("e", &[])]), names(&["e", "é", "Ω"]));
}

#[test]
fn hidden_many_roots() {
let ps: Vec<_> = (0..80).rev().map(|n| p(&format!("p{n:03}"), &[])).collect(); let expected: Vec<_> = (0..80).map(|n| format!("p{n:03}")).collect(); assert_eq!(install_order(&ps), expected);
}

#[test]
fn hidden_long_chain() {
let ps: Vec<_> = (0..70).map(|n| if n == 69 { p(&format!("x{n:02}"), &[]) } else { p(&format!("x{n:02}"), &[&format!("x{:02}", n+1)]) }).collect(); let expected: Vec<_> = (0..70).rev().map(|n| format!("x{n:02}")).collect(); assert_eq!(install_order(&ps), expected);
}

#[test]
fn hidden_multiple_new_ready() {
assert_eq!(install_order(&[p("a", &["d"]), p("b", &["d"]), p("c", &[]), p("d", &[]), p("e", &[])]), names(&["c", "d", "a", "b", "e"]));
}

#[test]
fn hidden_duplicate_and_diamond() {
assert_eq!(install_order(&[p("r", &["t", "s", "s"]), p("s", &["u", "u"]), p("t", &["u"]), p("u", &[])]), names(&["u", "s", "t", "r"]));
}

#[test]
fn hidden_repeated_calls() {
let ps = vec![p("beta", &["omega"]), p("omega", &[]), p("alpha", &[])]; let copy = ps.clone(); assert_eq!(install_order(&ps), names(&["alpha", "omega", "beta"])); assert_eq!(install_order(&ps), install_order(&copy)); assert_eq!(ps, copy);
}
