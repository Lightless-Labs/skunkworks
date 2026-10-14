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

pub fn resolve(
    packages: &[Package],
    requests: &[Request],
) -> Result<Vec<String>, ResolveError> {
    let catalog: BTreeMap<&str, &Package> = packages
        .iter()
        .map(|package| (package.name.as_str(), package))
        .collect();

    let mut requested_features: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for request in requests {
        requested_features
            .entry(request.package.clone())
            .or_default()
            .extend(request.features.iter().cloned());
    }

    if requested_features.is_empty() {
        return Ok(Vec::new());
    }

    let mut relevant = BTreeSet::new();
    let mut missing = BTreeSet::new();
    let mut pending: Vec<String> = requested_features.keys().cloned().collect();

    while let Some(name) = pending.pop() {
        if !relevant.insert(name.clone()) {
            continue;
        }

        let Some(package) = catalog.get(name.as_str()).copied() else {
            missing.insert(name);
            continue;
        };

        let enabled = requested_features.get(&name);
        let mut active_optional = BTreeSet::new();

        if let Some(enabled) = enabled {
            let feature_map: BTreeMap<&str, &Feature> = package
                .features
                .iter()
                .map(|feature| (feature.name.as_str(), feature))
                .collect();

            for feature_name in enabled {
                if let Some(feature) = feature_map.get(feature_name.as_str()) {
                    active_optional.extend(feature.dependencies.iter().cloned());
                }
            }
        }

        let mut targets: BTreeSet<String> =
            package.dependencies.iter().cloned().collect();
        targets.extend(
            package
                .optional_dependencies
                .iter()
                .filter(|target| active_optional.contains(*target))
                .cloned(),
        );

        pending.extend(targets);
    }

    if let Some(name) = missing.into_iter().next() {
        return Err(ResolveError::MissingPackage(name));
    }

    let mut unknown = BTreeSet::new();
    for (package_name, enabled) in &requested_features {
        if let Some(package) = catalog.get(package_name.as_str()).copied() {
            let known: BTreeSet<&str> = package
                .features
                .iter()
                .map(|feature| feature.name.as_str())
                .collect();

            for feature in enabled {
                if !known.contains(feature.as_str()) {
                    unknown.insert((package_name.clone(), feature.clone()));
                }
            }
        }
    }

    if let Some((package, feature)) = unknown.into_iter().next() {
        return Err(ResolveError::UnknownFeature { package, feature });
    }

    let mut graph: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for name in &relevant {
        let package = catalog[name.as_str()];
        let enabled = requested_features.get(name);
        let mut active_optional = BTreeSet::new();

        if let Some(enabled) = enabled {
            for feature in &package.features {
                if enabled.contains(&feature.name) {
                    active_optional.extend(feature.dependencies.iter().cloned());
                }
            }
        }

        let mut targets: BTreeSet<String> =
            package.dependencies.iter().cloned().collect();
        targets.extend(
            package
                .optional_dependencies
                .iter()
                .filter(|target| active_optional.contains(*target))
                .cloned(),
        );
        graph.insert(name.clone(), targets);
    }

    fn find_cycle(
        node: &str,
        graph: &BTreeMap<String, BTreeSet<String>>,
        visited: &mut BTreeSet<String>,
        stack: &mut Vec<String>,
        on_stack: &mut BTreeMap<String, usize>,
    ) -> Option<Vec<String>> {
        visited.insert(node.to_string());
        on_stack.insert(node.to_string(), stack.len());
        stack.push(node.to_string());

        if let Some(targets) = graph.get(node) {
            for target in targets {
                if let Some(&start) = on_stack.get(target) {
                    let mut cycle = stack[start..].to_vec();
                    let smallest = cycle
                        .iter()
                        .enumerate()
                        .min_by(|(_, a), (_, b)| a.cmp(b))
                        .map(|(index, _)| index)
                        .unwrap();
                    cycle.rotate_left(smallest);
                    cycle.push(cycle[0].clone());
                    return Some(cycle);
                }

                if !visited.contains(target) {
                    if let Some(cycle) =
                        find_cycle(target, graph, visited, stack, on_stack)
                    {
                        return Some(cycle);
                    }
                }
            }
        }

        stack.pop();
        on_stack.remove(node);
        None
    }

    let mut visited = BTreeSet::new();
    let mut stack = Vec::new();
    let mut on_stack = BTreeMap::new();
    for name in &relevant {
        if !visited.contains(name) {
            if let Some(cycle) =
                find_cycle(name, &graph, &mut visited, &mut stack, &mut on_stack)
            {
                return Err(ResolveError::Cycle(cycle));
            }
        }
    }

    let mut remaining_dependencies: BTreeMap<String, usize> = BTreeMap::new();
    let mut dependents: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for name in &relevant {
        let dependencies = &graph[name];
        remaining_dependencies.insert(name.clone(), dependencies.len());
        for dependency in dependencies {
            dependents
                .entry(dependency.clone())
                .or_default()
                .insert(name.clone());
        }
    }

    let mut ready: BTreeSet<String> = remaining_dependencies
        .iter()
        .filter_map(|(name, &count)| (count == 0).then_some(name.clone()))
        .collect();
    let mut order = Vec::with_capacity(relevant.len());

    while let Some(name) = ready.iter().next().cloned() {
        ready.remove(&name);
        order.push(name.clone());

        if let Some(users) = dependents.get(&name) {
            for user in users {
                let count = remaining_dependencies.get_mut(user).unwrap();
                *count -= 1;
                if *count == 0 {
                    ready.insert(user.clone());
                }
            }
        }
    }

    Ok(order)
}