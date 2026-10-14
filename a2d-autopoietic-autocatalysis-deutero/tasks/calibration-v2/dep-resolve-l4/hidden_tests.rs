
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
// Clauses: R1, R2, R10, R16.
#[test]
fn hidden_empty_and_unreachable() {
let c=vec![p("unused",vec![r(9,vec![d("absent",e(2))])])];
assert_eq!(run(&c,&[]).unwrap(),vec![]);assert_eq!(run(&[],&[]).unwrap(),vec![]);
}

// Clauses: R3, R7, R18.
#[test]
fn hidden_name_boundaries() {
let good="a12345678901234567890123";
assert_eq!(run(&[p(good,vec![r(1,vec![])])],&[q(good,e(1))]).unwrap()[0].package,good);
for bad in ["", "A", "a_", "a-", "0abc", "é", "a123456789012345678901234"] {assert_eq!(run(&[],&[q(bad,e(1))]).unwrap_err(),ResolveError::InvalidName{site:"root[0]".into(),name:bad.into()});}
assert!(matches!(run(&[],&[q("a-b9",e(1))]),Err(ResolveError::Missing{package,..}) if package=="a-b9"));
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
let c=vec![p("same",vec![r(1,vec![]),r(1,vec![])]),p("same",vec![r(2,vec![d("a/b",e(1))])])];
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
let c=vec![p("z",vec![r(3,vec![]),r(2,vec![]),r(3,vec![d("other",e(1))])]),p("a",vec![r(1,vec![]),r(1,vec![])])];
assert_eq!(run(&c,&[]).unwrap_err(),ResolveError::DuplicateVersion{package:"z".into(),version:x(3)});
}

// Clauses: R5, R18, R19, R20.
#[test]
fn hidden_canonical_constraints_and_utf8() {
let error=ResolveError::NoMatch{package:"a|b".into(),requirements:vec![Constraint::Caret(x(2)),Constraint::Before(x(4)),ge(2),e(3),e(1),e(3)],evidence:vec![]};
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
let c=vec![p("a",vec![r(1,vec![])]),p("b",vec![r(1,vec![d("a",e(2)),d("gone",e(1))])])];
assert!(matches!(run(&c,&[q("a",e(1)),q("b",e(1))]),Err(ResolveError::Missing{package,..}) if package=="gone"));
}

// Clauses: R13, R17.
#[test]
fn hidden_conflict_smallest_all_constraints() {
let c=vec![p("a",vec![r(1,vec![])]),p("b",vec![r(1,vec![])]),p("z",vec![r(1,vec![d("b",e(2)),d("a",ge(2)),d("a",ge(2))])])];
assert!(matches!(run(&c,&[q("z",e(1)),q("b",e(1)),q("a",e(1))]),Err(ResolveError::Conflict{package,selected,requirements,..}) if package=="a" && selected==x(1) && requirements==vec![e(1),ge(2)]));
}

// Clauses: R14, R15.
#[test]
fn hidden_all_nomatches_before_assignment() {
let c=vec![p("a",vec![r(1,vec![d("missing",e(1))])]),p("z",vec![]),p("b",vec![])];
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
let c=vec![p("c",vec![r(1,vec![d("a",e(1))])]),p("a",vec![r(1,vec![d("b",e(1)),d("a",e(1))])]),p("b",vec![r(1,vec![d("c",e(1))])])];
assert_eq!(run(&c,&[q("a",e(1))]).unwrap(),vec![sel("a",1),sel("b",1),sel("c",1)]);
}

