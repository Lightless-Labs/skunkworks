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
    !bytes.is_empty()
        && bytes.len() <= 24
        && bytes[0].is_ascii_lowercase()
        && bytes[bytes.len() - 1] != b'-'
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

fn canonicalize(constraints: &mut Vec<Constraint>) {
    constraints.sort();
    constraints.dedup();
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
    constraints.iter().all(|&c| matches(version, c))
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

fn constraints_text(constraints: &[Constraint]) -> String {
    let mut constraints = constraints.to_vec();
    canonicalize(&mut constraints);
    constraints
        .into_iter()
        .map(constraint_text)
        .collect::<Vec<_>>()
        .join(",")
}

fn encode_text(text: &str) -> String {
    let mut encoded = String::new();
    for byte in text.as_bytes() {
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

    let package_map: std::collections::BTreeMap<&str, &Package> = catalog
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
        for constraints in active.values_mut() {
            canonicalize(constraints);
        }

        if let Some(package) = active
            .keys()
            .filter(|name| !package_map.contains_key(name.as_str()))
            .next()
        {
            return Err(ResolveError::Missing {
                package: package.clone(),
            });
        }

        for (package, version) in &selected {
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

        let mut next: Option<String> = None;
        for package in active.keys() {
            if !selected.contains_key(package) {
                let catalog_package = package_map[package.as_str()];
                let constraints = &active[package];
                let has_match = catalog_package
                    .releases
                    .iter()
                    .any(|release| satisfies(release.version, constraints));
                if !has_match {
                    return Err(ResolveError::NoMatch {
                        package: package.clone(),
                        requirements: constraints.clone(),
                    });
                }
                if next.is_none() {
                    next = Some(package.clone());
                }
            }
        }

        let Some(package) = next else {
            return Ok(selected
                .into_iter()
                .map(|(package, version)| Selection { package, version })
                .collect());
        };

        let constraints = &active[&package];
        let release = package_map[package.as_str()]
            .releases
            .iter()
            .filter(|release| satisfies(release.version, constraints))
            .max_by_key(|release| release.version)
            .unwrap();

        selected.insert(package, release.version);
        for dep in &release.deps {
            active
                .entry(dep.package.clone())
                .or_default()
                .push(dep.constraint);
        }
    }
}