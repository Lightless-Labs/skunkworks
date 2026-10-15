#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Constraint {
    Exact(Version),
    AtLeast(Version),
    Before(Version),
    Caret(Version),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub package: String,
    pub version: Version,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    pub releases: Vec<Release>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Requirement {
    pub package: String,
    pub constraint: Constraint,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    pub deps: Vec<Requirement>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolveError {
    InvalidName {
        site: String,
        name: String,
    },
    DuplicatePackage {
        package: String,
    },
    DuplicateVersion {
        package: String,
        version: Version,
    },
    Missing {
        package: String,
    },
    NoMatch {
        package: String,
        requirements: Vec<Constraint>,
    },
    Conflict {
        package: String,
        selected: Version,
        requirements: Vec<Constraint>,
    },
}

fn valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.is_empty() || bytes.len() > 24 {
        return false;
    }
    if !bytes[0].is_ascii_lowercase() || bytes[bytes.len() - 1] == b'-' {
        return false;
    }
    bytes
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

fn canonical(mut constraints: Vec<Constraint>) -> Vec<Constraint> {
    constraints.sort();
    constraints.dedup();
    constraints
}

fn permits(constraint: Constraint, version: Version) -> bool {
    match constraint {
        Constraint::Exact(x) => version == x,
        Constraint::AtLeast(x) => version >= x,
        Constraint::Before(x) => version < x,
        Constraint::Caret(x) => {
            version >= x
                && if x.major > 0 {
                    version.major == x.major
                } else if x.minor > 0 {
                    version.major == 0 && version.minor == x.minor
                } else {
                    version == x
                }
        }
    }
}

fn constraints_match(constraints: &[Constraint], version: Version) -> bool {
    constraints.iter().all(|c| permits(*c, version))
}

fn version_text(version: Version) -> String {
    format!("{}.{}.{}", version.major, version.minor, version.patch)
}

fn constraint_text(constraint: Constraint) -> String {
    match constraint {
        Constraint::Exact(v) => format!("={}", version_text(v)),
        Constraint::AtLeast(v) => format!(">={}", version_text(v)),
        Constraint::Before(v) => format!("<{}", version_text(v)),
        Constraint::Caret(v) => format!("^{}", version_text(v)),
    }
}

fn constraints_text(constraints: &[Constraint]) -> String {
    canonical(constraints.to_vec())
        .iter()
        .map(|c| constraint_text(*c))
        .collect::<Vec<_>>()
        .join(",")
}

fn encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'.' | b'_' | b'-') {
            out.push(*byte as char);
        } else {
            out.push('%');
            out.push_str(&format!("{:02X}", byte));
        }
    }
    out
}

pub fn error_text(error: &ResolveError) -> String {
    match error {
        ResolveError::InvalidName { site, name } => {
            format!("invalid-name|site={}|name={}", encode(site), encode(name))
        }
        ResolveError::DuplicatePackage { package } => {
            format!("duplicate-package|package={}", encode(package))
        }
        ResolveError::DuplicateVersion { package, version } => format!(
            "duplicate-version|package={}|version={}",
            encode(package),
            version_text(*version)
        ),
        ResolveError::Missing { package } => {
            format!("missing|package={}", encode(package))
        }
        ResolveError::NoMatch {
            package,
            requirements,
        } => format!(
            "no-match|package={}|constraints={}",
            encode(package),
            constraints_text(requirements)
        ),
        ResolveError::Conflict {
            package,
            selected,
            requirements,
        } => format!(
            "conflict|package={}|selected={}|constraints={}",
            encode(package),
            version_text(*selected),
            constraints_text(requirements)
        ),
    }
}

fn validate_names(catalog: &[Package], roots: &[Requirement]) -> Result<(), ResolveError> {
    for (i, root) in roots.iter().enumerate() {
        if !valid_name(&root.package) {
            return Err(ResolveError::InvalidName {
                site: format!("root[{}]", i),
                name: root.package.clone(),
            });
        }
    }

    for (i, package) in catalog.iter().enumerate() {
        if !valid_name(&package.name) {
            return Err(ResolveError::InvalidName {
                site: format!("catalog[{}]", i),
                name: package.name.clone(),
            });
        }
        for (j, release) in package.releases.iter().enumerate() {
            for (k, dep) in release.deps.iter().enumerate() {
                if !valid_name(&dep.package) {
                    return Err(ResolveError::InvalidName {
                        site: format!("catalog[{}].release[{}].dep[{}]", i, j, k),
                        name: dep.package.clone(),
                    });
                }
            }
        }
    }
    Ok(())
}

