use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version { pub major: u32, pub minor: u32, pub patch: u32 }
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Constraint { Exact(Version), AtLeast(Version), LessThan(Version), Caret(Version) }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dependency { pub package: String, pub constraint: Constraint }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Feature { pub name: String, pub dependencies: Vec<String> }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    pub versions: Vec<Version>,
    pub dependencies: Vec<Dependency>,
    pub optional_dependencies: Vec<Dependency>,
    pub features: Vec<Feature>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request { pub package: String, pub constraint: Constraint, pub features: Vec<String> }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installed { pub package: String, pub version: Version }
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolveError {
    MissingPackage(String),
    UnknownFeature { package: String, feature: String },
    Conflict(String),
    Cycle(Vec<String>),
}
pub fn resolve(packages: &[Package], requests: &[Request]) -> Result<Vec<Installed>, ResolveError> {
    let catalog: BTreeMap<String, &Package> = packages.iter().map(|p| (p.name.clone(), p)).collect();
    let mut enabled: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut constraints: BTreeMap<String, Vec<Constraint>> = BTreeMap::new();
    for r in requests {
        enabled.entry(r.package.clone()).or_default().extend(r.features.iter().cloned());
        constraints.entry(r.package.clone()).or_default().push(r.constraint.clone());
    }
    let mut pending: BTreeSet<String> = enabled.keys().cloned().collect();
    let mut seen = BTreeSet::new();
    let mut missing = BTreeSet::new();
    let mut unknown = BTreeSet::new();
    let mut graph = BTreeMap::new();
    while let Some(name) = pending.pop_first() {
        if !seen.insert(name.clone()) { continue; }
        let Some(p) = catalog.get(&name) else { missing.insert(name); continue; };
        let mut active = BTreeSet::new();
        if let Some(features) = enabled.get(&name) {
            for f in features {
                if let Some(feature) = p.features.iter().find(|v| &v.name == f) {
                    active.extend(feature.dependencies.iter().cloned());
                } else { unknown.insert((name.clone(), f.clone())); }
            }
        }
        let mut deps = BTreeSet::new();
        for d in p.dependencies.iter().chain(p.optional_dependencies.iter().filter(|d| active.contains(&d.package))) {
            deps.insert(d.package.clone());
            constraints.entry(d.package.clone()).or_default().push(d.constraint.clone());
        }
        pending.extend(deps.iter().cloned());
        graph.insert(name, deps);
    }
    if let Some(name) = missing.into_iter().next() { return Err(ResolveError::MissingPackage(name)); }
    if let Some((package, feature)) = unknown.into_iter().next() {
        return Err(ResolveError::UnknownFeature { package, feature });
    }
    let mut selected = BTreeMap::new();
    for name in graph.keys() {
        let cs = constraints.get(name).unwrap();
        let best = catalog[name].versions.iter().filter(|v| cs.iter().all(|c| satisfies(v, c))).max();
        let Some(version) = best else { return Err(ResolveError::Conflict(name.clone())); };
        selected.insert(name.clone(), version.clone());
    }
    if let Some(c) = cycle(&graph) { return Err(ResolveError::Cycle(c)); }
    Ok(topo(&graph).into_iter().map(|package| Installed { version: selected[&package].clone(), package }).collect())
}
fn satisfies(version: &Version, constraint: &Constraint) -> bool {
    match constraint {
        Constraint::Exact(base) => version == base,
        Constraint::AtLeast(base) => version >= base,
        Constraint::LessThan(base) => version < base,
        Constraint::Caret(base) => {
            version >= base && if base.major > 0 {
                version.major == base.major
            } else if base.minor > 0 {
                version.major == 0 && version.minor == base.minor
            } else {
                version.major == 0 && version.minor == 0
            }
        }
    }
}
fn cycle(graph: &BTreeMap<String, BTreeSet<String>>) -> Option<Vec<String>> {
    fn visit(name: &str, graph: &BTreeMap<String, BTreeSet<String>>,
             colors: &mut BTreeMap<String, u8>, stack: &mut Vec<String>) -> Option<Vec<String>> {
        colors.insert(name.to_string(), 1);
        stack.push(name.to_string());
        for target in &graph[name] {
            match colors.get(target).copied().unwrap_or(0) {
                0 => if let Some(c) = visit(target, graph, colors, stack) { return Some(c); },
                1 => {
                    let start = stack.iter().position(|n| n == target).unwrap();
                    let mut path = stack[start..].to_vec();
                    let smallest = (0..path.len()).min_by_key(|&i| &path[i]).unwrap();
                    path.rotate_left(smallest);
                    path.push(path[0].clone());
                    return Some(path);
                }
                _ => {}
            }
        }
        stack.pop();
        colors.insert(name.to_string(), 2);
        None
    }
    let mut colors = BTreeMap::new();
    let mut stack = Vec::new();
    for name in graph.keys() {
        if colors.get(name).copied().unwrap_or(0) == 0 {
            if let Some(c) = visit(name, graph, &mut colors, &mut stack) { return Some(c); }
        }
    }
    None
}
fn topo(graph: &BTreeMap<String, BTreeSet<String>>) -> Vec<String> {
    let mut done = BTreeSet::new();
    let mut result = Vec::new();
    while result.len() < graph.len() {
        let next = graph.iter()
            .find(|(name, deps)| !done.contains(*name) && deps.iter().all(|d| done.contains(d)))
            .map(|(name, _)| name.clone()).unwrap();
        done.insert(next.clone());
        result.push(next);
    }
    result
}
