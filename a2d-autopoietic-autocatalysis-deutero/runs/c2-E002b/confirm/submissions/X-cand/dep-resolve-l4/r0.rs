#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version { pub major: u32, pub minor: u32, pub patch: u32 }
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Constraint { Exact(Version), AtLeast(Version), Before(Version), Caret(Version) }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection { pub package: String, pub version: Version }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package { pub name: String, pub releases: Vec<Release> }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target { Linux, Mac, Windows }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform { Any, Linux, Mac, Windows }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Requirement { pub package: String, pub constraint: Constraint, pub features: Vec<String> }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dependency { pub package: String, pub constraint: Constraint, pub features: Vec<String>, pub gate: Option<String>, pub platform: Platform }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Feature { pub name: String, pub enables: Vec<String> }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release { pub version: Version, pub deps: Vec<Dependency>, pub features: Vec<Feature> }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Evidence { pub constraint: Constraint, pub path: Vec<String> }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lock { pub package: String, pub version: Version }
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangeReason { Added, Removed, Unavailable, Rejected, Rebalanced }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change { pub package: String, pub before: Option<Version>, pub after: Option<Version>, pub reason: ChangeReason }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolution { pub selected: Vec<Selection>, pub changes: Vec<Change> }
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolveError {
    InvalidName { site: String, name: String },
    DuplicatePackage { package: String },
    DuplicateVersion { package: String, version: Version },
    DuplicateFeature { package: String, version: Version, feature: String },
    DuplicateLock { package: String },
    Missing { package: String, evidence: Vec<Evidence> },
    NoMatch { package: String, requirements: Vec<Constraint>, evidence: Vec<Evidence> },
    Conflict { package: String, selected: Version, requirements: Vec<Constraint>, evidence: Vec<Evidence> },
    MissingFeature { package: String, selected: Version, feature: String, path: Vec<String> },
    Limit { attempts: usize },
}

