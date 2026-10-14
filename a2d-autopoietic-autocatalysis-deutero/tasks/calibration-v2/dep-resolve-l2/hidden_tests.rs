
fn v(major:u32,minor:u32,patch:u32)->Version {Version{major,minor,patch}}
fn x(n:u32)->Version {v(n,0,0)}
fn e(n:u32)->Constraint {Constraint::Exact(x(n))}
fn ge(n:u32)->Constraint {Constraint::AtLeast(x(n))}
fn q(p:&str,c:Constraint)->Requirement {Requirement{package:p.into(),constraint:c}}
fn r(n:u32,deps:Vec<Requirement>)->Release {Release{version:x(n),deps}}
fn p(name:&str,releases:Vec<Release>)->Package {Package{name:name.into(),releases}}
fn sel(p:&str,n:u32)->Selection {Selection{package:p.into(),version:x(n)}}
fn run(c:&[Package],roots:&[Requirement])->Result<Vec<Selection>,ResolveError> {resolve(c,roots,100000)}
// Clauses: R1, R2, R10, R16.
#[test]
fn hidden_empty_and_unreachable() {
let c=vec![p("unused",vec![r(9,vec![q("absent",e(2))])])];
assert_eq!(run(&c,&[]).unwrap(),vec![]);assert_eq!(run(&[],&[]).unwrap(),vec![]);
}

// Clauses: R3, R7, R18.
#[test]
fn hidden_name_boundaries() {
let good="a12345678901234567890123";
assert_eq!(run(&[p(good,vec![r(1,vec![])])],&[q(good,e(1))]).unwrap()[0].package,good);
for bad in ["", "A", "a_", "a-", "0abc", "é", "a123456789012345678901234"] {
assert_eq!(run(&[],&[q(bad,e(1))]).unwrap_err(),ResolveError::InvalidName{site:"root[0]".into(),name:bad.into()});}
assert_eq!(run(&[],&[q("a-b9",e(1))]).unwrap_err(),ResolveError::Missing{package:"a-b9".into()} );
}

// Clauses: R7, R8.
#[test]
fn hidden_root_name_first() {
let c=vec![p("Bad",vec![]),p("Bad",vec![])];
assert_eq!(run(&c,&[q("ok",e(1)),q("!",e(1))]).unwrap_err(),ResolveError::InvalidName{site:"root[1]".into(),name:"!".into()});
}

// Clauses: R7, R8, R9, R10.
#[test]
fn hidden_unreachable_name_before_duplicates() {
let c=vec![p("same",vec![r(1,vec![]),r(1,vec![])]),p("same",vec![r(2,vec![q("a/b",e(1))])])];
assert_eq!(run(&c,&[]).unwrap_err(),ResolveError::InvalidName{site:"catalog[1].release[0].dep[0]".into(),name:"a/b".into()});
}

// Clauses: R8, R9.
#[test]
fn hidden_duplicate_package_precedence() {
let c=vec![p("z",vec![r(1,vec![]),r(1,vec![])]),p("b",vec![]),p("b",vec![]),p("z",vec![])];
assert_eq!(run(&c,&[]).unwrap_err(),ResolveError::DuplicatePackage{package:"b".into()});
}

// Clauses: R9, R10.
#[test]
fn hidden_duplicate_version_input_order() {
let c=vec![p("z",vec![r(3,vec![]),r(2,vec![]),r(3,vec![q("other",e(1))])]),p("a",vec![r(1,vec![]),r(1,vec![])])];
assert_eq!(run(&c,&[]).unwrap_err(),ResolveError::DuplicateVersion{package:"z".into(),version:x(3)});
}

