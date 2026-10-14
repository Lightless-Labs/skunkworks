#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    pub dependencies: Vec<String>,
}

pub fn install_order(packages: &[Package]) -> Vec<String> {
    use std::collections::{BTreeSet, HashMap};

    let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut remaining_dependencies: HashMap<&str, usize> = HashMap::new();

    for package in packages {
        let unique_dependencies: BTreeSet<&str> =
            package.dependencies.iter().map(String::as_str).collect();

        remaining_dependencies.insert(package.name.as_str(), unique_dependencies.len());
        for dependency in unique_dependencies {
            dependents
                .entry(dependency)
                .or_default()
                .push(package.name.as_str());
        }
    }

    let mut ready: BTreeSet<&str> = remaining_dependencies
        .iter()
        .filter_map(|(&name, &count)| (count == 0).then_some(name))
        .collect();

    let mut order = Vec::with_capacity(packages.len());

    while let Some(&name) = ready.iter().next() {
        ready.remove(name);
        order.push(name.to_string());

        if let Some(children) = dependents.get(name) {
            for &child in children {
                let count = remaining_dependencies
                    .get_mut(child)
                    .expect("dependent package must exist");
                *count -= 1;
                if *count == 0 {
                    ready.insert(child);
                }
            }
        }
    }

    order
}