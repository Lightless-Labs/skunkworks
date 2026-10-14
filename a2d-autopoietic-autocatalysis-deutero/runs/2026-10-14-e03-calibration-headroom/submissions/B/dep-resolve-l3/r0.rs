use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Feature {
    pub name: String,
    pub dependencies: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    pub dependencies: Vec<String>,
    pub optional_dependencies: Vec<String>,
    pub features: Vec<Feature>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub package: String,
    pub features: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolveError {
    MissingPackage(String),
    UnknownFeature { package: String, feature: String },
    Cycle(Vec<String>),
}

pub fn resolve(packages: &[Package], requests: &[Request]) -> Result<Vec<String>, ResolveError> {
    let catalog: BTreeMap<&str, &Package> =
        packages.iter().map(|package| (package.name.as_str(), package)).collect();

    let mut requested_features: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for request in requests {
        requested_features
            .entry(request.package.clone())
            .or_default()
            .extend(request.features.iter().cloned());
    }

    if requests.is_empty() {
        return Ok(Vec::new());
    }

    let mut relevant = BTreeSet::<String>::new();
    let mut edges = BTreeMap::<String, BTreeSet<String>>::new();
    let mut missing = BTreeSet::<String>::new();
    let mut pending: BTreeSet<String> = requested_features.keys().cloned().collect();

    while let Some(name) = pending.pop_first() {
        if relevant.contains(&name) {
            continue;
        }

        let Some(package) = catalog.get(name.as_str()).copied() else {
            missing.insert(name);
            continue;
        };

        relevant.insert(name.clone());

        let enabled = requested_features.get(&name);
        let enabled_targets: BTreeSet<&str> = enabled
            .into_iter()
            .flat_map(|features| features.iter())
            .filter_map(|feature_name| {
                package
                    .features
                    .iter()
                    .find(|feature| feature.name == *feature_name)
            })
            .flat_map(|feature| feature.dependencies.iter().map(String::as_str))
            .collect();

        let mut targets: BTreeSet<String> = package.dependencies.iter().cloned().collect();
        targets.extend(
            package
                .optional_dependencies
                .iter()
                .filter(|target| enabled_targets.contains(target.as_str()))
                .cloned(),
        );

        for target in &targets {
            if !catalog.contains_key(target.as_str()) {
                missing.insert(target.clone());
            } else if !relevant.contains(target) {
                pending.insert(target.clone());
            }
        }

        edges.insert(name, targets);
    }

    if let Some(name) = missing.into_iter().next() {
        return Err(ResolveError::MissingPackage(name));
    }

    let mut unknown_features = BTreeSet::<(String, String)>::new();
    for (package_name, features) in &requested_features {
        if let Some(package) = catalog.get(package_name.as_str()) {
            let known: BTreeSet<&str> = package.features.iter().map(|f| f.name.as_str()).collect();
            for feature in features {
                if !known.contains(feature.as_str()) {
                    unknown_features.insert((package_name.clone(), feature.clone()));
                }
            }
        }
    }

    if let Some((package, feature)) = unknown_features.into_iter().next() {
        return Err(ResolveError::UnknownFeature { package, feature });
    }

    let mut state = BTreeMap::<String, u8>::new();
    let mut stack = Vec::<String>::new();
    let mut cycle = None;

    fn visit(
        node: &str,
        edges: &BTreeMap<String, BTreeSet<String>>,
        state: &mut BTreeMap<String, u8>,
        stack: &mut Vec<String>,
        cycle: &mut Option<Vec<String>>,
    ) {
        if cycle.is_some() {
            return;
        }

        state.insert(node.to_string(), 1);
        stack.push(node.to_string());

        if let Some(targets) = edges.get(node) {
            for target in targets {
                match state.get(target).copied().unwrap_or(0) {
                    0 => visit(target, edges, state, stack, cycle),
                    1 => {
                        if let Some(index) = stack.iter().position(|item| item == target) {
                            let mut path = stack[index..].to_vec();
                            let smallest = path
                                .iter()
                                .enumerate()
                                .min_by(|(_, a), (_, b)| a.cmp(b))
                                .map(|(index, _)| index)
                                .unwrap_or(0);
                            path.rotate_left(smallest);
                            path.push(path[0].clone());
                            *cycle = Some(path);
                        }
                    }
                    _ => {}
                }

                if cycle.is_some() {
                    return;
                }
            }
        }

        stack.pop();
        state.insert(node.to_string(), 2);
    }

    for node in &relevant {
        if state.get(node).copied().unwrap_or(0) == 0 {
            visit(node, &edges, &mut state, &mut stack, &mut cycle);
            if cycle.is_some() {
                break;
            }
        }
    }

    if let Some(path) = cycle {
        return Err(ResolveError::Cycle(path));
    }

    let mut remaining_dependencies: BTreeMap<String, usize> = BTreeMap::new();
    let mut dependents: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for node in &relevant {
        let targets = edges.get(node).cloned().unwrap_or_default();
        remaining_dependencies.insert(node.clone(), targets.len());
        for target in targets {
            dependents.entry(target).or_default().insert(node.clone());
        }
    }

    let mut ready: BTreeSet<String> = remaining_dependencies
        .iter()
        .filter(|(_, count)| **count == 0)
        .map(|(name, _)| name.clone())
        .collect();

    let mut order = Vec::with_capacity(relevant.len());
    while let Some(node) = ready.pop_first() {
        order.push(node.clone());

        if let Some(parents) = dependents.get(&node) {
            for parent in parents {
                if let Some(count) = remaining_dependencies.get_mut(parent) {
                    *count -= 1;
                    if *count == 0 {
                        ready.insert(parent.clone());
                    }
                }
            }
        }
    }

    Ok(order)
}