fn valid_name(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty() && b.len() <= 24 && b[0].is_ascii_lowercase()
        && b[b.len()-1] != b'-'
        && b.iter().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

fn version_text(v: Version) -> String { format!("{}.{}.{}", v.major, v.minor, v.patch) }

fn constraint_key(c: &Constraint) -> (u8, Version) {
    match *c {
        Constraint::Exact(v) => (0, v),
        Constraint::AtLeast(v) => (1, v),
        Constraint::Before(v) => (2, v),
        Constraint::Caret(v) => (3, v),
    }
}

fn canonical(mut cs: Vec<Constraint>) -> Vec<Constraint> {
    cs.sort_by_key(constraint_key);
    cs.dedup();
    cs
}

fn constraint_text(c: Constraint) -> String {
    match c {
        Constraint::Exact(v) => format!("={}", version_text(v)),
        Constraint::AtLeast(v) => format!(">={}", version_text(v)),
        Constraint::Before(v) => format!("<{}", version_text(v)),
        Constraint::Caret(v) => format!("^{}", version_text(v)),
    }
}

fn constraints_text(cs: &[Constraint]) -> String {
    canonical(cs.to_vec()).iter().map(|c| constraint_text(*c)).collect::<Vec<_>>().join(",")
}

fn permits(c: Constraint, v: Version) -> bool {
    match c {
        Constraint::Exact(x) => v == x,
        Constraint::AtLeast(x) => v >= x,
        Constraint::Before(x) => v < x,
        Constraint::Caret(x) => {
            v >= x && if x.major > 0 { v.major == x.major }
            else if x.minor > 0 { v.major == 0 && v.minor == x.minor }
            else { v == x }
        }
    }
}

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-' {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

fn sort_evidence(e: &mut Vec<Evidence>) {
    e.sort_by(|a,b| constraint_key(&a.constraint).cmp(&constraint_key(&b.constraint))
        .then_with(|| a.path.cmp(&b.path)));
}

pub fn error_text(error: &ResolveError) -> String {
    match error {
        ResolveError::InvalidName{site,name} => format!("invalid-name|site={}|name={}",encode(site),encode(name)),
        ResolveError::DuplicatePackage{package} => format!("duplicate-package|package={}",encode(package)),
        ResolveError::DuplicateVersion{package,version} => format!("duplicate-version|package={}|version={}",encode(package),version_text(*version)),
        ResolveError::DuplicateFeature{package,version,feature} => format!("duplicate-feature|package={}|version={}|feature={}",encode(package),version_text(*version),encode(feature)),
        ResolveError::DuplicateLock{package} => format!("duplicate-lock|package={}",encode(package)),
        ResolveError::Missing{package,evidence} => {
            let mut e=evidence.clone(); sort_evidence(&mut e);
            let mut s=format!("missing|package={}",encode(package));
            for x in e { s.push_str(&format!("\n  C:{} via {}",constraint_text(x.constraint),x.path.join(" -> "))); }
            s
        }
        ResolveError::NoMatch{package,requirements,evidence} => {
            let mut e=evidence.clone(); sort_evidence(&mut e);
            let mut s=format!("no-match|package={}|constraints={}",encode(package),constraints_text(requirements));
            for x in e { s.push_str(&format!("\n  C:{} via {}",constraint_text(x.constraint),x.path.join(" -> "))); }
            s
        }
        ResolveError::Conflict{package,selected,requirements,evidence} => {
            let mut e=evidence.clone(); sort_evidence(&mut e);
            let mut s=format!("conflict|package={}|selected={}|constraints={}",encode(package),version_text(*selected),constraints_text(requirements));
            for x in e { s.push_str(&format!("\n  C:{} via {}",constraint_text(x.constraint),x.path.join(" -> "))); }
            s
        }
        ResolveError::MissingFeature{package,selected,feature,path} =>
            format!("missing-feature|package={}|selected={}|feature={}\n  F:{} via {}",encode(package),version_text(*selected),encode(feature),encode(feature),path.join(" -> ")),
        ResolveError::Limit{attempts} => format!("limit|attempts={}",attempts),
    }
}

#[derive(Clone)]
struct Origin { constraint: Constraint, path: Vec<String> }
#[derive(Clone)]
struct Closure {
    constraints: BTreeMap<String, Vec<Constraint>>,
    origins: BTreeMap<String, Vec<Origin>>,
    features: BTreeMap<String, BTreeSet<String>>,
    paths: BTreeMap<String, Vec<String>>,
}

use std::collections::{BTreeMap, BTreeSet};

fn platform_matches(p: Platform, t: Target) -> bool {
    matches!(p, Platform::Any) || matches!((p,t),
        (Platform::Linux,Target::Linux)|(Platform::Mac,Target::Mac)|(Platform::Windows,Target::Windows))
}

fn dep_token(src: &str, ver: Version, j: usize, d: &Dependency) -> String {
    let gate=d.gate.as_deref().unwrap_or("*");
    let platform=match d.platform { Platform::Any=>"any",Platform::Linux=>"linux",Platform::Mac=>"mac",Platform::Windows=>"windows" };
    let mut fs=d.features.clone(); fs.sort(); fs.dedup();
    format!("{}@{}/deps[{}]:{}@{}+{{}};gate={};platform={}",
        src,version_text(ver),j,d.package,constraint_text(d.constraint),gate,platform)
        .replace("+{}", &format!("+{{{}}}",fs.join(",")))
}

fn root_token(i: usize, r: &Requirement) -> String {
    let mut fs=r.features.clone(); fs.sort(); fs.dedup();
    format!("root[{}]:{}@{}+{{{}}}",i,r.package,constraint_text(r.constraint),fs.join(","))
}

fn compute_closure(
    roots: &[Requirement], selected: &BTreeMap<String, Version>,
    catalog: &BTreeMap<String, &Package>, target: Target
) -> Closure {
    let mut c=Closure { constraints:BTreeMap::new(), origins:BTreeMap::new(), features:BTreeMap::new(), paths:BTreeMap::new() };
    let mut pending: Vec<(String,String,Vec<String>)> = Vec::new();
    for (i,r) in roots.iter().enumerate() {
        let path=vec![root_token(i,r)];
        c.constraints.entry(r.package.clone()).or_default().push(r.constraint);
        c.origins.entry(r.package.clone()).or_default().push(Origin{constraint:r.constraint,path:path.clone()});
        c.paths.entry(r.package.clone()).and_modify(|p| if path.len()<p.len() || (path.len()==p.len() && path<*p) {*p=path.clone()}).or_insert(path);
        for f in &r.features { pending.push((r.package.clone(),f.clone(),vec![])); }
    }
    let mut processed_features: BTreeSet<(String,String)> = BTreeSet::new();
    let mut processed_deps: BTreeSet<(String,usize)> = BTreeSet::new();

    loop {
        let mut changed=false;
        while let Some((pkg,feature,_))=pending.pop() {
            if c.features.entry(pkg.clone()).or_default().insert(feature.clone()) { changed=true; }
        }
        for (pkg,ver) in selected {
            let Some(package)=catalog.get(pkg) else { continue };
            let Some(release)=package.releases.iter().find(|r|r.version==*ver) else {continue};
            let active_features=c.features.get(pkg).cloned().unwrap_or_default();
            for f in &release.features {
                if active_features.contains(&f.name) && processed_features.insert((pkg.clone(),f.name.clone())) {
                    for en in &f.enables { pending.push((pkg.clone(),en.clone(),vec![])); }
                }
            }
            for (j,d) in release.deps.iter().enumerate() {
                if !platform_matches(d.platform,target) || d.gate.as_ref().is_some_and(|g| !active_features.contains(g)) { continue; }
                if processed_deps.insert((pkg.clone(),j)) {
                    let srcpath=c.paths.get(pkg).cloned().unwrap_or_default();
                    let token=dep_token(pkg,*ver,j,d);
                    let mut path=srcpath; path.push(token);
                    c.constraints.entry(d.package.clone()).or_default().push(d.constraint);
                    c.origins.entry(d.package.clone()).or_default().push(Origin{constraint:d.constraint,path:path.clone()});
                    c.paths.entry(d.package.clone()).and_modify(|old| if path.len()<old.len() || (path.len()==old.len() && path<*old) {*old=path.clone()}).or_insert(path);
                    for f in &d.features { pending.push((d.package.clone(),f.clone(),vec![])); }
                    changed=true;
                }
            }
        }
        if pending.is_empty() && !changed { break; }
        if pending.is_empty() {
            // Continue once to detect newly enabled dependency edges.
            let before=processed_deps.len()+processed_features.len();
            for (pkg,ver) in selected {
                if let Some(package)=catalog.get(pkg) {
                    if let Some(release)=package.releases.iter().find(|r|r.version==*ver) {
                        let af=c.features.get(pkg).cloned().unwrap_or_default();
                        for f in &release.features {
                            if af.contains(&f.name) && processed_features.insert((pkg.clone(),f.name.clone())) {
                                for en in &f.enables { pending.push((pkg.clone(),en.clone(),vec![])); }
                            }
                        }
                        for (j,d) in release.deps.iter().enumerate() {
                            if platform_matches(d.platform,target) && d.gate.as_ref().is_none_or(|g|af.contains(g)) && processed_deps.insert((pkg.clone(),j)) {
                                let path0=c.paths.get(pkg).cloned().unwrap_or_default();
                                let mut path=path0; path.push(dep_token(pkg,*ver,j,d));
                                c.constraints.entry(d.package.clone()).or_default().push(d.constraint);
                                c.origins.entry(d.package.clone()).or_default().push(Origin{constraint:d.constraint,path:path.clone()});
                                c.paths.entry(d.package.clone()).and_modify(|old|if path.len()<old.len()||(path.len()==old.len()&&path<*old){*old=path.clone()}).or_insert(path);
                                for f in &d.features { pending.push((d.package.clone(),f.clone(),vec![])); }
                            }
                        }
                    }
                }
            }
            if pending.is_empty() && before==processed_deps.len()+processed_features.len() { break; }
        }
    }
    for cs in c.constraints.values_mut() { *cs=canonical(std::mem::take(cs)); }
    c
}

fn evidence_for(c: &Closure, p: &str) -> Vec<Evidence> {
    let mut out=c.origins.get(p).into_iter().flatten().map(|o|Evidence{constraint:o.constraint,path:o.path.clone()}).collect::<Vec<_>>();
    sort_evidence(&mut out); out
}

struct Search<'a> {
    roots: &'a [Requirement],
    catalog: &'a BTreeMap<String,&'a Package>,
    locks: &'a BTreeMap<String,Version>,
    target: Target,
    budget: usize,
    attempts: usize,
    names: Vec<String>,
    best: Option<(usize,Vec<Version>,BTreeMap<String,Version>)>,
    first_error: Option<ResolveError>,
    limit: bool,
}

