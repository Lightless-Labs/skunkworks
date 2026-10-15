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
    Limit {
        attempts: usize,
    },
}

fn valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.is_empty()
        || bytes.len() > 24
        || !bytes[0].is_ascii_lowercase()
        || bytes[bytes.len() - 1] == b'-'
    {
        return false;
    }
    bytes
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
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

fn canonicalize(mut constraints: Vec<Constraint>) -> Vec<Constraint> {
    constraints.sort();
    constraints.dedup();
    constraints
}

fn constraint_list_text(constraints: &[Constraint]) -> String {
    canonicalize(constraints.to_vec())
        .iter()
        .map(|c| constraint_text(*c))
        .collect::<Vec<_>>()
        .join(",")
}

fn encode_text(value: &str) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'.' | b'_' | b'-') {
            out.push(*byte as char);
        } else {
            out.push_str(&format!("%{:02X}", byte));
        }
    }
    out
}

pub fn error_text(error: &ResolveError) -> String {
    match error {
        ResolveError::InvalidName { site, name } => {
            format!("invalid-name|site={}|name={}", encode_text(site), encode_text(name))
        }
        ResolveError::DuplicatePackage { package } => {
            format!("duplicate-package|package={}", encode_text(package))
        }
        ResolveError::DuplicateVersion { package, version } => format!(
            "duplicate-version|package={}|version={}",
            encode_text(package),
            version_text(*version)
        ),
        ResolveError::Missing { package } => {
            format!("missing|package={}", encode_text(package))
        }
        ResolveError::NoMatch {
            package,
            requirements,
        } => format!(
            "no-match|package={}|constraints={}",
            encode_text(package),
            constraint_list_text(requirements)
        ),
        ResolveError::Conflict {
            package,
            selected,
            requirements,
        } => format!(
            "conflict|package={}|selected={}|constraints={}",
            encode_text(package),
            version_text(*selected),
            constraint_list_text(requirements)
        ),
        ResolveError::Limit { attempts } => format!("limit|attempts={attempts}"),
    }
}

fn matches_constraint(version: Version, constraint: Constraint) -> bool {
    match constraint {
        Constraint::Exact(v) => version == v,
        Constraint::AtLeast(v) => version >= v,
        Constraint::Before(v) => version < v,
        Constraint::Caret(v) => {
            version >= v
                && if v.major > 0 {
                    version.major == v.major
                } else if v.minor > 0 {
                    version.major == 0 && version.minor == v.minor
                } else {
                    version == v
                }
        }
    }
}

fn satisfies(version: Version, constraints: &[Constraint]) -> bool {
    constraints
        .iter()
        .all(|constraint| matches_constraint(version, *constraint))
}

fn validate(catalog: &[Package], roots: &[Requirement]) -> Result<(), ResolveError> {
    for (i, root) in roots.iter().enumerate() {
        if !valid_name(&root.package) {
            return Err(ResolveError::InvalidName {
                site: format!("root[{i}]"),
                name: root.package.clone(),
            });
        }
    }

    for (i, package) in catalog.iter().enumerate() {
        if !valid_name(&package.name) {
            return Err(ResolveError::InvalidName {
                site: format!("catalog[{i}]"),
                name: package.name.clone(),
            });
        }
        for (j, release) in package.releases.iter().enumerate() {
            for (k, dep) in release.deps.iter().enumerate() {
                if !valid_name(&dep.package) {
                    return Err(ResolveError::InvalidName {
                        site: format!("catalog[{i}].release[{j}].dep[{k}]"),
                        name: dep.package.clone(),
                    });
                }
            }
        }
    }

    let mut seen_packages = std::collections::BTreeSet::new();
    for package in catalog {
        if !seen_packages.insert(package.name.as_str()) {
            return Err(ResolveError::DuplicatePackage {
                package: package.name.clone(),
            });
        }
    }

    for package in catalog {
        let mut seen_versions = std::collections::BTreeSet::new();
        for release in &package.releases {
            if !seen_versions.insert(release.version) {
                return Err(ResolveError::DuplicateVersion {
                    package: package.name.clone(),
                    version: release.version,
                });
            }
        }
    }

    Ok(())
}

