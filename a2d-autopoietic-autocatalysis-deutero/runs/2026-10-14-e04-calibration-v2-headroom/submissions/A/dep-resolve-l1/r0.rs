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

fn constraints_text(constraints: &[Constraint]) -> String {
    canonicalize(constraints.to_vec())
        .iter()
        .map(|c| constraint_text(*c))
        .collect::<Vec<_>>()
        .join(",")
}

fn encode(text: &str) -> String {
    let mut result = String::new();
    for byte in text.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'.' | b'_' | b'-') {
            result.push(*byte as char);
        } else {
            result.push('%');
            result.push(char::from_digit((byte >> 4) as u32, 16).unwrap().to_ascii_uppercase());
            result.push(char::from_digit((byte & 0x0f) as u32, 16).unwrap().to_ascii_uppercase());
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

pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
) -> Result<Vec<Selection>, ResolveError> {
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

    let mut package_names = std::collections::HashSet::new();
    for package in catalog {
        if !package_names.insert(package.name.as_str()) {
            return Err(ResolveError::DuplicatePackage {
                package: package.name.clone(),
            });
        }
    }

    for package in catalog {
        let mut versions = std::collections::HashSet::new();
        for release in &package.releases {
            if !versions.insert(release.version) {
                return Err(ResolveError::DuplicateVersion {
                    package: package.name.clone(),
                    version: release.version,
                });
            }
        }
    }

    let package_map: std::collections::HashMap<&str, &Package> = catalog
        .iter()
        .map(|package| (package.name.as_str(), package))
        .collect();

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
        let missing = active
            .keys()
            .filter(|name| !package_map.contains_key(name.as_str()))
            .next()
            .cloned();
        if let Some(package) = missing {
            return Err(ResolveError::Missing { package });
        }

        for (name, version) in &selected {
            if let Some(constraints) = active.get(name) {
                let constraints = canonicalize(constraints.clone());
                if constraints.iter().any(|c| !matches(*version, *c)) {
                    return Err(ResolveError::Conflict {
                        package: name.clone(),
                        selected: *version,
                        requirements: constraints,
                    });
                }
            }
        }

        for (name, constraints) in &active {
            if selected.contains_key(name) {
                continue;
            }
            let canonical = canonicalize(constraints.clone());
            let package = package_map[name.as_str()];
            if !package.releases.iter().any(|release| {
                canonical
                    .iter()
                    .all(|constraint| matches(release.version, *constraint))
            }) {
                return Err(ResolveError::NoMatch {
                    package: name.clone(),
                    requirements: canonical,
                });
            }
        }

        let next = active.keys().find(|name| !selected.contains_key(*name)).cloned();
        let Some(name) = next else {
            return Ok(selected
                .into_iter()
                .map(|(package, version)| Selection { package, version })
                .collect());
        };

        let constraints = canonicalize(active[&name].clone());
        let package = package_map[name.as_str()];
        let release = package
            .releases
            .iter()
            .filter(|release| {
                constraints
                    .iter()
                    .all(|constraint| matches(release.version, *constraint))
            })
            .max_by_key(|release| release.version)
            .expect("candidate existence was checked");

        selected.insert(name, release.version);
        for dep in &release.deps {
            active
                .entry(dep.package.clone())
                .or_default()
                .push(dep.constraint);
        }
    }
}