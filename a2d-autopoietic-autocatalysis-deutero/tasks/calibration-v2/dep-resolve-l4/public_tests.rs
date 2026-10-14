
fn v(major:u32,minor:u32,patch:u32)->Version {Version{major,minor,patch}}
fn x(n:u32)->Version {v(n,0,0)}
fn e(n:u32)->Constraint {Constraint::Exact(x(n))}
fn ge(n:u32)->Constraint {Constraint::AtLeast(x(n))}
fn q(p:&str,c:Constraint)->Requirement {Requirement{package:p.into(),constraint:c,features:vec![]}}
fn d(p:&str,c:Constraint)->Dependency {Dependency{package:p.into(),constraint:c,features:vec![],gate:None,platform:Platform::Any}}
fn r(n:u32,deps:Vec<Dependency>)->Release {Release{version:x(n),deps,features:vec![]}}
fn feat(name:&str,enables:&[&str])->Feature {Feature{name:name.into(),enables:enables.iter().map(|s|s.to_string()).collect()}}
fn ask(p:&str,c:Constraint,fs:&[&str])->Requirement {let mut t=q(p,c);t.features=fs.iter().map(|s|s.to_string()).collect();t}
fn p(name:&str,releases:Vec<Release>)->Package {Package{name:name.into(),releases}}
fn sel(p:&str,n:u32)->Selection {Selection{package:p.into(),version:x(n)}}
fn lk(p:&str,n:u32)->Lock {Lock{package:p.into(),version:x(n)}}
fn run(c:&[Package],roots:&[Requirement])->Result<Vec<Selection>,ResolveError> {
    resolve(c,roots,&[],Target::Linux,100000).map(|r|r.selected)
}
fn locked(c:&[Package],roots:&[Requirement],locks:&[Lock],budget:usize)->Result<Resolution,ResolveError> {
    resolve(c,roots,locks,Target::Linux,budget)
}
fn change(p:&str,b:Option<u32>,a:Option<u32>,reason:ChangeReason)->Change {Change{package:p.into(),before:b.map(x),after:a.map(x),reason}}
// Clauses: R4, R6, R15, R16.
#[test]
fn public_transitive_selection() {
let c=vec![p("engine",vec![r(1,vec![d("util",ge(1))])]),p("util",vec![r(1,vec![])])];
assert_eq!(run(&c,&[q("engine",e(1))]).unwrap(),vec![sel("engine",1),sel("util",1)]);
}

// Clauses: R4, R5, R15.
#[test]
fn public_ranges() {
let c=vec![p("codec",vec![r(3,vec![]),r(1,vec![]),r(2,vec![])])];
assert_eq!(run(&c,&[q("codec",ge(1)),q("codec",Constraint::Before(x(3)))]).unwrap(),vec![sel("codec",2)]);
}

// Clauses: R18, R19.
#[test]
fn public_error_format() {
let error=ResolveError::InvalidName{site:"root[0]".into(),name:"Bad Name".into()};
assert_eq!(error_text(&error),"invalid-name|site=root%5B0%5D|name=Bad%20Name");
}

// Clauses: R39, R41, R42.
#[test]
fn public_direct_feature() {
let mut release=r(1,vec![]);release.features.push(feat("extras",&[]));
let mut dep=d("plugin",e(1));dep.gate=Some("extras".into());release.deps.push(dep);
let c=vec![p("app",vec![release]),p("plugin",vec![r(1,vec![])])];
let out=resolve(&c,&[ask("app",e(1),&["extras"])],&[],Target::Linux,10).unwrap();
assert_eq!(out.selected,vec![sel("app",1),sel("plugin",1)]);
assert_eq!(out.changes,vec![change("app",None,Some(1),ChangeReason::Added),change("plugin",None,Some(1),ChangeReason::Added)]);
}