fn canonical_active(
    roots: &[Requirement],
    chosen: &[(usize, usize)],
    catalog: &[Package],
) -> std::collections::BTreeMap<String, Vec<Constraint>> {
    let mut active = std::collections::BTreeMap::<String, Vec<Constraint>>::new();

    for root in roots {
        active
            .entry(root.package.clone())
            .or_default()
            .push(root.constraint);
    }
    for &(package_index, release_index) in chosen {
        let package = &catalog[package_index];
        let release = &package.releases[release_index];
        for dep in &release.deps {
            active
                .entry(dep.package.clone())
                .or_default()
                .push(dep.constraint);
        }
    }
    for constraints in active.values_mut() {
        *constraints = canonical(std::mem::take(constraints));
    }
    active
}

fn search(
    catalog: &[Package],
    package_indices: &std::collections::BTreeMap<String, usize>,
    roots: &[Requirement],
    chosen: &mut std::collections::BTreeMap<String, (usize, usize)>,
) -> Result<(), ResolveError> {
    let chosen_list: Vec<(usize, usize)> = chosen.values().copied().collect();
    let active = canonical_active(roots, &chosen_list, catalog);

    if let Some(name) = active
        .keys()
        .filter(|name| !package_indices.contains_key(*name))
        .next()
    {
        return Err(ResolveError::Missing {
            package: name.clone(),
        });
    }

    for (name, constraints) in &active {
        if let Some(&(package_index, release_index)) = chosen.get(name) {
            let version = catalog[package_index].releases[release_index].version;
            if !constraints_match(constraints, version) {
                return Err(ResolveError::Conflict {
                    package: name.clone(),
                    selected: version,
                    requirements: constraints.clone(),
                });
            }
        }
    }

    for (name, constraints) in &active {
        if chosen.contains_key(name) {
            continue;
        }
        let package_index = package_indices[name];
        let package = &catalog[package_index];
        if !package
            .releases
            .iter()
            .any(|release| constraints_match(constraints, release.version))
        {
            return Err(ResolveError::NoMatch {
                package: name.clone(),
                requirements: constraints.clone(),
            });
        }
    }

    let next = active.keys().find(|name| !chosen.contains_key(*name));
    let Some(name) = next else {
        return Ok(());
    };

    let package_index = package_indices[name];
    let constraints = &active[name];
    let release_index = catalog[package_index]
        .releases
        .iter()
        .enumerate()
        .filter(|(_, release)| constraints_match(constraints, release.version))
        .max_by_key(|(_, release)| release.version)
        .map(|(i, _)| i)
        .expect("candidate existence checked");

    chosen.insert(name.clone(), (package_index, release_index));
    search(catalog, package_indices, roots, chosen)
}

pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
) -> Result<Vec<Selection>, ResolveError> {
    validate_names(catalog, roots)?;

    let mut package_indices = std::collections::BTreeMap::<String, usize>::new();
    for (i, package) in catalog.iter().enumerate() {
        if package_indices.contains_key(&package.name) {
            return Err(ResolveError::DuplicatePackage {
                package: package.name.clone(),
            });
        }
        package_indices.insert(package.name.clone(), i);
    }

    for package in catalog {
        let mut seen = std::collections::BTreeSet::new();
        for release in &package.releases {
            if !seen.insert(release.version) {
                return Err(ResolveError::DuplicateVersion {
                    package: package.name.clone(),
                    version: release.version,
                });
            }
        }
    }

    let mut chosen = std::collections::BTreeMap::<String, (usize, usize)>::new();
    search(catalog, &package_indices, roots, &mut chosen)?;

    Ok(chosen
        .into_iter()
        .map(|(package, (package_index, release_index))| Selection {
            package,
            version: catalog[package_index].releases[release_index].version,
        })
        .collect())
}

fn main() {}