type Active = std::collections::BTreeMap<String, Vec<Constraint>>;
type Assigned = std::collections::BTreeMap<String, Version>;

fn active_constraints(roots: &[Requirement], assigned: &Assigned, catalog: &[Package]) -> Active {
    let mut active = Active::new();
    for root in roots {
        active
            .entry(root.package.clone())
            .or_default()
            .push(root.constraint);
    }

    for (name, version) in assigned {
        if let Some(package) = catalog.iter().find(|p| p.name == *name) {
            if let Some(release) = package.releases.iter().find(|r| r.version == *version) {
                for dep in &release.deps {
                    active
                        .entry(dep.package.clone())
                        .or_default()
                        .push(dep.constraint);
                }
            }
        }
    }

    for constraints in active.values_mut() {
        *constraints = canonicalize(std::mem::take(constraints));
    }
    active
}

fn search(
    catalog: &[Package],
    roots: &[Requirement],
    assigned: &mut Assigned,
    attempts: &mut usize,
    max_attempts: usize,
) -> Result<Vec<Selection>, ResolveError> {
    let active = active_constraints(roots, assigned, catalog);

    if let Some(package) = active
        .keys()
        .filter(|name| !catalog.iter().any(|p| p.name == ***name))
        .next()
    {
        return Err(ResolveError::Missing {
            package: package.clone(),
        });
    }

    for (package, version) in assigned.iter() {
        if let Some(constraints) = active.get(package) {
            if !satisfies(*version, constraints) {
                return Err(ResolveError::Conflict {
                    package: package.clone(),
                    selected: *version,
                    requirements: constraints.clone(),
                });
            }
        }
    }

    for (package, constraints) in &active {
        if !assigned.contains_key(package) {
            let catalog_package = catalog.iter().find(|p| p.name == *package).unwrap();
            if !catalog_package
                .releases
                .iter()
                .any(|release| satisfies(release.version, constraints))
            {
                return Err(ResolveError::NoMatch {
                    package: package.clone(),
                    requirements: constraints.clone(),
                });
            }
        }
    }

    let Some(package) = active.keys().find(|name| !assigned.contains_key(*name)) else {
        return Ok(assigned
            .iter()
            .map(|(package, version)| Selection {
                package: package.clone(),
                version: *version,
            })
            .collect());
    };

    let package_name = package.clone();
    let constraints = active.get(&package_name).unwrap();
    let catalog_package = catalog.iter().find(|p| p.name == package_name).unwrap();
    let mut candidates: Vec<Version> = catalog_package
        .releases
        .iter()
        .filter(|release| satisfies(release.version, constraints))
        .map(|release| release.version)
        .collect();
    candidates.sort_by(|a, b| b.cmp(a));

    let mut first_error = None;
    for version in candidates {
        if *attempts == max_attempts {
            return Err(ResolveError::Limit {
                attempts: max_attempts,
            });
        }
        *attempts += 1;
        assigned.insert(package_name.clone(), version);
        let result = search(catalog, roots, assigned, attempts, max_attempts);
        assigned.remove(&package_name);

        match result {
            Ok(solution) => return Ok(solution),
            Err(error @ ResolveError::Limit { .. }) => return Err(error),
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }

    Err(first_error.unwrap_or_else(|| ResolveError::NoMatch {
        package: package_name,
        requirements: canonicalize(active.get(package).cloned().unwrap_or_default()),
    }))
}

pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
    max_attempts: usize,
) -> Result<Vec<Selection>, ResolveError> {
    validate(catalog, roots)?;
    let mut assigned = Assigned::new();
    let mut attempts = 0;
    search(catalog, roots, &mut assigned, &mut attempts, max_attempts)
}

fn main() {}