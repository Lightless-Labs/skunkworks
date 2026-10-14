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
fn hidden_empty_ignores_defects() {
let ps = vec![p("bad", &["missing"]), p("loop", &["loop"])]; assert_eq!(resolve(&ps, &[]), Ok(vec![]));
}

#[test]
fn hidden_unreachable_defects() {
assert_eq!(resolve(&[p("root", &[]), p("bad", &["missing"]), p("loop", &["loop"])], &[r("root", &[])]), Ok(names(&["root"])));
}

#[test]
fn hidden_dormant_missing_and_cycle() {
let ps = vec![opt("a", &[], &["gone", "a"], vec![f("extras", &["gone", "a"])])]; assert_eq!(resolve(&ps, &[r("a", &[])]), Ok(names(&["a"])));
}

#[test]
fn hidden_requests_union() {
let ps = vec![opt("app", &[], &["left", "right"], vec![f("l", &["left"]), f("r", &["right"])]), p("left", &[]), p("right", &[])]; assert_eq!(resolve(&ps, &[r("app", &["l"]), r("app", &["r", "r"])]), Ok(names(&["left", "right", "app"])));
}

#[test]
fn hidden_no_feature_propagation() {
let ps = vec![opt("a", &["b"], &[], vec![f("wide", &[])]), opt("b", &[], &["gone"], vec![f("wide", &["gone"])])]; assert_eq!(resolve(&ps, &[r("a", &["wide"])]), Ok(names(&["b", "a"])));
}

#[test]
fn hidden_dependency_requested_feature() {
let ps = vec![p("a", &["b"]), opt("b", &[], &["c"], vec![f("on", &["c"])]), p("c", &[])]; assert_eq!(resolve(&ps, &[r("a", &[]), r("b", &["on"])]), Ok(names(&["c", "b", "a"])));
}

#[test]
fn hidden_missing_priority_unknown() {
let ps = vec![p("a", &["gone"])]; assert_eq!(resolve(&ps, &[r("a", &["bad"])]), Err(ResolveError::MissingPackage("gone".into())));
}

#[test]
fn hidden_missing_globally_smallest() {
let ps = vec![p("a", &["z"]), p("b", &["c"]), p("c", &["m"])]; assert_eq!(resolve(&ps, &[r("a", &[]), r("b", &[])]), Err(ResolveError::MissingPackage("m".into())));
}

#[test]
fn hidden_unknown_pair_sort() {
assert_eq!(resolve(&[p("b", &[]), p("a", &[])], &[r("b", &["aa"]), r("a", &["zz", "bb"])]), Err(ResolveError::UnknownFeature { package: "a".into(), feature: "bb".into() }));
}

#[test]
fn hidden_unknown_precedes_cycle() {
assert_eq!(resolve(&[p("a", &["a"])], &[r("a", &["bad"])]), Err(ResolveError::UnknownFeature { package: "a".into(), feature: "bad".into() }));
}

#[test]
fn hidden_optional_cycle_rotation() {
let ps = vec![p("a", &["z"]), p("z", &["b"]), opt("b", &[], &["c"], vec![f("on", &["c"])]), p("c", &["z"])]; assert_eq!(resolve(&ps, &[r("a", &[]), r("b", &["on"])]), Err(ResolveError::Cycle(names(&["b", "c", "z", "b"]))));
}

#[test]
fn hidden_shared_and_repeated_targets() {
let ps = vec![opt("a", &["c", "c"], &["c", "b", "b"], vec![f("on", &["c", "b", "b"])]), p("b", &["c"]), p("c", &[])]; assert_eq!(resolve(&ps, &[r("a", &["on", "on"])]), Ok(names(&["c", "b", "a"])));
}

#[test]
fn hidden_dynamic_ready_roots() {
assert_eq!(resolve(&[p("a", &["b"]), p("b", &[]), p("z", &[])], &[r("z", &[]), r("a", &[])]), Ok(names(&["b", "a", "z"])));
}

#[test]
fn hidden_valid_features_despite_unknown() {
let ps = vec![opt("a", &[], &["lost"], vec![f("yes", &["lost"])])]; assert_eq!(resolve(&ps, &[r("a", &["bad", "yes"])]), Err(ResolveError::MissingPackage("lost".into())));
}

#[test]
fn hidden_missing_root_and_deep_target() {
let ps = vec![p("b", &["c"]), p("c", &["a"])]; assert_eq!(resolve(&ps, &[r("z", &[]), r("b", &[])]), Err(ResolveError::MissingPackage("a".into())));
}

#[test]
fn hidden_large_closure() {
let ps: Vec<_> = (0..65).map(|n| if n == 64 { p(&format!("p{n:02}"), &[]) } else { p(&format!("p{n:02}"), &[&format!("p{:02}", n+1)]) }).collect(); let expected: Vec<_> = (0..65).rev().map(|n| format!("p{n:02}")).collect(); assert_eq!(resolve(&ps, &[r("p00", &[])]), Ok(expected));
}
