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

fn canonical_constraints(mut constraints: Vec<Constraint>) -> Vec<Constraint> {
    constraints.sort();
    constraints.dedup();
    constraints
}

fn constraints_text(constraints: &[Constraint]) -> String {
    canonical_constraints(constraints.to_vec())
        .iter()
        .map(|c| constraint_text(*c))
        .collect::<Vec<_>>()
        .join(",")
}

fn encode_text(input: &str) -> String {
    let mut output = String::new();
    for byte in input.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'.' | b'_' | b'-') {
            output.push(*byte as char);
        } else {
            output.push('%');
            output.push(char::from_digit((byte >> 4) as u32, 16).unwrap().to_ascii_uppercase());
            output.push(char::from_digit((byte & 0x0f) as u32, 16).unwrap().to_ascii_uppercase());
        }
    }
    output
}

pub fn error_text(error: &ResolveError) -> String {
    match error {
        ResolveError::InvalidName { site, name } => format!(
            "invalid-name|site={}|name={}",
            encode_text(site),
            encode_text(name)
        ),
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
        Constraint::Exact(required) => version == required,
        Constraint::AtLeast(required) => version >= required,
        Constraint::Before(required) => version < required,
        Constraint::Caret(required) => {
            if version < required {
                false
            } else if required.major > 0 {
                version.major == required.major
            } else if required.minor > 0 {
                version.major == 0 && version.minor == required.minor
            } else {
                version == required
            }
        }
    }
}

fn satisfies(version: Version, constraints: &[Constraint]) -> bool {
    constraints
        .iter()
        .all(|constraint| matches_constraint(version, *constraint))
}

fn validate_inputs(catalog: &[Package], roots: &[Requirement]) -> Result<(), ResolveError> {
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
            for (k, dependency) in release.deps.iter().enumerate() {
                if !valid_name(&dependency.package) {
                    return Err(ResolveError::InvalidName {
                        site: format!("catalog[{}].release[{}].dep[{}]", i, j, k),
                        name: dependency.package.clone(),
                    });
                }
            }
        }
    }

    Ok(())
}

fn build_catalog<'a>(
    catalog: &'a [Package],
) -> Result<std::collections::BTreeMap<&'a str, &'a Package>, ResolveError> {
    let mut packages = std::collections::BTreeMap::new();
    for package in catalog {
        if packages.contains_key(package.name.as_str()) {
            return Err(ResolveError::DuplicatePackage {
                package: package.name.clone(),
            });
        }
        packages.insert(package.name.as_str(), package);
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

    Ok(packages)
}

fn active_constraints(
    roots: &[Requirement],
    selected: &std::collections::BTreeMap<String, Version>,
    catalog: &std::collections::BTreeMap<&str, &Package>,
) -> std::collections::BTreeMap<String, Vec<Constraint>> {
    let mut active = std::collections::BTreeMap::<String, Vec<Constraint>>::new();

    for root in roots {
        active
            .entry(root.package.clone())
            .or_default()
            .push(root.constraint);
    }

    for (name, version) in selected {
        if let Some(package) = catalog.get(name.as_str()) {
            if let Some(release) = package
                .releases
                .iter()
                .find(|release| release.version == *version)
            {
                for dependency in &release.deps {
                    active
                        .entry(dependency.package.clone())
                        .or_default()
                        .push(dependency.constraint);
                }
            }
        }
    }

    for constraints in active.values_mut() {
        *constraints = canonical_constraints(std::mem::take(constraints));
    }

    active
}

fn search(
    roots: &[Requirement],
    catalog: &std::collections::BTreeMap<&str, &Package>,
    selected: &mut std::collections::BTreeMap<String, Version>,
    attempts: &mut usize,
    max_attempts: usize,
) -> Result<Vec<Selection>, ResolveError> {
    let active = active_constraints(roots, selected, catalog);

    for name in active.keys() {
        if !catalog.contains_key(name.as_str()) {
            return Err(ResolveError::Missing {
                package: name.clone(),
            });
        }
    }

    for (name, version) in selected.iter() {
        if let Some(constraints) = active.get(name) {
            if !satisfies(*version, constraints) {
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
        let package = catalog[name.as_str()];
        if !package
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

    let next = active.keys().find(|name| !selected.contains_key(*name));
    let Some(name) = next else {
        return Ok(selected
            .iter()
            .map(|(package, version)| Selection {
                package: package.clone(),
                version: *version,
            })
            .collect());
    };

    let constraints = &active[name];
    let package = catalog[name.as_str()];
    let mut candidates: Vec<Version> = package
        .releases
        .iter()
        .map(|release| release.version)
        .filter(|version| satisfies(*version, constraints))
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
        selected.insert(name.clone(), version);
        let result = search(roots, catalog, selected, attempts, max_attempts);
        selected.remove(name);

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
        package: name.clone(),
        requirements: constraints.clone(),
    }))
}

pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
    max_attempts: usize,
) -> Result<Vec<Selection>, ResolveError> {
    validate_inputs(catalog, roots)?;
    let package_map = build_catalog(catalog)?;
    let mut attempts = 0;
    let mut selected = std::collections::BTreeMap::new();
    search(
        roots,
        &package_map,
        &mut selected,
        &mut attempts,
        max_attempts,
    )
}