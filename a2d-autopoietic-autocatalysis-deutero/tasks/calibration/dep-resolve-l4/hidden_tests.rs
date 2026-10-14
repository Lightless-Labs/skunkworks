fn v(major: u32, minor: u32, patch: u32) -> Version { Version { major, minor, patch } }
fn any() -> Constraint { Constraint::AtLeast(v(0,0,0)) }
fn d(package: &str, constraint: Constraint) -> Dependency { Dependency { package: package.into(), constraint } }
fn p(name: &str, versions: Vec<Version>, deps: Vec<Dependency>) -> Package {
    Package { name: name.into(), versions, dependencies: deps, optional_dependencies: vec![], features: vec![] }
}
fn f(name: &str, deps: &[&str]) -> Feature { Feature { name: name.into(), dependencies: deps.iter().map(|s| s.to_string()).collect() } }
fn r(name: &str, constraint: Constraint, features: &[&str]) -> Request {
    Request { package: name.into(), constraint, features: features.iter().map(|s| s.to_string()).collect() }
}
fn i(package: &str, version: Version) -> Installed { Installed { package: package.into(), version } }

#[test]
fn hidden_numeric_unsorted_duplicates() {
let ps = vec![p("lib",vec![v(1,9,9),v(1,12,0),v(1,2,0),v(1,12,0)],vec![])]; assert_eq!(resolve(&ps,&[r("lib",any(),&[])]),Ok(vec![i("lib",v(1,12,0))]));
}

#[test]
fn hidden_shared_constraints_arrive_late() {
let ps = vec![p("a",vec![v(1,0,0),v(2,0,0),v(3,0,0)],vec![]),p("m",vec![v(1,0,0)],vec![d("a",Constraint::AtLeast(v(2,0,0)))]),p("z",vec![v(1,0,0)],vec![d("a",Constraint::LessThan(v(3,0,0)))])]; assert_eq!(resolve(&ps,&[r("m",any(),&[]),r("z",any(),&[])]),Ok(vec![i("a",v(2,0,0)),i("m",v(1,0,0)),i("z",v(1,0,0))]));
}

#[test]
fn hidden_disjoint_dependents_conflict() {
let ps = vec![p("a",vec![v(1,0,0),v(2,0,0)],vec![]),p("x",vec![v(1,0,0)],vec![d("a",Constraint::Exact(v(1,0,0)))]),p("y",vec![v(1,0,0)],vec![d("a",Constraint::Exact(v(2,0,0)))])]; assert_eq!(resolve(&ps,&[r("x",any(),&[]),r("y",any(),&[])]),Err(ResolveError::Conflict("a".into())));
}

#[test]
fn hidden_repeated_root_constraints() {
let ps = vec![p("q",vec![v(1,0,0),v(2,0,0),v(3,0,0)],vec![])]; assert_eq!(resolve(&ps,&[r("q",Constraint::AtLeast(v(2,0,0)),&[]),r("q",Constraint::LessThan(v(3,0,0)),&[])]),Ok(vec![i("q",v(2,0,0))]));
}

#[test]
fn hidden_caret_zero_minor() {
let ps = vec![p("q",vec![v(0,4,6),v(0,4,9),v(0,5,0),v(1,0,0)],vec![])]; assert_eq!(resolve(&ps,&[r("q",Constraint::Caret(v(0,4,7)),&[])]),Ok(vec![i("q",v(0,4,9))]));
}

#[test]
fn hidden_caret_zero_patch() {
let ps = vec![p("q",vec![v(0,0,2),v(0,0,3),v(0,1,0)],vec![])]; assert_eq!(resolve(&ps,&[r("q",Constraint::Caret(v(0,0,2)),&[])]),Ok(vec![i("q",v(0,0,2))]));
}

#[test]
fn hidden_caret_max_components() {
let ps = vec![p("a",vec![v(u32::MAX,0,0),v(u32::MAX,u32::MAX,u32::MAX)],vec![]),p("b",vec![v(0,u32::MAX,1),v(0,u32::MAX,u32::MAX)],vec![]),p("c",vec![v(0,0,u32::MAX)],vec![])]; assert_eq!(resolve(&ps,&[r("a",Constraint::Caret(v(u32::MAX,0,0)),&[]),r("b",Constraint::Caret(v(0,u32::MAX,2)),&[]),r("c",Constraint::Caret(v(0,0,u32::MAX)),&[])]),Ok(vec![i("a",v(u32::MAX,u32::MAX,u32::MAX)),i("b",v(0,u32::MAX,u32::MAX)),i("c",v(0,0,u32::MAX))]));
}

#[test]
fn hidden_strict_less_than_boundary() {
let ps = vec![p("q",vec![v(4,2,0)],vec![])]; assert_eq!(resolve(&ps,&[r("q",Constraint::LessThan(v(4,2,0)),&[])]),Err(ResolveError::Conflict("q".into())));
}

