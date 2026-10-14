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

fn canonical_constraints(constraints: &[Constraint]) -> Vec<Constraint> {
    let mut result = constraints.to_vec();
    result.sort();
    result.dedup();
    result
}

fn constraints_text(constraints: &[Constraint]) -> String {
    canonical_constraints(constraints)
        .iter()
        .map(|c| constraint_text(*c))
        .collect::<Vec<_>>()
        .join(",")
}

fn encode_text(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'.' | b'_' | b'-') {
            encoded.push(*byte as char);
        } else {
            encoded.push('%');
            encoded.push(char::from_digit((byte >> 4) as u32, 16).unwrap().to_ascii_uppercase());
            encoded.push(char::from_digit((byte & 0x0f) as u32, 16).unwrap().to_ascii_uppercase());
        }
    }
    encoded
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
        ResolveError::Limit { attempts } => format!("limit|attempts={attempts}"),
    }
}

fn satisfies(version: Version, constraint: Constraint) -> bool {
    match constraint {
        Constraint::Exact(required) => version == required,
        Constraint::AtLeast(required) => version >= required,
        Constraint::Before(required) => version < required,
        Constraint::Caret(required) => {
            version >= required
                && if required.major > 0 {
                    version.major == required.major
                } else if required.minor > 0 {
                    version.major == 0 && version.minor == required.minor
                } else {
                    version == required
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

fn inspect_node(
    catalog: &[Package],
    roots: &[Requirement],
    selected: &std::collections::BTreeMap<String, Version>,
) -> Result<
    (
        std::collections::BTreeMap<String, Vec<Constraint>>,
        Option<String>,
    ),
    ResolveError,
> {
    let mut active: std::collections::BTreeMap<String, Vec<Constraint>> =
        std::collections::BTreeMap::new();

    for root in roots {
        active
            .entry(root.package.clone())
            .or_default()
            .push(root.constraint);
    }
    for (package_name, version) in selected {
        if let Some(package) = catalog.iter().find(|p| p.name == *package_name) {
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
        *constraints = canonical_constraints(constraints);
    }

    if let Some(name) = active
        .keys()
        .find(|name| !catalog.iter().any(|p| p.name == ***name))
    {
        return Err(ResolveError::Missing {
            package: name.clone(),
        });
    }

    for (name, version) in selected {
        if let Some(constraints) = active.get(name) {
            if constraints.iter().any(|c| !satisfies(*version, *c)) {
                return Err(ResolveError::Conflict {
                    package: name.clone(),
                    selected: *version,
                    requirements: constraints.clone(),
                });
            }
        }
    }

    for (name, constraints) in &active {
        if selected.contains_key(name) {
            continue;
        }
        let package = catalog.iter().find(|p| p.name == *name).unwrap();
        if !package.releases.iter().any(|release| {
            constraints
                .iter()
                .all(|constraint| satisfies(release.version, *constraint))
        }) {
            return Err(ResolveError::NoMatch {
                package: name.clone(),
                requirements: constraints.clone(),
            });
        }
    }

    let next = active.keys().find(|name| !selected.contains_key(*name)).cloned();
    Ok((active, next))
}

fn search(
    catalog: &[Package],
    roots: &[Requirement],
    selected: &mut std::collections::BTreeMap<String, Version>,
    attempts: &mut usize,
    max_attempts: usize,
) -> Result<(), ResolveError> {
    let (active, next) = inspect_node(catalog, roots, selected)?;
    let Some(package_name) = next else {
        return Ok(());
    };

    let package = catalog.iter().find(|p| p.name == package_name).unwrap();
    let constraints = active.get(&package_name).unwrap();
    let mut candidates: Vec<Version> = package
        .releases
        .iter()
        .filter(|release| {
            constraints
                .iter()
                .all(|constraint| satisfies(release.version, *constraint))
        })
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
        selected.insert(package_name.clone(), version);

        match search(catalog, roots, selected, attempts, max_attempts) {
            Ok(()) => return Ok(()),
            Err(error @ ResolveError::Limit { .. }) => return Err(error),
            Err(error) => {
                selected.remove(&package_name);
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }

    Err(first_error.unwrap_or_else(|| ResolveError::NoMatch {
        package: package_name,
        requirements: constraints.clone(),
    }))
}

pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
    max_attempts: usize,
) -> Result<Vec<Selection>, ResolveError> {
    validate(catalog, roots)?;

    let mut selected = std::collections::BTreeMap::new();
    let mut attempts = 0usize;
    search(catalog, roots, &mut selected, &mut attempts, max_attempts)?;

    Ok(selected
        .into_iter()
        .map(|(package, version)| Selection { package, version })
        .collect())
}