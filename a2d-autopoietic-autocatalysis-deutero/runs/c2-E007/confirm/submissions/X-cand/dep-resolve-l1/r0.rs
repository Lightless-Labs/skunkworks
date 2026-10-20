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

fn canonicalize(mut constraints: Vec<Constraint>) -> Vec<Constraint> {
    constraints.sort();
    constraints.dedup();
    constraints
}

fn constraints_text(constraints: &[Constraint]) -> String {
    canonicalize(constraints.to_vec())
        .iter()
        .map(|c| constraint_text(*c))
        .collect::<Vec<_>>()
        .join(",")
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

fn satisfies(version: Version, constraints: &[Constraint]) -> bool {
    constraints.iter().all(|c| matches(version, *c))
}

fn validate_name(site: String, name: &str) -> Result<(), ResolveError> {
    if valid_name(name) {
        Ok(())
    } else {
        Err(ResolveError::InvalidName {
            site,
            name: name.to_string(),
        })
    }
}

pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
) -> Result<Vec<Selection>, ResolveError> {
    for (i, root) in roots.iter().enumerate() {
        validate_name(format!("root[{i}]"), &root.package)?;
    }
    for (i, package) in catalog.iter().enumerate() {
        validate_name(format!("catalog[{i}]"), &package.name)?;
        for (j, release) in package.releases.iter().enumerate() {
            for (k, dep) in release.deps.iter().enumerate() {
                validate_name(
                    format!("catalog[{i}].release[{j}].dep[{k}]"),
                    &dep.package,
                )?;
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

    let by_name: std::collections::BTreeMap<&str, &Package> =
        catalog.iter().map(|p| (p.name.as_str(), p)).collect();

    let mut active: std::collections::BTreeMap<String, Vec<Constraint>> =
        std::collections::BTreeMap::new();
    for root in roots {
        active
            .entry(root.package.clone())
            .or_default()
            .push(root.constraint);
    }

    let mut selected: std::collections::BTreeMap<String, Version> =
        std::collections::BTreeMap::new();

    loop {
        for constraints in active.values_mut() {
            *constraints = canonicalize(std::mem::take(constraints));
        }

        if let Some(name) = active
            .keys()
            .find(|name| !by_name.contains_key(name.as_str()))
        {
            return Err(ResolveError::Missing {
                package: name.clone(),
            });
        }

        for (name, version) in &selected {
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
            let package = by_name[name.as_str()];
            if !package
                .releases
                .iter()
                .any(|r| satisfies(r.version, constraints))
            {
                return Err(ResolveError::NoMatch {
                    package: name.clone(),
                    requirements: constraints.clone(),
                });
            }
        }

        let next = active
            .keys()
            .find(|name| !selected.contains_key(*name))
            .cloned();
        let Some(name) = next else {
            break;
        };

        let constraints = &active[&name];
        let package = by_name[name.as_str()];
        let release = package
            .releases
            .iter()
            .filter(|r| satisfies(r.version, constraints))
            .max_by_key(|r| r.version)
            .expect("candidate existence was checked");

        selected.insert(name, release.version);
        for dep in &release.deps {
            active
                .entry(dep.package.clone())
                .or_default()
                .push(dep.constraint);
        }
    }

    Ok(selected
        .into_iter()
        .map(|(package, version)| Selection { package, version })
        .collect())
}

fn encode_text(text: &str) -> String {
    let mut result = String::new();
    for byte in text.as_bytes() {
        if byte.is_ascii_alphabetic()
            || byte.is_ascii_digit()
            || matches!(*byte, b'.' | b'_' | b'-')
        {
            result.push(*byte as char);
        } else {
            result.push_str(&format!("%{:02X}", byte));
        }
    }
    result
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
    }
}