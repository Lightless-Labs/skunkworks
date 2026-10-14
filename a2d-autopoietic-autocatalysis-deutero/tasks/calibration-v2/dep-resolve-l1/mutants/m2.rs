
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version { pub major: u32, pub minor: u32, pub patch: u32 }
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Constraint { Exact(Version), AtLeast(Version), Before(Version), Caret(Version) }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection { pub package: String, pub version: Version }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package { pub name: String, pub releases: Vec<Release> }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Requirement { pub package: String, pub constraint: Constraint }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release { pub version: Version, pub deps: Vec<Requirement> }
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolveError {
    InvalidName { site: String, name: String },
    DuplicatePackage { package: String },
    DuplicateVersion { package: String, version: Version },
    Missing { package: String },
    NoMatch { package: String, requirements: Vec<Constraint> },
    Conflict { package: String, selected: Version, requirements: Vec<Constraint> },
}

fn valid(s: &str) -> bool {
    let b=s.as_bytes(); !b.is_empty() && b.len()<=24 && b[0].is_ascii_lowercase()
    && b.iter().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c==b'-')
    && b.last()!=Some(&b'-')
}
fn enc(s: &str) -> String {
    let mut out=String::new();
    for b in s.bytes() { if b.is_ascii_alphanumeric() || b"._-".contains(&b) { out.push(b as char); }
        else { out.push_str(&format!("%{:02X}",b)); } }
    out
}
fn ver(v: Version) -> String { format!("{}.{}.{}",v.major,v.minor,v.patch) }
fn ct(c: Constraint) -> String { match c {
    Constraint::Exact(v)=>format!("={}",ver(v)), Constraint::AtLeast(v)=>format!(">={}",ver(v)),
    Constraint::Before(v)=>format!("<{}",ver(v)), Constraint::Caret(v)=>format!("^{}",ver(v)) } }
fn canon(cs: &[Constraint]) -> Vec<Constraint> { cs.iter().copied().collect::<BTreeSet<_>>().into_iter().collect() }
fn cs_text(cs: &[Constraint]) -> String { canon(cs).into_iter().map(ct).collect::<Vec<_>>().join(",") }
fn fits(v: Version, c: Constraint) -> bool { match c {
    Constraint::Exact(x)=>v==x, Constraint::AtLeast(x)=>v>=x, Constraint::Before(x)=>v<x,
    Constraint::Caret(x)=> v>=x && if x.major>0 {v.major==x.major}
        else if x.minor>0 {v.major==0 && v.minor==x.minor}
        else {v.major==0 && v.minor==0 && v.patch==x.patch}
} }
fn name(s: &str, site: String) -> Result<(), ResolveError> {
    if valid(s) {Ok(())} else {Err(ResolveError::InvalidName {site,name:s.into()})}
}
fn validate(catalog: &[Package], roots: &[Requirement]) -> Result<(), ResolveError> {
    for (i,r) in roots.iter().enumerate() {
        name(&r.package,format!("root[{}]",i))?;
        
    }
    for (i,p) in catalog.iter().enumerate() {
        name(&p.name,format!("catalog[{}]",i))?;
        for (j,r) in p.releases.iter().enumerate() {
            for (k,d) in r.deps.iter().enumerate() {
                name(&d.package,format!("catalog[{}].release[{}].dep[{}]",i,j,k))?;
                
            }
            
        }
    }
    let mut packages=BTreeSet::new();
    for p in catalog {
        if !packages.insert(&p.name) {return Err(ResolveError::DuplicatePackage{package:p.name.clone()});}
    }
    for p in catalog {
        let mut versions=BTreeSet::new();
        for r in &p.releases {
            if !versions.insert(r.version) {return Err(ResolveError::DuplicateVersion{package:p.name.clone(),version:r.version});}
        }
    }
    
    Ok(())
}
pub fn error_text(error:&ResolveError)->String {
    match error {
        ResolveError::InvalidName{site,name}=>format!("invalid-name|site={}|name={}",enc(site),enc(name)),
        ResolveError::DuplicatePackage{package}=>format!("duplicate-package|package={}",enc(package)),
        ResolveError::DuplicateVersion{package,version}=>format!("duplicate-version|package={}|version={}",enc(package),ver(*version)),
        ResolveError::Missing{package}=>format!("missing|package={}",enc(package)),
        ResolveError::NoMatch{package,requirements}=>format!("no-match|package={}|constraints={}",enc(package),cs_text(requirements)),
        ResolveError::Conflict{package,selected,requirements}=>format!("conflict|package={}|selected={}|constraints={}",enc(package),ver(*selected),cs_text(requirements)),
    }
}

#[derive(Clone)]
struct Active { constraints: BTreeMap<String,Vec<Constraint>> }
fn active(roots:&[Requirement], selected:&BTreeMap<String,Release>) -> Active {
    let mut constraints: BTreeMap<String,Vec<Constraint>>=BTreeMap::new();
    for r in roots {constraints.entry(r.package.clone()).or_default().push(r.constraint);}
    for r in selected.values() {for d in &r.deps {constraints.entry(d.package.clone()).or_default().push(d.constraint);}}
    for cs in constraints.values_mut() {*cs=canon(cs);}
    Active{constraints}
}

fn inspect(catalog:&BTreeMap<String,Package>,selected:&BTreeMap<String,Release>,a:&Active) -> Result<Option<(String,Vec<Release>)>,ResolveError> {

    
    for (p,cs) in &a.constraints {
        if let Some(r)=selected.get(p) {
            if !cs.iter().all(|c|fits(r.version,*c)) {
                return Err(ResolveError::Conflict{package:p.clone(),selected:r.version,requirements:cs.clone() });
            }
        }
    }

    for p in a.constraints.keys() {
        if !catalog.contains_key(p) {return Err(ResolveError::Missing{package:p.clone() });}
    }
    let mut choices=BTreeMap::new();
    for (p,cs) in &a.constraints {
        if !selected.contains_key(p) {
            let mut rs:Vec<Release>=catalog[p].releases.iter().filter(|r|cs.iter().all(|c|fits(r.version,*c))).cloned().collect();
            rs.sort_by(|a,b|b.version.cmp(&a.version));
            if rs.is_empty() {return Err(ResolveError::NoMatch{package:p.clone(),requirements:cs.clone() });}
            choices.insert(p.clone(),rs);
        }
    }
    Ok(choices.into_iter().next())
}
fn selections(s:&BTreeMap<String,Release>)->Vec<Selection> {s.iter().map(|(p,r)|Selection{package:p.clone(),version:r.version}).collect()}
pub fn resolve(catalog:&[Package],roots:&[Requirement])->Result<Vec<Selection>,ResolveError> {
    validate(catalog,roots)?;
    let catalog=catalog.iter().map(|p|(p.name.clone(),p.clone())).collect();
    let mut selected=BTreeMap::new();
    loop {
        let a=active(roots,&selected);
        match inspect(&catalog,&selected,&a)? {
            Some((p,rs))=>{selected.insert(p,rs[0].clone());}, None=>return Ok(selections(&selected))
        }
    }
}
