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

fn version_text(v: Version) -> String {
    format!("{}.{}.{}", v.major, v.minor, v.patch)
}

fn constraint_text(c: Constraint) -> String {
    match c {
        Constraint::Exact(v) => format!("={}", version_text(v)),
        Constraint::AtLeast(v) => format!(">={}", version_text(v)),
        Constraint::Before(v) => format!("<{}", version_text(v)),
        Constraint::Caret(v) => format!("^{}", version_text(v)),
    }
}

fn canonical(mut constraints: Vec<Constraint>) -> Vec<Constraint> {
    constraints.sort();
    constraints.dedup();
    constraints
}

fn constraints_text(constraints: &[Constraint]) -> String {
    canonical(constraints.to_vec())
        .iter()
        .map(|c| constraint_text(*c))
        .collect::<Vec<_>>()
        .join(",")
}

fn encode_text(s: &str) -> String {
    let mut out = String::new();
    for byte in s.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'.' | b'_' | b'-') {
            out.push(*byte as char);
        } else {
            use std::fmt::Write;
            let _ = write!(out, "%{:02X}", byte);
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
            constraints_text(requirements)
        ),
        ResolveError::Conflict {
            package,
            selected,
            requirements,
        } => format!(
            "conflict|package={}|selected={}|constraints={}",
            encode_text(package),
            version_text(*selected),
            constraints_text(requirements)
        ),
        ResolveError::Limit { attempts } => format!("limit|attempts={}", attempts),
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

fn search(
    catalog: &[Package],
    roots: &[Requirement],
    assigned: &mut Vec<(usize, Version)>,
    attempts: &mut usize,
    max_attempts: usize,
) -> Result<Vec<Selection>, ResolveError> {
    let mut active: std::collections::BTreeMap<String, Vec<Constraint>> =
        std::collections::BTreeMap::new();

    for root in roots {
        active
            .entry(root.package.clone())
            .or_default()
            .push(root.constraint);
    }

    for (package_index, version) in assigned.iter().copied() {
        if let Some(release) = catalog[package_index]
            .releases
            .iter()
            .find(|release| release.version == version)
        {
            for dep in &release.deps {
                active
                    .entry(dep.package.clone())
                    .or_default()
                    .push(dep.constraint);
            }
        }
    }

    for constraints in active.values_mut() {
        *constraints = canonical(std::mem::take(constraints));
    }

    let package_indices: std::collections::BTreeMap<&str, usize> = catalog
        .iter()
        .enumerate()
        .map(|(i, package)| (package.name.as_str(), i))
        .collect();

    for name in active.keys() {
        if !package_indices.contains_key(name.as_str()) {
            return Err(ResolveError::Missing {
                package: name.clone(),
            });
        }
    }

    for name in active.keys() {
        let index = package_indices[name.as_str()];
        if let Some((_, version)) = assigned.iter().find(|(i, _)| *i == index) {
            let constraints = &active[name];
            if !satisfies(*version, constraints) {
                return Err(ResolveError::Conflict {
                    package: name.clone(),
                    selected: *version,
                    requirements: constraints.clone(),
                });
            }
        }
    }

    for name in active.keys() {
        let index = package_indices[name.as_str()];
        if assigned.iter().any(|(i, _)| *i == index) {
            continue;
        }
        let constraints = &active[name];
        if !catalog[index]
            .releases
            .iter()
            .any(|release| satisfies(release.version, constraints))
        {
            return Err(ResolveError::NoMatch {
                package: name.clone(),
                requirements: constraints.clone(),
            });
        }
    }

    let next = active.keys().find_map(|name| {
        let index = package_indices[name.as_str()];
        if assigned.iter().any(|(i, _)| *i == index) {
            None
        } else {
            Some((name.clone(), index))
        }
    });

    let Some((_, index)) = next else {
        let mut selections = assigned
            .iter()
            .map(|(i, version)| Selection {
                package: catalog[*i].name.clone(),
                version: *version,
            })
            .collect::<Vec<_>>();
        selections.sort_by(|a, b| a.package.cmp(&b.package));
        return Ok(selections);
    };

    let constraints = &active[&catalog[index].name];
    let mut candidates = catalog[index]
        .releases
        .iter()
        .filter(|release| satisfies(release.version, constraints))
        .map(|release| release.version)
        .collect::<Vec<_>>();
    candidates.sort_by(|a, b| b.cmp(a));

    let mut first_error = None;
    for candidate in candidates {
        if *attempts == max_attempts {
            return Err(ResolveError::Limit {
                attempts: max_attempts,
            });
        }
        *attempts += 1;
        assigned.push((index, candidate));
        let result = search(catalog, roots, assigned, attempts, max_attempts);
        assigned.pop();

        match result {
            Ok(solution) => return Ok(solution),
            Err(ResolveError::Limit { attempts }) => {
                return Err(ResolveError::Limit { attempts });
            }
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }

    Err(first_error.expect("active package had at least one candidate"))
}

pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
    max_attempts: usize,
) -> Result<Vec<Selection>, ResolveError> {
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

    if roots.is_empty() {
        return Ok(Vec::new());
    }

    search(catalog, roots, &mut Vec::new(), &mut 0, max_attempts)
}