// Clauses: R11, R13, R21, R26.
#[test]
fn hidden_self_constraint_after_assignment() {
let c=vec![p("self",vec![r(2,vec![d("self",e(1))]),r(1,vec![])])];assert_eq!(run(&c,&[q("self",ge(1))]).unwrap(),vec![sel("self",1)]);
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
(ResolveError::Missing{package:"abc".into(),evidence:vec![]},"missing|package=abc"),
(ResolveError::Conflict{package:"abc".into(),selected:x(2),requirements:vec![ge(1),e(3),ge(1)],evidence:vec![]},"conflict|package=abc|selected=2.0.0|constraints==3.0.0,>=1.0.0"),
(ResolveError::NoMatch{package:"".into(),requirements:vec![],evidence:vec![]},"no-match|package=|constraints=")] {assert_eq!(error_text(&err),text);}
}

// Clauses: R11, R16, R20.
#[test]
fn hidden_long_transitive_chain() {
let mut c=Vec::new();for i in 0..60 {let deps=if i==59 {vec![]} else {vec![d(&format!("node{:02}",i+1),e(1))]};c.push(p(&format!("node{:02}",i),vec![r(1,deps)]));}
let out=run(&c,&[q("node00",e(1))]).unwrap();assert_eq!(out.len(),60);for (i,s) in out.iter().enumerate() {assert_eq!(*s,sel(&format!("node{:02}",i),1));}
}

// Clauses: R18, R19, R25, R27, R28, R29, R34, R35, R7, R9.
#[test]
fn hidden_locks_validation_removal() {
{let c=vec![p("bad",vec![r(1,vec![]),r(1,vec![])])];
assert!(matches!(locked(&c,&[],&[lk("!",1)],0),Err(ResolveError::DuplicateVersion{..})));
assert_eq!(locked(&[],&[],&[lk("ok",1),lk("!",2),lk("ok",3)],0).unwrap_err(),ResolveError::InvalidName{site:"lock[1]".into(),name:"!".into()});}
{assert_eq!(locked(&[],&[],&[lk("z",1),lk("b",1),lk("b",2),lk("z",3)],0).unwrap_err(),ResolveError::DuplicateLock{package:"b".into()});
assert_eq!(error_text(&ResolveError::DuplicateLock{package:"b".into()}),"duplicate-lock|package=b");}
{let out=locked(&[],&[],&[lk("zombie",9),lk("absent",2)],0).unwrap();assert!(out.selected.is_empty());
assert_eq!(out.changes,vec![change("absent",Some(2),None,ChangeReason::Removed),change("zombie",Some(9),None,ChangeReason::Removed)]);}
}

// Clauses: R30, R31, R32.
#[test]
fn hidden_locks_global_objective() {
{let c=vec![p("a",vec![r(1,vec![d("b",e(2)),d("c",e(2))]),r(2,vec![d("b",e(1)),d("c",e(1))])]),p("b",vec![r(1,vec![]),r(2,vec![])]),p("c",vec![r(1,vec![]),r(2,vec![])])];
let out=locked(&c,&[q("a",ge(1))],&[lk("a",1),lk("b",1),lk("c",1)],100).unwrap();
assert_eq!(out.selected,vec![sel("a",2),sel("b",1),sel("c",1)]);assert_eq!(out.changes,vec![change("a",Some(1),Some(2),ChangeReason::Rebalanced)]);}
{let c=vec![p("z",vec![r(8,vec![])]),p("app",vec![r(1,vec![]),r(2,vec![d("z",e(8))])])];
assert_eq!(locked(&c,&[q("app",ge(1))],&[],20).unwrap().selected,vec![sel("app",2),sel("z",8)]);
let mut zero=r(1,vec![]);zero.version=v(0,0,0);
let c=vec![p("a",vec![zero]),p("z",vec![r(2,vec![]),r(1,vec![d("a",e(0))])])];
assert_eq!(locked(&c,&[q("z",ge(1))],&[],20).unwrap().selected,vec![sel("a",0),sel("z",1)]);}
}