#[test]
fn hidden_inclusive_and_caret_lower() {
let ps = vec![p("q",vec![v(2,3,4)],vec![])]; assert_eq!(resolve(&ps,&[r("q",Constraint::AtLeast(v(2,3,4)),&[]),r("q",Constraint::Caret(v(2,3,4)),&[])]),Ok(vec![i("q",v(2,3,4))])); assert_eq!(resolve(&ps,&[r("q",Constraint::Caret(v(2,3,5)),&[])]),Err(ResolveError::Conflict("q".into())));
}

#[test]
fn hidden_empty_and_unreachable() {
let ps = vec![p("broken",vec![],vec![d("lost",any())]),p("q",vec![v(1,0,0)],vec![])]; assert_eq!(resolve(&ps,&[]),Ok(vec![])); assert_eq!(resolve(&ps,&[r("q",any(),&[])]),Ok(vec![i("q",v(1,0,0))])); assert_eq!(resolve(&[p("q",vec![],vec![])],&[r("q",any(),&[])]),Err(ResolveError::Conflict("q".into())));
}

#[test]
fn hidden_optional_and_mandatory_intersection() {
let mut a = p("a",vec![v(1,0,0)],vec![d("b",Constraint::AtLeast(v(2,0,0)))]); a.optional_dependencies = vec![d("b",Constraint::LessThan(v(3,0,0))),d("b",Constraint::AtLeast(v(1,0,0)))]; a.features = vec![f("on", &["b","b"])]; let ps = vec![a,p("b",vec![v(1,0,0),v(2,0,0),v(3,0,0)],vec![])]; assert_eq!(resolve(&ps,&[r("a",any(),&["on"])]),Ok(vec![i("b",v(2,0,0)),i("a",v(1,0,0))])); assert_eq!(resolve(&ps,&[r("a",any(),&[])]),Ok(vec![i("b",v(3,0,0)),i("a",v(1,0,0))]));
}

#[test]
fn hidden_error_precedence() {
let ps = vec![p("a",vec![],vec![d("a",any()),d("z",any())])]; assert_eq!(resolve(&ps,&[r("a",any(),&["bad"])]),Err(ResolveError::MissingPackage("z".into()))); let ps = vec![p("a",vec![],vec![d("a",any())])]; assert_eq!(resolve(&ps,&[r("a",any(),&["bad"])]),Err(ResolveError::UnknownFeature{package:"a".into(),feature:"bad".into()})); assert_eq!(resolve(&ps,&[r("a",any(),&[])]),Err(ResolveError::Conflict("a".into())));
}

#[test]
fn hidden_multiple_conflicts_sort() {
let ps = vec![p("z",vec![],vec![]),p("a",vec![v(1,0,0)],vec![])]; assert_eq!(resolve(&ps,&[r("z",any(),&[]),r("a",Constraint::Exact(v(9,0,0)),&[])]),Err(ResolveError::Conflict("a".into())));
}

#[test]
fn hidden_feature_union_no_propagation() {
let mut a = p("a",vec![v(1,0,0)],vec![]); a.optional_dependencies = vec![d("b",any()),d("c",any())]; a.features=vec![f("first", &["b"]),f("second", &["c"])]; let mut b=p("b",vec![v(1,0,0)],vec![]); b.optional_dependencies=vec![d("lost",any())]; b.features=vec![f("first", &["lost"])]; let ps=vec![a,b,p("c",vec![v(1,0,0)],vec![])]; assert_eq!(resolve(&ps,&[r("a",any(),&["first"]),r("a",any(),&["second"])]),Ok(vec![i("b",v(1,0,0)),i("c",v(1,0,0)),i("a",v(1,0,0))]));
}

#[test]
fn hidden_canonical_cycle_and_ready_order() {
let ps=vec![p("a",vec![v(1,0,0)],vec![d("z",any())]),p("z",vec![v(1,0,0)],vec![d("b",any())]),p("b",vec![v(1,0,0)],vec![d("c",any())]),p("c",vec![v(1,0,0)],vec![d("z",any())])]; assert_eq!(resolve(&ps,&[r("a",any(),&[])]),Err(ResolveError::Cycle(vec!["b".into(),"c".into(),"z".into(),"b".into()]))); let ps=vec![p("a",vec![v(1,0,0)],vec![d("b",any())]),p("b",vec![v(1,0,0)],vec![]),p("z",vec![v(1,0,0)],vec![])]; assert_eq!(resolve(&ps,&[r("a",any(),&[]),r("z",any(),&[])]),Ok(vec![i("b",v(1,0,0)),i("a",v(1,0,0)),i("z",v(1,0,0))]));
}

#[test]
fn hidden_repeated_edges_and_large_closure() {
let mut ps: Vec<_> = (0..45).map(|n| p(&format!("p{n:02}"),vec![v(1,0,0),v(1,5,0)],if n==44 {vec![]} else {vec![d(&format!("p{:02}",n+1),any()),d(&format!("p{:02}",n+1),Constraint::LessThan(v(1,5,0)))]})).collect(); ps.push(p("unused",vec![],vec![])); let expected:Vec<_>=(0..45).rev().map(|n|i(&format!("p{n:02}"),if n==0 {v(1,5,0)} else {v(1,0,0)})).collect(); assert_eq!(resolve(&ps,&[r("p00",any(),&[])]),Ok(expected));
}
