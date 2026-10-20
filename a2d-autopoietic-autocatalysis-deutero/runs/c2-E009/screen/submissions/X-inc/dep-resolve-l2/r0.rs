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
    if bytes.is_empty() || bytes.len() > 24 || !bytes[0].is_ascii_lowercase() {
        return false;
    }
    if bytes.last() == Some(&b'-') {
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
        ResolveError::Limit { attempts } => format!("limit|attempts={attempts}"),
    }
}

fn matches(version: Version, constraint: Constraint) -> bool {
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

    let mut names = std::collections::BTreeSet::new();
    for package in catalog {
        if !names.insert(package.name.as_str()) {
            return Err(ResolveError::DuplicatePackage {
                package: package.name.clone(),
            });
        }
    }

    for package in catalog {
        let mut versions = std::collections::BTreeSet::new();
        for release in &package.releases {
            if !versions.insert(release.version) {
                return Err(ResolveError::DuplicateVersion {
                    package: package.name.clone(),
                    version: release.version,
                });
            }
        }
    }
    Ok(())
}

fn search(
    catalog: &std::collections::BTreeMap<&str, &Package>,
    roots: &[Requirement],
    selected: &mut std::collections::BTreeMap<String, (Version, Vec<Requirement>)>,
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
    for (package_name, (_, deps)) in selected.iter() {
        let _ = package_name;
        for dep in deps {
            active
                .entry(dep.package.clone())
                .or_default()
                .push(dep.constraint);
        }
    }

    for name in active.keys() {
        if !catalog.contains_key(name.as_str()) {
            return Err(ResolveError::Missing {
                package: name.clone(),
            });
        }
    }

    for (name, (version, _)) in selected.iter() {
        if let Some(constraints) = active.get(name) {
            if constraints.iter().any(|c| !matches(*version, *c)) {
                return Err(ResolveError::Conflict {
                    package: name.clone(),
                    selected: *version,
                    requirements: canonical(constraints.clone()),
                });
            }
        }
    }

    let mut candidates_by_name = std::collections::BTreeMap::new();
    for (name, constraints) in &active {
        if selected.contains_key(name) {
            continue;
        }
        let package = catalog[name.as_str()];
        let mut candidates: Vec<&Release> = package
            .releases
            .iter()
            .filter(|release| constraints.iter().all(|c| matches(release.version, *c)))
            .collect();
        candidates.sort_by(|a, b| b.version.cmp(&a.version));
        if candidates.is_empty() {
            return Err(ResolveError::NoMatch {
                package: name.clone(),
                requirements: canonical(constraints.clone()),
            });
        }
        candidates_by_name.insert(name.clone(), candidates);
    }

    let Some((name, candidates)) = candidates_by_name.into_iter().next() else {
        return Ok(selected
            .iter()
            .map(|(package, (version, _))| Selection {
                package: package.clone(),
                version: *version,
            })
            .collect());
    };

    let mut first_error = None;
    for release in candidates {
        if *attempts == max_attempts {
            return Err(ResolveError::Limit {
                attempts: max_attempts,
            });
        }
        *attempts += 1;
        selected.insert(name.clone(), (release.version, release.deps.clone()));
        match search(catalog, roots, selected, attempts, max_attempts) {
            Ok(solution) => return Ok(solution),
            Err(error @ ResolveError::Limit { .. }) => {
                selected.remove(&name);
                return Err(error);
            }
            Err(error) => {
                selected.remove(&name);
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }
    Err(first_error.expect("a nonempty candidate list has a failure if no branch succeeds"))
}

pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
    max_attempts: usize,
) -> Result<Vec<Selection>, ResolveError> {
    validate(catalog, roots)?;

    let catalog_map: std::collections::BTreeMap<&str, &Package> = catalog
        .iter()
        .map(|package| (package.name.as_str(), package))
        .collect();
    let mut selected = std::collections::BTreeMap::new();
    let mut attempts = 0;
    search(
        &catalog_map,
        roots,
        &mut selected,
        &mut attempts,
        max_attempts,
    )
}