impl<'a> Search<'a> {
    fn visit(&mut self, selected: &mut BTreeMap<String,Version>) {
        if self.limit { return; }
        let cl=compute_closure(self.roots,selected,self.catalog,self.target);
        let active: BTreeSet<String>=cl.constraints.keys().cloned().collect();

        for p in active.iter() {
            if !self.catalog.contains_key(p) {
                let err=ResolveError::Missing{package:p.clone(),evidence:evidence_for(&cl,p)};
                if self.first_error.is_none(){self.first_error=Some(err);} return;
            }
        }
        for (p,v) in selected.iter() {
            let fs=cl.features.get(p).cloned().unwrap_or_default();
            if let Some(pkg)=self.catalog.get(p) {
                if let Some(rel)=pkg.releases.iter().find(|r|r.version==*v) {
                    for f in fs {
                        if !rel.features.iter().any(|x|x.name==f) {
                            let path=cl.paths.get(p).cloned().unwrap_or_default();
                            let err=ResolveError::MissingFeature{package:p.clone(),selected:*v,feature:f,path};
                            if self.first_error.is_none(){self.first_error=Some(err);} return;
                        }
                    }
                }
            }
        }
        for (p,v) in selected.iter() {
            let cs=cl.constraints.get(p).cloned().unwrap_or_default();
            if cs.iter().any(|x|!permits(*x,*v)) {
                let err=ResolveError::Conflict{package:p.clone(),selected:*v,requirements:cs,evidence:evidence_for(&cl,p)};
                if self.first_error.is_none(){self.first_error=Some(err);} return;
            }
        }
        for p in active.iter().filter(|p| !selected.contains_key(*p)) {
            let cs=cl.constraints.get(p).cloned().unwrap_or_default();
            let pkg=self.catalog[p];
            if !pkg.releases.iter().any(|r|cs.iter().all(|x|permits(*x,r.version))) {
                let err=ResolveError::NoMatch{package:p.clone(),requirements:cs,evidence:evidence_for(&cl,p)};
                if self.first_error.is_none(){self.first_error=Some(err);} return;
            }
        }
        let next=active.iter().find(|p|!selected.contains_key(*p)).cloned();
        if let Some(p)=next {
            let cs=cl.constraints.get(&p).cloned().unwrap_or_default();
            let pkg=self.catalog[&p];
            let mut candidates=pkg.releases.iter().filter(|r|cs.iter().all(|x|permits(*x,r.version))).map(|r|r.version).collect::<Vec<_>>();
            candidates.sort_by(|a,b|b.cmp(a));
            if let Some(lv)=self.locks.get(&p) {
                if let Some(i)=candidates.iter().position(|v|v==lv) { let x=candidates.remove(i); candidates.insert(0,x); }
            }
            for v in candidates {
                if self.attempts==self.budget { self.limit=true; return; }
                self.attempts+=1;
                selected.insert(p.clone(),v);
                self.visit(selected);
                selected.remove(&p);
                if self.limit { return; }
            }
            return;
        }

        let cost=self.locks.iter().filter(|(p,v)|selected.get(*p)!=Some(*v)).count();
        let vector=self.names.iter().map(|n|selected.get(n).copied().unwrap_or(Version{major:0,minor:0,patch:0})).collect::<Vec<_>>();
        let is_better=self.best.as_ref().is_none_or(|(bc,bv,_)|cost<*bc || (cost==*bc && vector>*bv));
        if is_better { self.best=Some((cost,vector,selected.clone())); }
    }
}