// Clauses: R5, R18, R19, R20.
#[test]
fn hidden_canonical_constraints_and_utf8() {
let error=ResolveError::NoMatch{package:"a|b".into(),requirements:vec![Constraint::Caret(x(2)),Constraint::Before(x(4)),ge(2),e(3),e(1),e(3)]};
assert_eq!(error_text(&error),"no-match|package=a%7Cb|constraints==1.0.0,=3.0.0,>=2.0.0,<4.0.0,^2.0.0");
assert_eq!(error_text(&ResolveError::InvalidName{site:"é[%]".into(),name:"
/".into()}),"invalid-name|site=%C3%A9%5B%25%5D|name=%0A%2F");
}

// Clauses: R4, R15.
#[test]
fn hidden_caret_boundaries_and_overflow() {
for (base,yes,no) in [(v(4,7,1),v(4,99,9),v(5,0,0)),(v(0,6,2),v(0,6,8),v(0,7,0)),(v(u32::MAX,9,0),v(u32::MAX,u32::MAX,u32::MAX),v(u32::MAX,8,9))] {
let mut a=r(1,vec![]);a.version=yes;let mut b=r(1,vec![]);b.version=no;
let out=run(&[p("range",vec![b,a])],&[q("range",Constraint::Caret(base))]).unwrap();assert_eq!(out[0].version,yes);}
let mut a=r(1,vec![]);a.version=v(7,1,0);let mut b=r(1,vec![]);b.version=v(7,0,99);
assert_eq!(run(&[p("edge",vec![a,b])],&[q("edge",Constraint::Before(v(7,1,0))),q("edge",Constraint::AtLeast(v(7,0,1)))]).unwrap()[0].version,v(7,0,99));
}

// Clauses: R4.
#[test]
fn hidden_zero_patch_caret() {
for patch in [0,4,u32::MAX] {let mut a=r(1,vec![]);a.version=v(0,0,patch);let mut b=r(1,vec![]);b.version=v(0,1,0);
let out=run(&[p("tiny",vec![b,a])],&[q("tiny",Constraint::Caret(v(0,0,patch)))]).unwrap();assert_eq!(out[0].version,v(0,0,patch));}
let mut a=r(1,vec![]);a.version=v(0,0,5);let mut b=r(1,vec![]);b.version=v(0,0,4);
assert_eq!(run(&[p("tiny",vec![a,b])],&[q("tiny",Constraint::Caret(v(0,0,4)))]).unwrap()[0].version,v(0,0,4));
}

// Clauses: R5, R6, R14, R17.
#[test]
fn hidden_root_conjunction_no_match() {
let err=run(&[p("combo",vec![r(1,vec![]),r(3,vec![])])],&[q("combo",ge(2)),q("combo",Constraint::Before(x(3))),q("combo",ge(2))]).unwrap_err();
assert!(matches!(err,ResolveError::NoMatch{package,requirements,..} if package=="combo" && requirements==vec![ge(2),Constraint::Before(x(3))]));
}

// Clauses: R12, R14.
#[test]
fn hidden_missing_smallest_before_nomatch() {
let c=vec![p("a",vec![])];let err=run(&c,&[q("z",e(1)),q("b",e(1)),q("a",e(1))]).unwrap_err();
assert!(matches!(err,ResolveError::Missing{package,..} if package=="b"));
}

// Clauses: R11, R12, R13.
#[test]
fn hidden_missing_before_conflict() {
let c=vec![p("a",vec![r(1,vec![])]),p("b",vec![r(1,vec![q("a",e(2)),q("gone",e(1))])])];
assert!(matches!(run(&c,&[q("a",e(1)),q("b",e(1))]),Err(ResolveError::Missing{package,..}) if package=="gone"));
}

// Clauses: R13, R17.
#[test]
fn hidden_conflict_smallest_all_constraints() {
let c=vec![p("a",vec![r(1,vec![])]),p("b",vec![r(1,vec![])]),p("z",vec![r(1,vec![q("b",e(2)),q("a",ge(2)),q("a",ge(2))])])];
assert!(matches!(run(&c,&[q("z",e(1)),q("b",e(1)),q("a",e(1))]),Err(ResolveError::Conflict{package,selected,requirements,..}) if package=="a" && selected==x(1) && requirements==vec![e(1),ge(2)]));
}

// Clauses: R14, R15.
#[test]
fn hidden_all_nomatches_before_assignment() {
let c=vec![p("a",vec![r(1,vec![q("missing",e(1))])]),p("z",vec![]),p("b",vec![])];
assert!(matches!(run(&c,&[q("a",e(1)),q("z",e(1)),q("b",e(1))]),Err(ResolveError::NoMatch{package,requirements,..}) if package=="b" && requirements==vec![e(1)]));
}

// Clauses: R15, R16, R20.
#[test]
fn hidden_order_and_highest() {
let c=vec![p("z",vec![r(2,vec![]),r(7,vec![]),r(1,vec![])]),p("a",vec![r(3,vec![])]),p("unused",vec![r(99,vec![])])];
let roots=vec![q("z",ge(1)),q("a",e(3))];let mut reverse=c.clone();reverse.reverse();reverse[2].releases.reverse();
assert_eq!(run(&c,&roots).unwrap(),vec![sel("a",3),sel("z",7)]);assert_eq!(run(&c,&roots),run(&reverse,&roots));
}

// Clauses: R6, R11, R16.
#[test]
fn hidden_cycles_are_finite() {
let c=vec![p("c",vec![r(1,vec![q("a",e(1))])]),p("a",vec![r(1,vec![q("b",e(1)),q("a",e(1))])]),p("b",vec![r(1,vec![q("c",e(1))])])];
assert_eq!(run(&c,&[q("a",e(1))]).unwrap(),vec![sel("a",1),sel("b",1),sel("c",1)]);
}

// Clauses: R11, R13, R21, R26.
#[test]
fn hidden_self_constraint_after_assignment() {
let c=vec![p("self",vec![r(2,vec![q("self",e(1))]),r(1,vec![])])];assert_eq!(run(&c,&[q("self",ge(1))]).unwrap(),vec![sel("self",1)]);
}

// Clauses: R5, R6, R16, R20.
#[test]
fn hidden_duplicate_roots_and_input_immutability() {
let c=vec![p("repeat",vec![r(8,vec![])])];let roots=vec![q("repeat",ge(3)),q("repeat",e(8)),q("repeat",ge(3))];
let snapshot=(c.clone(),roots.clone());assert_eq!(run(&c,&roots).unwrap(),vec![sel("repeat",8)]);assert_eq!((c,roots),snapshot);
}

// Clauses: R17, R18, R19.
#[test]
fn hidden_render_all_base_variants() {
for (err,text) in [
(ResolveError::DuplicatePackage{package:"abc".into()},"duplicate-package|package=abc"),
(ResolveError::DuplicateVersion{package:"abc".into(),version:v(9,8,7)},"duplicate-version|package=abc|version=9.8.7"),
(ResolveError::Missing{package:"abc".into()},"missing|package=abc"),
(ResolveError::Conflict{package:"abc".into(),selected:x(2),requirements:vec![ge(1),e(3),ge(1)]},"conflict|package=abc|selected=2.0.0|constraints==3.0.0,>=1.0.0"),
(ResolveError::NoMatch{package:"".into(),requirements:vec![]},"no-match|package=|constraints=")] {assert_eq!(error_text(&err),text);}
}

// Clauses: R11, R16, R20.
#[test]
fn hidden_long_transitive_chain() {
let mut c=Vec::new();for i in 0..60 {let deps=if i==59 {vec![]} else {vec![q(&format!("node{:02}",i+1),e(1))]};c.push(p(&format!("node{:02}",i),vec![r(1,deps)]));}
let out=run(&c,&[q("node00",e(1))]).unwrap();assert_eq!(out.len(),60);for (i,s) in out.iter().enumerate() {assert_eq!(*s,sel(&format!("node{:02}",i),1));}
}

// Clauses: R21, R26.
#[test]
fn hidden_later_constraint_backtracks() {
let c=vec![p("a",vec![r(3,vec![]),r(2,vec![]),r(1,vec![])]),p("z",vec![r(1,vec![q("a",e(2))])])];
assert_eq!(run(&c,&[q("a",ge(1)),q("z",e(1))]).unwrap(),vec![sel("a",2),sel("z",1)]);
}

// Clauses: R21, R22.
#[test]
fn hidden_first_failing_leaf() {
let c=vec![p("app",vec![r(1,vec![q("aa",e(1))]),r(3,vec![q("zz",e(1))]),r(2,vec![q("bb",e(1))])])];
assert!(matches!(run(&c,&[q("app",ge(1))]),Err(ResolveError::Missing{package}) if package=="zz"));
}

// Clauses: R23, R24, R25.
#[test]
fn hidden_budget_exact_and_global() {
let c=vec![p("app",vec![r(3,vec![q("lost",e(1))]),r(2,vec![])])];
assert_eq!(resolve(&c,&[q("app",ge(1))],1),Err(ResolveError::Limit{attempts:1}));
assert_eq!(resolve(&c,&[q("app",ge(1))],2).unwrap(),vec![sel("app",2)]);
assert_eq!(error_text(&ResolveError::Limit{attempts:17}),"limit|attempts=17");
}

// Clauses: R23, R24, R25.
#[test]
fn hidden_zero_budget_and_final_leaf() {
assert_eq!(resolve(&[],&[],0).unwrap(),vec![]);
assert!(matches!(resolve(&[],&[q("lost",e(1))],0),Err(ResolveError::Missing{..})));
assert!(matches!(resolve(&[p("empty",vec![])],&[q("empty",e(1))],0),Err(ResolveError::NoMatch{..})));
let c=vec![p("only",vec![r(1,vec![q("lost",e(1))])])];assert!(matches!(resolve(&c,&[q("only",e(1))],1),Err(ResolveError::Missing{..})));
assert_eq!(resolve(&c,&[q("only",e(1))],0),Err(ResolveError::Limit{attempts:0}));
}

// Clauses: R21, R26.
#[test]
fn hidden_discard_branch_only_constraints() {
let c=vec![p("a",vec![r(4,vec![q("junk",e(1)),q("z",e(2))]),r(3,vec![q("z",e(1))])]),p("junk",vec![r(1,vec![])]),p("z",vec![r(1,vec![])])];
assert_eq!(run(&c,&[q("a",ge(1)),q("z",e(1))]).unwrap(),vec![sel("a",3),sel("z",1)]);
}

// Clauses: R15, R21.
#[test]
fn hidden_depth_first_not_global_highest() {
let c=vec![p("a",vec![r(2,vec![q("z",e(1))]),r(1,vec![q("z",e(9))])]),p("z",vec![r(9,vec![]),r(1,vec![])])];
assert_eq!(run(&c,&[q("a",ge(1))]).unwrap(),vec![sel("a",2),sel("z",1)]);
}