// Clauses: R22, R23, R24, R25, R30, R33.
#[test]
fn hidden_locks_order_and_budget() {
{let c=vec![p("a",vec![r(2,vec![]),r(1,vec![])])];
assert_eq!(locked(&c,&[q("a",ge(1))],&[lk("a",1)],1),Err(ResolveError::Limit{attempts:1}));
assert_eq!(locked(&c,&[q("a",ge(1))],&[lk("a",1)],2).unwrap().selected,vec![sel("a",1)]);
assert_eq!(error_text(&ResolveError::Limit{attempts:7}),"limit|attempts=7");
assert_eq!(locked(&c,&[q("a",ge(1))],&[],0),Err(ResolveError::Limit{attempts:0}));
assert!(matches!(locked(&[],&[q("lost",e(1))],&[],0),Err(ResolveError::Missing{..})));
assert!(matches!(locked(&[p("a",vec![])],&[q("a",e(1))],&[],0),Err(ResolveError::NoMatch{..})));}
{let c=vec![p("a",vec![r(3,vec![d("highest",e(1))]),r(1,vec![d("locked",e(1))])])];
assert!(matches!(locked(&c,&[q("a",ge(1))],&[lk("a",1)],2),Err(ResolveError::Missing{package,..}) if package=="locked"));
assert_eq!(locked(&c,&[q("a",ge(1))],&[lk("a",1)],1),Err(ResolveError::Limit{attempts:1}));}
}

// Clauses: R21, R26, R28, R30, R31, R34, R35.
#[test]
fn hidden_locks_change_reasons() {
{let c=vec![p("a",vec![r(2,vec![])]),p("b",vec![r(1,vec![]),r(3,vec![])]),p("c",vec![r(1,vec![])])];
let out=locked(&c,&[q("a",e(2)),q("b",e(3)),q("c",e(1))],&[lk("old",1),lk("b",1),lk("a",9)],20).unwrap();
assert_eq!(out.changes,vec![change("a",Some(9),Some(2),ChangeReason::Unavailable),change("b",Some(1),Some(3),ChangeReason::Rejected),change("c",None,Some(1),ChangeReason::Added),change("old",Some(1),None,ChangeReason::Removed)]);}
{let c=vec![p("a",vec![r(1,vec![d("lost",e(1))]),r(2,vec![])])];
let out=locked(&c,&[q("a",ge(1))],&[lk("a",1)],10).unwrap();assert_eq!(out.changes,vec![change("a",Some(1),Some(2),ChangeReason::Rebalanced)]);}
{let c=vec![p("a",vec![r(3,vec![d("b",e(2)),d("lost",e(1))]),r(2,vec![d("b",e(1))])]),p("b",vec![r(1,vec![]),r(2,vec![])])];
let out=locked(&c,&[q("a",ge(1))],&[lk("a",3),lk("b",1)],20).unwrap();assert_eq!(out.selected,vec![sel("a",2),sel("b",1)]);
assert_eq!(out.changes,vec![change("a",Some(3),Some(2),ChangeReason::Rebalanced)]);}
}

// Clauses: R3, R7, R37, R38.
#[test]
fn hidden_feature_validation_and_duplicates() {
let mut rel=r(1,vec![d("ok",e(1))]);rel.deps[0].gate=Some("!".into());rel.features=vec![feat("same",&[]),feat("same",&[])];
assert_eq!(locked(&[p("app",vec![rel.clone()])],&[],&[],0).unwrap_err(),ResolveError::InvalidName{site:"catalog[0].release[0].dep[0].gate".into(),name:"!".into()});
rel.deps.clear();assert_eq!(locked(&[p("app",vec![rel.clone()])],&[],&[lk("!",1)],0).unwrap_err(),ResolveError::DuplicateFeature{package:"app".into(),version:x(1),feature:"same".into()});
assert_eq!(error_text(&ResolveError::DuplicateFeature{package:"app".into(),version:x(1),feature:"same".into()}),"duplicate-feature|package=app|version=1.0.0|feature=same");
rel.features=vec![feat("a",&["bad_"])];assert_eq!(locked(&[p("app",vec![rel])],&[],&[],0).unwrap_err(),ResolveError::InvalidName{site:"catalog[0].release[0].feature[0].enable[0]".into(),name:"bad_".into()});
let c=vec![p("app",vec![r(1,vec![])])];assert_eq!(run(&c,&[ask("app",e(1),&["!"])]).unwrap_err(),ResolveError::InvalidName{site:"root[0].feature[0]".into(),name:"!".into()});
let mut rel=r(1,vec![d("dest",e(1))]);rel.deps[0].features=vec!["bad!".into()];assert!(matches!(locked(&[p("app",vec![rel])],&[],&[],0),Err(ResolveError::InvalidName{site,..}) if site=="catalog[0].release[0].dep[0].feature[0]"));
}

