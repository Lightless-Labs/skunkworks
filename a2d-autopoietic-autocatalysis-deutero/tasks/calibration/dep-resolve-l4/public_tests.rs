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
fn public_highest_with_dependency() {
let ps = vec![p("app", vec![v(1,0,0)], vec![d("lib", Constraint::AtLeast(v(1,0,0)))]), p("lib", vec![v(1,0,0),v(1,4,0)], vec![])]; assert_eq!(resolve(&ps, &[r("app", any(), &[])]), Ok(vec![i("lib",v(1,4,0)),i("app",v(1,0,0))]));
}

#[test]
fn public_exact_and_less_than() {
let ps = vec![p("a", vec![v(1,0,0),v(2,0,0)], vec![]), p("b", vec![v(1,0,0),v(2,0,0)], vec![])]; assert_eq!(resolve(&ps, &[r("a",Constraint::Exact(v(1,0,0)),&[]),r("b",Constraint::LessThan(v(2,0,0)),&[])]), Ok(vec![i("a",v(1,0,0)),i("b",v(1,0,0))]));
}

#[test]
fn public_caret_optional() {
let mut app = p("app",vec![v(1,0,0)],vec![]); app.optional_dependencies = vec![d("lib",Constraint::Caret(v(1,1,0)))]; app.features = vec![f("on", &["lib"])]; let ps = vec![app,p("lib",vec![v(1,1,0),v(1,8,0),v(2,0,0)],vec![])]; assert_eq!(resolve(&ps,&[r("app",any(),&["on"])]),Ok(vec![i("lib",v(1,8,0)),i("app",v(1,0,0))]));
}

#[test]
fn public_errors() {
assert_eq!(resolve(&[p("q",vec![v(1,0,0)],vec![])],&[r("q",Constraint::Exact(v(2,0,0)),&[])]),Err(ResolveError::Conflict("q".into()))); assert_eq!(resolve(&[],&[r("gone",any(),&[])]),Err(ResolveError::MissingPackage("gone".into()))); assert_eq!(resolve(&[p("a",vec![v(1,0,0)],vec![])],&[r("a",any(),&["bad"])]),Err(ResolveError::UnknownFeature{package:"a".into(),feature:"bad".into()})); assert_eq!(resolve(&[p("a",vec![v(1,0,0)],vec![d("a",any())])],&[r("a",any(),&[])]),Err(ResolveError::Cycle(vec!["a".into(),"a".into()])));
}
