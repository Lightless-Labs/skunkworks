#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    pub dependencies: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolveError {
    MissingDependency { package: String, dependency: String },
    Cycle(Vec<String>),
}

pub fn install_order(packages: &[Package]) -> Result<Vec<String>, ResolveError> {
    use std::collections::{BTreeSet, HashMap};

    let by_name: HashMap<&str, usize> = packages
        .iter()
        .enumerate()
        .map(|(index, package)| (package.name.as_str(), index))
        .collect();

    let mut missing: Option<(&str, &str)> = None;
    for package in packages {
        for dependency in &package.dependencies {
            if !by_name.contains_key(dependency.as_str()) {
                let candidate = (package.name.as_str(), dependency.as_str());
                if missing.is_none_or(|current| candidate < current) {
                    missing = Some(candidate);
                }
            }
        }
    }

    if let Some((package, dependency)) = missing {
        return Err(ResolveError::MissingDependency {
            package: package.to_string(),
            dependency: dependency.to_string(),
        });
    }

    let mut names: Vec<&str> = packages.iter().map(|package| package.name.as_str()).collect();
    names.sort_unstable();

    let mut visited = vec![false; packages.len()];
    let mut on_stack = vec![false; packages.len()];
    let mut stack = Vec::new();

    fn visit(
        node: usize,
        packages: &[Package],
        by_name: &HashMap<&str, usize>,
        visited: &mut [bool],
        on_stack: &mut [bool],
        stack: &mut Vec<usize>,
    ) -> Option<Vec<String>> {
        visited[node] = true;
        on_stack[node] = true;
        stack.push(node);

        let mut dependencies: Vec<&str> = packages[node]
            .dependencies
            .iter()
            .map(String::as_str)
            .collect();
        dependencies.sort_unstable();
        dependencies.dedup();

        for dependency in dependencies {
            let next = by_name[dependency];
            if on_stack[next] {
                let start = stack.iter().position(|&entry| entry == next).unwrap();
                let mut cycle: Vec<String> = stack[start..]
                    .iter()
                    .map(|&entry| packages[entry].name.clone())
                    .collect();

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
            if !visited[next] {
                if let Some(cycle) =
                    visit(next, packages, by_name, visited, on_stack, stack)
                {
                    return Some(cycle);
                }
            }
        }

        stack.pop();
        on_stack[node] = false;
        None
    }

    for name in names {
        let node = by_name[name];
        if !visited[node] {
            if let Some(cycle) = visit(
                node,
                packages,
                &by_name,
                &mut visited,
                &mut on_stack,
                &mut stack,
            ) {
                return Err(ResolveError::Cycle(cycle));
            }
        }
    }

    let mut remaining_dependencies = vec![BTreeSet::<String>::new(); packages.len()];
    let mut dependents = vec![Vec::<usize>::new(); packages.len()];

    for (index, package) in packages.iter().enumerate() {
        for dependency in &package.dependencies {
            remaining_dependencies[index].insert(dependency.clone());
        }
        for dependency in &remaining_dependencies[index] {
            let dependency_index = by_name[dependency.as_str()];
            dependents[dependency_index].push(index);
        }
    }

    let mut ready = BTreeSet::<String>::new();
    for (index, package) in packages.iter().enumerate() {
        if remaining_dependencies[index].is_empty() {
            ready.insert(package.name.clone());
        }
    }

    let mut order = Vec::with_capacity(packages.len());
    while let Some(name) = ready.iter().next().cloned() {
        ready.remove(&name);
        let node = by_name[name.as_str()];
        order.push(name);

        for &dependent in &dependents[node] {
            remaining_dependencies[dependent].remove(packages[node].name.as_str());
            if remaining_dependencies[dependent].is_empty() {
                ready.insert(packages[dependent].name.clone());
            }
        }
    }

    Ok(order)
}