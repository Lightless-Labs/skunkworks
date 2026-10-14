
fn v(major:u32,minor:u32,patch:u32)->Version {Version{major,minor,patch}}
fn x(n:u32)->Version {v(n,0,0)}
fn e(n:u32)->Constraint {Constraint::Exact(x(n))}
fn ge(n:u32)->Constraint {Constraint::AtLeast(x(n))}
fn q(p:&str,c:Constraint)->Requirement {Requirement{package:p.into(),constraint:c}}
fn r(n:u32,deps:Vec<Requirement>)->Release {Release{version:x(n),deps}}
fn p(name:&str,releases:Vec<Release>)->Package {Package{name:name.into(),releases}}
fn sel(p:&str,n:u32)->Selection {Selection{package:p.into(),version:x(n)}}
fn run(c:&[Package],roots:&[Requirement])->Result<Vec<Selection>,ResolveError> {resolve(c,roots,100000)}
// Clauses: R4, R6, R15, R16.
#[test]
fn public_transitive_selection() {
let c=vec![p("engine",vec![r(1,vec![q("util",ge(1))])]),p("util",vec![r(1,vec![])])];
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

// Clauses: R21, R26.
#[test]
fn public_backtracking() {
let c=vec![p("app",vec![r(2,vec![q("missing",e(1))]),r(1,vec![])])];
assert_eq!(run(&c,&[q("app",ge(1))]).unwrap(),vec![sel("app",1)]);
}
