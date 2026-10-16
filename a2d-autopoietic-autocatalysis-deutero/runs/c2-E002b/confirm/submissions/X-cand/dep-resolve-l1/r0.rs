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

fn constraint_text(constraint: Constraint) -> String {
    match constraint {
        Constraint::Exact(v) => format!("={}", version_text(v)),
        Constraint::AtLeast(v) => format!(">={}", version_text(v)),
        Constraint::Before(v) => format!("<{}", version_text(v)),
        Constraint::Caret(v) => format!("^{}", version_text(v)),
    }
}

fn canonical(mut constraints: Vec<Constraint>) -> Vec<Constraint> {
    constraints.sort();
    constraints.dedup();
    constraints
}

fn permits(constraint: Constraint, version: Version) -> bool {
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

fn encode(text: &str) -> String {
    let mut result = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-') {
            result.push(byte as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            result.push('%');
            result.push(HEX[(byte >> 4) as usize] as char);
            result.push(HEX[(byte & 0x0f) as usize] as char);
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
        } => {
            let constraints = canonical(requirements.clone())
                .into_iter()
                .map(constraint_text)
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "no-match|package={}|constraints={}",
                encode(package),
                constraints
            )
        }
        ResolveError::Conflict {
            package,
            selected,
            requirements,
        } => {
            let constraints = canonical(requirements.clone())
                .into_iter()
                .map(constraint_text)
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "conflict|package={}|selected={}|constraints={}",
                encode(package),
                version_text(*selected),
                constraints
            )
        }
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
        let package_site = format!("catalog[{i}]");
        if !valid_name(&package.name) {
            return Err(ResolveError::InvalidName {
                site: package_site.clone(),
                name: package.name.clone(),
            });
        }
        for (j, release) in package.releases.iter().enumerate() {
            for (k, dep) in release.deps.iter().enumerate() {
                if !valid_name(&dep.package) {
                    return Err(ResolveError::InvalidName {
                        site: format!("{package_site}.release[{j}].dep[{k}]"),
                        name: dep.package.clone(),
                    });
                }
            }
        }
    }

    let mut package_indices = std::collections::BTreeMap::new();
    for (i, package) in catalog.iter().enumerate() {
        if package_indices.contains_key(&package.name) {
            return Err(ResolveError::DuplicatePackage {
                package: package.name.clone(),
            });
        }
        package_indices.insert(package.name.clone(), i);
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
            .filter(|name| !package_indices.contains_key(*name))
            .next()
            .cloned();
        if let Some(package) = missing {
            return Err(ResolveError::Missing { package });
        }

        for (package, version) in &selected {
            let constraints = canonical(active.get(package).cloned().unwrap_or_default());
            if !constraints.iter().all(|c| permits(*c, *version)) {
                return Err(ResolveError::Conflict {
                    package: package.clone(),
                    selected: *version,
                    requirements: constraints,
                });
            }
        }

        for (package, constraints) in &active {
            if selected.contains_key(package) {
                continue;
            }
            let canonical_constraints = canonical(constraints.clone());
            let catalog_package = &catalog[package_indices[package]];
            if !catalog_package.releases.iter().any(|release| {
                canonical_constraints
                    .iter()
                    .all(|constraint| permits(*constraint, release.version))
            }) {
                return Err(ResolveError::NoMatch {
                    package: package.clone(),
                    requirements: canonical_constraints,
                });
            }
        }

        let next = active
            .keys()
            .find(|name| !selected.contains_key(*name))
            .cloned();
        let Some(package) = next else {
            break;
        };

        let constraints = canonical(active[&package].clone());
        let catalog_package = &catalog[package_indices[&package]];
        let version = catalog_package
            .releases
            .iter()
            .filter(|release| {
                constraints
                    .iter()
                    .all(|constraint| permits(*constraint, release.version))
            })
            .map(|release| release.version)
            .max()
            .expect("candidate existence checked above");

        selected.insert(package.clone(), version);
        let release = catalog_package
            .releases
            .iter()
            .find(|release| release.version == version)
            .expect("selected version exists");
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