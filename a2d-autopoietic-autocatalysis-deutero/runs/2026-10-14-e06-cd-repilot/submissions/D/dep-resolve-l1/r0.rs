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

fn validate_names(catalog: &[Package], roots: &[Requirement]) -> Result<(), ResolveError> {
    for (i, req) in roots.iter().enumerate() {
        if !valid_name(&req.package) {
            return Err(ResolveError::InvalidName {
                site: format!("root[{i}]"),
                name: req.package.clone(),
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

fn fmt_version(v: Version) -> String {
    format!("{}.{}.{}", v.major, v.minor, v.patch)
}

fn fmt_constraint(c: Constraint) -> String {
    match c {
        Constraint::Exact(v) => format!("={}", fmt_version(v)),
        Constraint::AtLeast(v) => format!(">={}", fmt_version(v)),
        Constraint::Before(v) => format!("<{}", fmt_version(v)),
        Constraint::Caret(v) => format!("^{}", fmt_version(v)),
    }
}

fn canonical_constraint_text(constraints: &[Constraint]) -> String {
    canonicalize(constraints.to_vec())
        .into_iter()
        .map(fmt_constraint)
        .collect::<Vec<_>>()
        .join(",")
}

fn encode(text: &str) -> String {
    let mut result = String::new();
    for byte in text.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'.' | b'_' | b'-') {
            result.push(*byte as char);
        } else {
            result.push_str(&format!("%{byte:02X}"));
        }
    }
    result
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
            fmt_version(*version)
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
            canonical_constraint_text(requirements)
        ),
        ResolveError::Conflict {
            package,
            selected,
            requirements,
        } => format!(
            "conflict|package={}|selected={}|constraints={}",
            encode(package),
            fmt_version(*selected),
            canonical_constraint_text(requirements)
        ),
    }
}

pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
) -> Result<Vec<Selection>, ResolveError> {
    validate_names(catalog, roots)?;

    let mut package_indices = std::collections::BTreeMap::new();
    for (index, package) in catalog.iter().enumerate() {
        if package_indices.insert(package.name.as_str(), index).is_some() {
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

    let mut active: std::collections::BTreeMap<String, Vec<Constraint>> =
        std::collections::BTreeMap::new();
    for root in roots {
        active
            .entry(root.package.clone())
            .or_default()
            .push(root.constraint);
    }

    let mut selected: std::collections::BTreeMap<String, (Version, usize)> =
        std::collections::BTreeMap::new();

    loop {
        for constraints in active.values_mut() {
            *constraints = canonicalize(std::mem::take(constraints));
        }

        if let Some(package) = active
            .keys()
            .filter(|name| !package_indices.contains_key(name.as_str()))
            .next()
        {
            return Err(ResolveError::Missing {
                package: package.clone(),
            });
        }

        for (package, (version, _)) in &selected {
            if let Some(constraints) = active.get(package) {
                if !constraints.iter().all(|&c| matches(*version, c)) {
                    return Err(ResolveError::Conflict {
                        package: package.clone(),
                        selected: *version,
                        requirements: constraints.clone(),
                    });
                }
            }
        }

        for (package, constraints) in &active {
            if selected.contains_key(package) {
                continue;
            }
            let index = package_indices[package.as_str()];
            let has_match = catalog[index].releases.iter().any(|release| {
                constraints
                    .iter()
                    .all(|&constraint| matches(release.version, constraint))
            });
            if !has_match {
                return Err(ResolveError::NoMatch {
                    package: package.clone(),
                    requirements: constraints.clone(),
                });
            }
        }

        let next = active
            .keys()
            .find(|package| !selected.contains_key(*package))
            .cloned();
        let Some(package) = next else {
            break;
        };

        let index = package_indices[package.as_str()];
        let release_index = catalog[index]
            .releases
            .iter()
            .enumerate()
            .filter(|(_, release)| {
                active[&package]
                    .iter()
                    .all(|&constraint| matches(release.version, constraint))
            })
            .max_by_key(|(_, release)| release.version)
            .map(|(i, _)| i)
            .expect("candidate existence was checked");

        let release = &catalog[index].releases[release_index];
        selected.insert(package, (release.version, release_index));

        for dep in &release.deps {
            active
                .entry(dep.package.clone())
                .or_default()
                .push(dep.constraint);
        }
    }

    Ok(selected
        .into_iter()
        .map(|(package, (version, _))| Selection { package, version })
        .collect())
}