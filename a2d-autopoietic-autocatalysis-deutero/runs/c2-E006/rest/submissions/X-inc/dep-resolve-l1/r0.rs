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

fn encoded(text: &str) -> String {
    let mut out = String::new();
    for byte in text.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'.' | b'_' | b'-') {
            out.push(*byte as char);
        } else {
            out.push('%');
            out.push(char::from_digit((byte >> 4) as u32, 16).unwrap().to_ascii_uppercase());
            out.push(char::from_digit((byte & 0x0f) as u32, 16).unwrap().to_ascii_uppercase());
        }
    }
    out
}

fn constraints_text(constraints: &[Constraint]) -> String {
    canonical_constraints(constraints)
        .iter()
        .map(|c| constraint_text(*c))
        .collect::<Vec<_>>()
        .join(",")
}

pub fn error_text(error: &ResolveError) -> String {
    match error {
        ResolveError::InvalidName { site, name } => {
            format!("invalid-name|site={}|name={}", encoded(site), encoded(name))
        }
        ResolveError::DuplicatePackage { package } => {
            format!("duplicate-package|package={}", encoded(package))
        }
        ResolveError::DuplicateVersion { package, version } => format!(
            "duplicate-version|package={}|version={}",
            encoded(package),
            version_text(*version)
        ),
        ResolveError::Missing { package } => {
            format!("missing|package={}", encoded(package))
        }
        ResolveError::NoMatch {
            package,
            requirements,
        } => format!(
            "no-match|package={}|constraints={}",
            encoded(package),
            constraints_text(requirements)
        ),
        ResolveError::Conflict {
            package,
            selected,
            requirements,
        } => format!(
            "conflict|package={}|selected={}|constraints={}",
            encoded(package),
            version_text(*selected),
            constraints_text(requirements)
        ),
    }
}

fn permits(constraint: Constraint, version: Version) -> bool {
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

fn validate_names(catalog: &[Package], roots: &[Requirement]) -> Result<(), ResolveError> {
    for (i, requirement) in roots.iter().enumerate() {
        if !valid_name(&requirement.package) {
            return Err(ResolveError::InvalidName {
                site: format!("root[{i}]"),
                name: requirement.package.clone(),
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
            for (k, dependency) in release.deps.iter().enumerate() {
                if !valid_name(&dependency.package) {
                    return Err(ResolveError::InvalidName {
                        site: format!("catalog[{i}].release[{j}].dep[{k}]"),
                        name: dependency.package.clone(),
                    });
                }
            }
        }
    }
    Ok(())
}

pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
) -> Result<Vec<Selection>, ResolveError> {
    validate_names(catalog, roots)?;

    let mut package_indices = std::collections::BTreeMap::<String, usize>::new();
    for (i, package) in catalog.iter().enumerate() {
        if package_indices.insert(package.name.clone(), i).is_some() {
            return Err(ResolveError::DuplicatePackage {
                package: package.name.clone(),
            });
        }
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

    let mut active = std::collections::BTreeMap::<String, Vec<Constraint>>::new();
    for root in roots {
        active
            .entry(root.package.clone())
            .or_default()
            .push(root.constraint);
    }
    let mut selected = std::collections::BTreeMap::<String, Version>::new();

    loop {
        for constraints in active.values_mut() {
            *constraints = canonical_constraints(constraints);
        }

        if let Some(package) = active
            .keys()
            .filter(|name| !package_indices.contains_key(*name))
            .next()
        {
            return Err(ResolveError::Missing {
                package: package.clone(),
            });
        }

        for (package, version) in &selected {
            if let Some(constraints) = active.get(package) {
                if constraints.iter().any(|c| !permits(*c, *version)) {
                    return Err(ResolveError::Conflict {
                        package: package.clone(),
                        selected: *version,
                        requirements: constraints.clone(),
                    });
                }
            }
        }

        let mut next_package = None;
        for (package, constraints) in &active {
            if selected.contains_key(package) {
                continue;
            }
            let index = package_indices[package];
            let has_match = catalog[index]
                .releases
                .iter()
                .any(|release| constraints.iter().all(|c| permits(*c, release.version)));
            if !has_match {
                return Err(ResolveError::NoMatch {
                    package: package.clone(),
                    requirements: constraints.clone(),
                });
            }
            if next_package.is_none() {
                next_package = Some(package.clone());
            }
        }

        let Some(package) = next_package else {
            return Ok(selected
                .into_iter()
                .map(|(package, version)| Selection { package, version })
                .collect());
        };

        let constraints = &active[&package];
        let index = package_indices[&package];
        let release = catalog[index]
            .releases
            .iter()
            .filter(|release| constraints.iter().all(|c| permits(*c, release.version)))
            .max_by_key(|release| release.version)
            .expect("matching release was checked above");

        selected.insert(package, release.version);
        for dependency in &release.deps {
            active
                .entry(dependency.package.clone())
                .or_default()
                .push(dependency.constraint);
        }
    }
}