// Clauses: R39, R40, R41.
#[test]
fn hidden_transitive_feature_cycle_no_default() {
let mut dep=d("leaf",e(1));dep.gate=Some("last".into());
let mut rel=r(1,vec![dep]);rel.features=vec![feat("first",&["middle","middle"]),feat("middle",&["last"]),feat("last",&["first"]),feat("default",&["last"])];
let c=vec![p("app",vec![rel]),p("leaf",vec![r(1,vec![])])];
assert_eq!(run(&c,&[q("app",e(1))]).unwrap(),vec![sel("app",1)]);
assert_eq!(run(&c,&[ask("app",e(1),&["first","first"])]).unwrap(),vec![sel("app",1),sel("leaf",1)]);
}

// Clauses: R36, R41.
#[test]
fn hidden_platform_and_gate_conjunction() {
let mut rel=r(1,vec![]);rel.features=vec![feat("on",&[])];
for (name,platform,gate) in [("linux",Platform::Linux,None),("mac",Platform::Mac,None),("windows",Platform::Windows,None),("any",Platform::Any,None),("optional",Platform::Any,Some("on")),("inactive",Platform::Windows,Some("never"))] {
let mut dep=d(name,e(1));dep.platform=platform;dep.gate=gate.map(str::to_string);rel.deps.push(dep);}
let mut c=vec![p("app",vec![rel])];for name in ["linux","mac","windows","any","optional"] {c.push(p(name,vec![r(1,vec![])]));}
for (target,name) in [(Target::Linux,"linux"),(Target::Mac,"mac"),(Target::Windows,"windows")] {
assert_eq!(resolve(&c,&[q("app",e(1))],&[],target,20).unwrap().selected,vec![sel("any",1),sel("app",1),sel(name,1)]);
let mut expected=vec![sel("any",1),sel("app",1),sel(name,1),sel("optional",1)];expected.sort_by(|a,b|a.package.cmp(&b.package));
assert_eq!(resolve(&c,&[ask("app",e(1),&["on"])],&[],target,20).unwrap().selected,expected);}
}

// Clauses: R39, R40, R42.
#[test]
fn hidden_late_requests_joint_fixed_point() {
let mut a=r(1,vec![d("b",e(1))]);a.features=vec![feat("extra",&[])];let mut late=d("leaf",e(1));late.gate=Some("extra".into());a.deps.push(late);
let mut back=d("a",e(1));back.features=vec!["extra".into(),"extra".into()];
let c=vec![p("a",vec![a]),p("b",vec![r(1,vec![back])]),p("leaf",vec![r(1,vec![])])];
assert_eq!(run(&c,&[q("a",e(1))]).unwrap(),vec![sel("a",1),sel("b",1),sel("leaf",1)]);
}

