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

fn validate_names(catalog: &[Package], roots: &[Requirement]) -> Result<(), ResolveError> {
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
    Ok(())
}

fn canonicalize(mut constraints: Vec<Constraint>) -> Vec<Constraint> {
    constraints.sort();
    constraints.dedup();
    constraints
}

fn matches(version: Version, constraint: Constraint) -> bool {
    match constraint {
        Constraint::Exact(v) => version == v,
        Constraint::AtLeast(v) => version >= v,
        Constraint::Before(v) => version < v,
        Constraint::Caret(v) => {
            if version < v {
                false
            } else if v.major > 0 {
                version.major == v.major
            } else if v.minor > 0 {
                version.major == 0 && version.minor == v.minor
            } else {
                version == v
            }
        }
    }
}

fn format_version(version: Version) -> String {
    format!("{}.{}.{}", version.major, version.minor, version.patch)
}

fn format_constraint(constraint: Constraint) -> String {
    match constraint {
        Constraint::Exact(v) => format!("={}", format_version(v)),
        Constraint::AtLeast(v) => format!(">={}", format_version(v)),
        Constraint::Before(v) => format!("<{}", format_version(v)),
        Constraint::Caret(v) => format!("^{}", format_version(v)),
    }
}

fn format_constraints(constraints: &[Constraint]) -> String {
    canonicalize(constraints.to_vec())
        .iter()
        .map(|c| format_constraint(*c))
        .collect::<Vec<_>>()
        .join(",")
}

fn encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
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
            format_version(*version)
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
            format_constraints(requirements)
        ),
        ResolveError::Conflict {
            package,
            selected,
            requirements,
        } => format!(
            "conflict|package={}|selected={}|constraints={}",
            encode(package),
            format_version(*selected),
            format_constraints(requirements)
        ),
    }
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
            *constraints = canonicalize(std::mem::take(constraints));
        }

        if let Some(package) = active
            .keys()
            .find(|name| !package_indices.contains_key(*name))
        {
            return Err(ResolveError::Missing {
                package: package.clone(),
            });
        }

        for (package, version) in &selected {
            let constraints = active.get(package).cloned().unwrap_or_default();
            if constraints.iter().any(|c| !matches(*version, *c)) {
                return Err(ResolveError::Conflict {
                    package: package.clone(),
                    selected: *version,
                    requirements: canonicalize(constraints),
                });
            }
        }

        for (package, constraints) in &active {
            if selected.contains_key(package) {
                continue;
            }
            let index = package_indices[package];
            let has_match = catalog[index].releases.iter().any(|release| {
                constraints
                    .iter()
                    .all(|constraint| matches(release.version, *constraint))
            });
            if !has_match {
                return Err(ResolveError::NoMatch {
                    package: package.clone(),
                    requirements: canonicalize(constraints.clone()),
                });
            }
        }

        let next = active
            .keys()
            .find(|package| !selected.contains_key(*package))
            .cloned();

        let Some(package) = next else {
            return Ok(selected
                .into_iter()
                .map(|(package, version)| Selection { package, version })
                .collect());
        };

        let index = package_indices[&package];
        let constraints = &active[&package];
        let release = catalog[index]
            .releases
            .iter()
            .filter(|release| {
                constraints
                    .iter()
                    .all(|constraint| matches(release.version, *constraint))
            })
            .max_by_key(|release| release.version)
            .expect("candidate existence checked");

        selected.insert(package, release.version);
        for dep in &release.deps {
            active
                .entry(dep.package.clone())
                .or_default()
                .push(dep.constraint);
        }
    }
}