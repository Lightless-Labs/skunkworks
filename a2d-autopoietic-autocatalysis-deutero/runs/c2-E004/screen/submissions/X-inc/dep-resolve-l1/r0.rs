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
        && bytes.last() != Some(&b'-')
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

fn version_text(version: Version) -> String {
    format!("{}.{}.{}", version.major, version.minor, version.patch)
}

fn constraint_text(constraint: Constraint) -> String {
    match constraint {
        Constraint::Exact(version) => format!("={}", version_text(version)),
        Constraint::AtLeast(version) => format!(">={}", version_text(version)),
        Constraint::Before(version) => format!("<{}", version_text(version)),
        Constraint::Caret(version) => format!("^{}", version_text(version)),
    }
}

fn constraint_key(constraint: Constraint) -> (u8, Version) {
    match constraint {
        Constraint::Exact(version) => (0, version),
        Constraint::AtLeast(version) => (1, version),
        Constraint::Before(version) => (2, version),
        Constraint::Caret(version) => (3, version),
    }
}

fn canonical_constraints(constraints: &[Constraint]) -> Vec<Constraint> {
    let mut result = constraints.to_vec();
    result.sort_by_key(|constraint| constraint_key(*constraint));
    result.dedup();
    result
}

fn constraints_text(constraints: &[Constraint]) -> String {
    canonical_constraints(constraints)
        .iter()
        .map(|constraint| constraint_text(*constraint))
        .collect::<Vec<_>>()
        .join(",")
}

fn encode_text(value: &str) -> String {
    let mut encoded = String::new();
    for &byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-') {
            encoded.push(byte as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            encoded.push('%');
            encoded.push(HEX[(byte >> 4) as usize] as char);
            encoded.push(HEX[(byte & 0x0f) as usize] as char);
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

fn matches_constraint(version: Version, constraint: Constraint) -> bool {
    match constraint {
        Constraint::Exact(target) => version == target,
        Constraint::AtLeast(target) => version >= target,
        Constraint::Before(target) => version < target,
        Constraint::Caret(target) => {
            version >= target
                && if target.major > 0 {
                    version.major == target.major
                } else if target.minor > 0 {
                    version.major == 0 && version.minor == target.minor
                } else {
                    version == target
                }
        }
    }
}

fn matches_all(version: Version, constraints: &[Constraint]) -> bool {
    constraints
        .iter()
        .all(|constraint| matches_constraint(version, *constraint))
}

pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
) -> Result<Vec<Selection>, ResolveError> {
    for (index, root) in roots.iter().enumerate() {
        if !valid_name(&root.package) {
            return Err(ResolveError::InvalidName {
                site: format!("root[{index}]"),
                name: root.package.clone(),
            });
        }
    }

    for (package_index, package) in catalog.iter().enumerate() {
        if !valid_name(&package.name) {
            return Err(ResolveError::InvalidName {
                site: format!("catalog[{package_index}]"),
                name: package.name.clone(),
            });
        }
        for (release_index, release) in package.releases.iter().enumerate() {
            for (dependency_index, dependency) in release.deps.iter().enumerate() {
                if !valid_name(&dependency.package) {
                    return Err(ResolveError::InvalidName {
                        site: format!(
                            "catalog[{package_index}].release[{release_index}].dep[{dependency_index}]"
                        ),
                        name: dependency.package.clone(),
                    });
                }
            }
        }
    }

    let mut seen_packages = std::collections::HashSet::new();
    for package in catalog {
        if !seen_packages.insert(package.name.as_str()) {
            return Err(ResolveError::DuplicatePackage {
                package: package.name.clone(),
            });
        }
    }

    for package in catalog {
        let mut seen_versions = Vec::new();
        for release in &package.releases {
            if seen_versions.contains(&release.version) {
                return Err(ResolveError::DuplicateVersion {
                    package: package.name.clone(),
                    version: release.version,
                });
            }
            seen_versions.push(release.version);
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
        if let Some(name) = active
            .keys()
            .find(|name| !package_map.contains_key(name.as_str()))
        {
            return Err(ResolveError::Missing {
                package: name.clone(),
            });
        }

        for (name, version) in &selected {
            if let Some(constraints) = active.get(name) {
                if !matches_all(*version, constraints) {
                    return Err(ResolveError::Conflict {
                        package: name.clone(),
                        selected: *version,
                        requirements: canonical_constraints(constraints),
                    });
                }
            }
        }

        for (name, constraints) in &active {
            if selected.contains_key(name) {
                continue;
            }
            let package = package_map[name.as_str()];
            if !package
                .releases
                .iter()
                .any(|release| matches_all(release.version, constraints))
            {
                return Err(ResolveError::NoMatch {
                    package: name.clone(),
                    requirements: canonical_constraints(constraints),
                });
            }
        }

        let next_name = active
            .keys()
            .find(|name| !selected.contains_key(*name))
            .cloned();

        let Some(name) = next_name else {
            break;
        };

        let constraints = &active[&name];
        let package = package_map[name.as_str()];
        let release = package
            .releases
            .iter()
            .filter(|release| matches_all(release.version, constraints))
            .max_by_key(|release| release.version)
            .expect("candidate existence was checked");

        selected.insert(name, release.version);
        for dependency in &release.deps {
            active
                .entry(dependency.package.clone())
                .or_default()
                .push(dependency.constraint);
        }
    }

    Ok(selected
        .into_iter()
        .map(|(package, version)| Selection { package, version })
        .collect())
}