// Clauses: R12, R13, R43.
#[test]
fn hidden_missing_feature_precedence() {
let mut a=r(1,vec![]);a.features=vec![feat("go",&["zeta","alpha"])];
let c=vec![p("a",vec![a.clone()])];let err=run(&c,&[ask("a",e(1),&["go"])]).unwrap_err();assert!(matches!(err,ResolveError::MissingFeature{feature,..} if feature=="alpha"));
a.deps=vec![d("a",e(2)),d("lost",e(1))];assert!(matches!(run(&[p("a",vec![a.clone()])],&[ask("a",e(1),&["go"])]),Err(ResolveError::Missing{..})));
a.deps.pop();assert!(matches!(run(&[p("a",vec![a])],&[ask("a",e(1),&["go"])]),Err(ResolveError::MissingFeature{..})));
let mut z=r(1,vec![d("a",e(1))]);z.deps[0].features=vec!["zzz".into()];z.deps.push(d("z",e(1)));z.deps[1].features=vec!["aaa".into()];
assert!(matches!(run(&[p("a",vec![r(1,vec![])]),p("z",vec![z])],&[q("a",e(1)),q("z",e(1))]),Err(ResolveError::MissingFeature{package,feature,..}) if package=="a" && feature=="zzz"));
}

// Clauses: R26, R42, R44.
#[test]
fn hidden_feature_backtrack_discards_activation() {
let mut bad=r(3,vec![d("b",e(1)),d("lost",e(1))]);bad.deps[0].features=vec!["extra".into()];
let mut b=r(1,vec![]);b.features=vec![feat("extra",&[])];let mut edge=d("only-bad",e(1));edge.gate=Some("extra".into());b.deps.push(edge);
let c=vec![p("a",vec![bad,r(2,vec![d("b",e(1))])]),p("b",vec![b])];
assert_eq!(run(&c,&[q("a",ge(1))]).unwrap(),vec![sel("a",2),sel("b",1)]);
let mut good=r(1,vec![]);good.features=vec![feat("needed",&[])];let c=vec![p("a",vec![r(2,vec![]),good])];
assert_eq!(locked(&c,&[ask("a",ge(1),&["needed"])],&[lk("a",2)],10).unwrap().changes,vec![change("a",Some(2),Some(1),ChangeReason::Rebalanced)]);
}

