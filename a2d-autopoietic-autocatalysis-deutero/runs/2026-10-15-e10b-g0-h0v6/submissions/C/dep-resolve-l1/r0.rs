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

fn constraint_order(constraint: Constraint) -> (u8, Version) {
    match constraint {
        Constraint::Exact(v) => (0, v),
        Constraint::AtLeast(v) => (1, v),
        Constraint::Before(v) => (2, v),
        Constraint::Caret(v) => (3, v),
    }
}

fn canonicalize(mut constraints: Vec<Constraint>) -> Vec<Constraint> {
    constraints.sort_by_key(|c| constraint_order(*c));
    constraints.dedup();
    constraints
}

fn constraints_text(constraints: &[Constraint]) -> String {
    let constraints = canonicalize(constraints.to_vec());
    constraints
        .into_iter()
        .map(|constraint| match constraint {
            Constraint::Exact(v) => format!("={}", version_text(v)),
            Constraint::AtLeast(v) => format!(">={}", version_text(v)),
            Constraint::Before(v) => format!("<{}", version_text(v)),
            Constraint::Caret(v) => format!("^{}", version_text(v)),
        })
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
            encoded.push(char::from(b"0123456789ABCDEF"[(byte >> 4) as usize]));
            encoded.push(char::from(b"0123456789ABCDEF"[(byte & 0x0f) as usize]));
        }
    }
    encoded
}

fn matches_constraint(version: Version, constraint: Constraint) -> bool {
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

fn matches_all(version: Version, constraints: &[Constraint]) -> bool {
    constraints
        .iter()
        .all(|constraint| matches_constraint(version, *constraint))
}

fn active_constraints(
    roots: &[Requirement],
    selected: &[(String, Version, Vec<Requirement>)],
) -> std::collections::BTreeMap<String, Vec<Constraint>> {
    let mut active = std::collections::BTreeMap::new();
    for requirement in roots {
        active
            .entry(requirement.package.clone())
            .or_insert_with(Vec::new)
            .push(requirement.constraint);
    }
    for (_, _, deps) in selected {
        for requirement in deps {
            active
                .entry(requirement.package.clone())
                .or_insert_with(Vec::new)
                .push(requirement.constraint);
        }
    }
    for constraints in active.values_mut() {
        *constraints = canonicalize(std::mem::take(constraints));
    }
    active
}

pub fn resolve(catalog: &[Package], roots: &[Requirement]) -> Result<Vec<Selection>, ResolveError> {
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

    let mut package_names = std::collections::BTreeSet::new();
    for package in catalog {
        if !package_names.insert(package.name.as_str()) {
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

    let by_name: std::collections::BTreeMap<&str, &Package> = catalog
        .iter()
        .map(|package| (package.name.as_str(), package))
        .collect();

    let mut selected: Vec<(String, Version, Vec<Requirement>)> = Vec::new();

    loop {
        let active = active_constraints(roots, &selected);

        if let Some(name) = active.keys().find(|name| !by_name.contains_key(name.as_str())) {
            return Err(ResolveError::Missing {
                package: name.clone(),
            });
        }

        let selected_map: std::collections::BTreeMap<&str, Version> = selected
            .iter()
            .map(|(name, version, _)| (name.as_str(), *version))
            .collect();

        for (name, constraints) in &active {
            if let Some(version) = selected_map.get(name.as_str()) {
                if !matches_all(*version, constraints) {
                    return Err(ResolveError::Conflict {
                        package: name.clone(),
                        selected: *version,
                        requirements: constraints.clone(),
                    });
                }
            }
        }

        for (name, constraints) in &active {
            if selected_map.contains_key(name.as_str()) {
                continue;
            }
            let package = by_name[name.as_str()];
            if !package
                .releases
                .iter()
                .any(|release| matches_all(release.version, constraints))
            {
                return Err(ResolveError::NoMatch {
                    package: name.clone(),
                    requirements: constraints.clone(),
                });
            }
        }

        let next = active
            .keys()
            .find(|name| !selected_map.contains_key(name.as_str()));
        let Some(name) = next else {
            break;
        };

        let constraints = &active[name];
        let package = by_name[name.as_str()];
        let release = package
            .releases
            .iter()
            .filter(|release| matches_all(release.version, constraints))
            .max_by_key(|release| release.version)
            .expect("matching release checked above");

        selected.push((
            name.clone(),
            release.version,
            release.deps.clone(),
        ));
    }

    selected.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(selected
        .into_iter()
        .map(|(package, version, _)| Selection { package, version })
        .collect())
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

fn main() {}