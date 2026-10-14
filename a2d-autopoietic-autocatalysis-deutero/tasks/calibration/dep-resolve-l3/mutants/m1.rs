use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Feature { pub name: String, pub dependencies: Vec<String> }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    pub dependencies: Vec<String>,
    pub optional_dependencies: Vec<String>,
    pub features: Vec<Feature>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request { pub package: String, pub features: Vec<String> }
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolveError {
    MissingPackage(String),
    UnknownFeature { package: String, feature: String },
    Cycle(Vec<String>),
}
pub fn resolve(packages: &[Package], requests: &[Request]) -> Result<Vec<String>, ResolveError> {
    let catalog: BTreeMap<String, &Package> = packages.iter().map(|p| (p.name.clone(), p)).collect();
    let mut enabled: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for r in requests { enabled.insert(r.package.clone(), r.features.iter().cloned().collect()); }
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
        let mut deps: BTreeSet<String> = p.dependencies.iter().cloned().collect();
        deps.extend(p.optional_dependencies.iter().filter(|d| active.contains(*d)).cloned());
        pending.extend(deps.iter().cloned());
        graph.insert(name, deps);
    }
    if let Some(name) = missing.into_iter().next() { return Err(ResolveError::MissingPackage(name)); }
    if let Some((package, feature)) = unknown.into_iter().next() {
        return Err(ResolveError::UnknownFeature { package, feature });
    }
    if let Some(c) = cycle(&graph) { return Err(ResolveError::Cycle(c)); }
    Ok(topo(&graph))
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