// Clauses: R45, R46, R47.
#[test]
fn hidden_evidence_shortest_and_all_origins() {
let c=vec![p("a",vec![r(1,vec![d("b",e(1))])]),p("b",vec![r(1,vec![d("lost",e(3))])])];
let err=run(&c,&[q("a",e(1)),q("b",e(1)),q("b",e(1))]).unwrap_err();
assert_eq!(error_text(&err),"missing|package=lost
  C:=3.0.0 via root[1]:b@=1.0.0+{} -> b@1.0.0/deps[0]:lost@=3.0.0+{};gate=*;platform=any");
let err=run(&[],&[q("lost",ge(4)),q("lost",e(3)),q("lost",e(3))]).unwrap_err();
assert_eq!(error_text(&err),"missing|package=lost
  C:=3.0.0 via root[1]:lost@=3.0.0+{}
  C:=3.0.0 via root[2]:lost@=3.0.0+{}
  C:>=4.0.0 via root[0]:lost@>=4.0.0+{}");
}

// Clauses: R18, R45, R47, R48.
#[test]
fn hidden_evidence_lexical_tie_and_renderer() {
let c=vec![p("a",vec![r(1,vec![d("middle",e(1))])]),p("b",vec![r(1,vec![d("middle",e(1))])]),p("middle",vec![r(1,vec![d("lost",e(1))])])];
let err=run(&c,&[q("b",e(1)),q("a",e(1))]).unwrap_err();
assert_eq!(error_text(&err),"missing|package=lost
  C:=1.0.0 via root[0]:b@=1.0.0+{} -> b@1.0.0/deps[0]:middle@=1.0.0+{};gate=*;platform=any -> middle@1.0.0/deps[0]:lost@=1.0.0+{};gate=*;platform=any");
let e1=Evidence{constraint:e(1),path:vec!["z|raw".into()]};let e2=Evidence{constraint:e(1),path:vec!["a".into(),"b".into()]};
let err=ResolveError::Missing{package:"a/b".into(),evidence:vec![e1.clone(),e2.clone(),e2]};
assert_eq!(error_text(&err),"missing|package=a%2Fb
  C:=1.0.0 via a -> b
  C:=1.0.0 via a -> b
  C:=1.0.0 via z|raw");
}

// Clauses: R13, R17, R41, R46, R47, R48.
#[test]
fn hidden_exact_conflict_derivation() {
let mut edge=d("a",e(2));edge.gate=Some("extra".into());edge.platform=Platform::Linux;edge.features=vec!["z".into(),"z".into(),"b".into()];
let mut a=r(1,vec![]);a.features=vec![feat("b",&[]),feat("z",&[])];let mut source=r(1,vec![edge]);source.features=vec![feat("extra",&[])];
let c=vec![p("a",vec![a]),p("source",vec![source])];
let err=run(&c,&[q("a",e(1)),ask("source",e(1),&["extra","extra"])]).unwrap_err();
assert_eq!(error_text(&err),"conflict|package=a|selected=1.0.0|constraints==1.0.0,=2.0.0
  C:=1.0.0 via root[0]:a@=1.0.0+{}
  C:=2.0.0 via root[1]:source@=1.0.0+{extra} -> source@1.0.0/deps[0]:a@=2.0.0+{b,z};gate=extra;platform=linux");
let err=run(&[p("none",vec![])],&[q("none",e(7))]).unwrap_err();assert_eq!(error_text(&err),"no-match|package=none|constraints==7.0.0
  C:=7.0.0 via root[0]:none@=7.0.0+{}");
}

// Clauses: R21, R22, R23, R24, R43, R44, R48.
#[test]
fn hidden_feature_attempts_no_prefilter() {
let mut good=r(1,vec![]);good.features=vec![feat("yes",&[])];let c=vec![p("a",vec![good,r(2,vec![])])];
assert_eq!(locked(&c,&[ask("a",ge(1),&["yes"])],&[],1),Err(ResolveError::Limit{attempts:1}));
assert_eq!(locked(&c,&[ask("a",ge(1),&["yes"])],&[],2).unwrap().selected,vec![sel("a",1)]);
let c=vec![p("a",vec![r(1,vec![]),r(2,vec![])])];let err=locked(&c,&[ask("a",ge(1),&["bad"])],&[],2).unwrap_err();
assert_eq!(error_text(&err),"missing-feature|package=a|selected=2.0.0|feature=bad
  F:bad via root[0]:a@>=1.0.0+{bad}");
assert!(matches!(locked(&[p("a",vec![])],&[ask("a",e(1),&["bad"])],&[],0),Err(ResolveError::NoMatch{..})));
}

// Clauses: R39, R40, R42, R45, R46, R48.
#[test]
fn hidden_cyclic_paths_and_root_feature_union() {
let mut back=d("a",e(1));back.features=vec!["unknown".into()];let c=vec![p("a",vec![r(1,vec![d("b",e(1))])]),p("b",vec![r(1,vec![back])])];
let err=run(&c,&[q("a",e(1))]).unwrap_err();assert_eq!(error_text(&err),"missing-feature|package=a|selected=1.0.0|feature=unknown
  F:unknown via root[0]:a@=1.0.0+{}");
let mut rel=r(1,vec![]);rel.features=vec![feat("a",&[]),feat("b",&[])];let mut edge=d("lost",e(1));edge.gate=Some("b".into());rel.deps.push(edge);
let err=run(&[p("p",vec![rel])],&[ask("p",e(1),&["b","a","b"]),ask("p",e(1),&["a"])]).unwrap_err();
assert_eq!(error_text(&err),"missing|package=lost
  C:=1.0.0 via root[0]:p@=1.0.0+{a,b} -> p@1.0.0/deps[0]:lost@=1.0.0+{};gate=b;platform=any");
}