pub fn resolve(catalog: &[Package], roots: &[Requirement], locks: &[Lock], target: Target, max_attempts: usize) -> Result<Resolution, ResolveError> {
    for (i,r) in roots.iter().enumerate() {
        if !valid_name(&r.package) { return Err(ResolveError::InvalidName{site:format!("root[{}]",i),name:r.package.clone()}); }
        for (f,name) in r.features.iter().enumerate() {
            if !valid_name(name) { return Err(ResolveError::InvalidName{site:format!("root[{}].feature[{}]",i,f),name:name.clone()}); }
        }
    }
    for (i,p) in catalog.iter().enumerate() {
        if !valid_name(&p.name) { return Err(ResolveError::InvalidName{site:format!("catalog[{}]",i),name:p.name.clone()}); }
        for (j,r) in p.releases.iter().enumerate() {
            for (k,d) in r.deps.iter().enumerate() {
                if !valid_name(&d.package) { return Err(ResolveError::InvalidName{site:format!("catalog[{}].release[{}].dep[{}]",i,j,k),name:d.package.clone()}); }
                if let Some(g)=&d.gate { if !valid_name(g) { return Err(ResolveError::InvalidName{site:format!("catalog[{}].release[{}].dep[{}].gate",i,j,k),name:g.clone()}); } }
                for (f,n) in d.features.iter().enumerate() {
                    if !valid_name(n) { return Err(ResolveError::InvalidName{site:format!("catalog[{}].release[{}].dep[{}].feature[{}]",i,j,k,f),name:n.clone()}); }
                }
            }
            for (f,feat) in r.features.iter().enumerate() {
                if !valid_name(&feat.name) { return Err(ResolveError::InvalidName{site:format!("catalog[{}].release[{}].feature[{}]",i,j,f),name:feat.name.clone()}); }
                for (e,n) in feat.enables.iter().enumerate() {
                    if !valid_name(n) { return Err(ResolveError::InvalidName{site:format!("catalog[{}].release[{}].feature[{}].enable[{}]",i,j,f,e),name:n.clone()}); }
                }
            }
        }
    }
    let mut seen=BTreeSet::new();
    for p in catalog {
        if !seen.insert(p.name.clone()) { return Err(ResolveError::DuplicatePackage{package:p.name.clone()}); }
    }
    for p in catalog {
        let mut versions=BTreeSet::new();
        for r in &p.releases {
            if !versions.insert(r.version) { return Err(ResolveError::DuplicateVersion{package:p.name.clone(),version:r.version}); }
        }
    }
    for p in catalog {
        for r in &p.releases {
            let mut fs=BTreeSet::new();
            for f in &r.features {
                if !fs.insert(f.name.clone()) { return Err(ResolveError::DuplicateFeature{package:p.name.clone(),version:r.version,feature:f.name.clone()}); }
            }
        }
    }
    for (i,l) in locks.iter().enumerate() {
        if !valid_name(&l.package) { return Err(ResolveError::InvalidName{site:format!("lock[{}]",i),name:l.package.clone()}); }
    }
    let mut ls=BTreeSet::new();
    let mut lockmap=BTreeMap::new();
    for l in locks {
        if !ls.insert(l.package.clone()) { return Err(ResolveError::DuplicateLock{package:l.package.clone()}); }
        lockmap.insert(l.package.clone(),l.version);
    }
    let cmap=catalog.iter().map(|p|(p.name.clone(),p)).collect::<BTreeMap<_,_>>();
    let mut names=catalog.iter().map(|p|p.name.clone()).collect::<Vec<_>>();
    names.sort();
    let mut search=Search{roots,catalog:&cmap,locks:&lockmap,target,budget:max_attempts,attempts:0,names,best:None,first_error:None,limit:false};
    let mut selected=BTreeMap::new();
    search.visit(&mut selected);
    if search.limit { return Err(ResolveError::Limit{attempts:max_attempts}); }
    let chosen=if let Some((_,_,s))=search.best { s } else { return Err(search.first_error.unwrap_or(ResolveError::NoMatch{package:String::new(),requirements:vec![],evidence:vec![]})); };
    let final_cl=compute_closure(roots,&chosen,&cmap,target);
    let selected_vec=chosen.iter().map(|(package,version)|Selection{package:package.clone(),version:*version}).collect::<Vec<_>>();
    let mut all=BTreeSet::new();
    for p in lockmap.keys(){all.insert(p.clone());}
    for p in chosen.keys(){all.insert(p.clone());}
    let mut changes=Vec::new();
    for p in all {
        let before=lockmap.get(&p).copied();
        let after=chosen.get(&p).copied();
        if before==after {continue;}
        let reason=match (before,after) {
            (None,Some(_))=>ChangeReason::Added,
            (Some(_),None)=>ChangeReason::Removed,
            (Some(b),Some(_))=>{
                let exists=cmap.get(&p).is_some_and(|pkg|pkg.releases.iter().any(|r|r.version==b));
                if !exists {ChangeReason::Unavailable}
                else if final_cl.constraints.get(&p).is_some_and(|cs|cs.iter().any(|c|!permits(*c,b))) {ChangeReason::Rejected}
                else {ChangeReason::Rebalanced}
            }
            _=>unreachable!(),
        };
        changes.push(Change{package:p,before,after,reason});
    }
    Ok(Resolution{selected:selected_vec,changes})
}