use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package { pub name: String, pub dependencies: Vec<String> }
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolveError {
    MissingDependency { package: String, dependency: String },
    Cycle(Vec<String>),
}
pub fn install_order(packages: &[Package]) -> Result<Vec<String>, ResolveError> {
    let graph: BTreeMap<String, BTreeSet<String>> = packages.iter()
        .map(|p| (p.name.clone(), p.dependencies.iter().cloned().collect())).collect();
    for (name, deps) in &graph {
        for target in deps {
            if !graph.contains_key(target) {
                return Err(ResolveError::MissingDependency { package: name.clone(), dependency: target.clone() });
            }
        }
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
