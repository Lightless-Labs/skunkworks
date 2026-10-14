use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package { pub name: String, pub dependencies: Vec<String> }
pub fn install_order(packages: &[Package]) -> Vec<String> {
    let graph: BTreeMap<String, BTreeSet<String>> = packages.iter().map(|p| (p.name.clone(), p.dependencies.iter().cloned().collect())).collect();
    topo(&graph)
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
