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
        .all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn matches(version: Version, constraint: Constraint) -> bool {
    match constraint {
        Constraint::Exact(x) => version == x,
        Constraint::AtLeast(x) => version >= x,
        Constraint::Before(x) => version < x,
        Constraint::Caret(x) => {
            version >= x
                && if x.major > 0 {
                    version.major == x.major
                } else if x.minor > 0 {
                    version.major == 0 && version.minor == x.minor
                } else {
                    version == x
                }
        }
    }
}

fn canonicalize(mut constraints: Vec<Constraint>) -> Vec<Constraint> {
    constraints.sort();
    constraints.dedup();
    constraints
}

fn canonical_constraints(
    active: &std::collections::BTreeMap<String, Vec<Constraint>>,
    package: &str,
) -> Vec<Constraint> {
    canonicalize(active.get(package).cloned().unwrap_or_default())
}

fn resolve_node(
    catalog: &[Package],
    roots_active: &std::collections::BTreeMap<String, Vec<Constraint>>,
    selected: &mut std::collections::BTreeMap<String, Version>,
) -> Result<(), ResolveError> {
    let mut active = roots_active.clone();

    for (package, version) in selected.iter() {
        if let Some(definition) = catalog.iter().find(|p| p.name == *package) {
            if let Some(release) = definition.releases.iter().find(|r| r.version == *version) {
                for dep in &release.deps {
                    active
                        .entry(dep.package.clone())
                        .or_default()
                        .push(dep.constraint);
                }
            }
        }
    }

    if let Some(package) = active
        .keys()
        .filter(|name| !catalog.iter().any(|p| p.name == ***name))
        .next()
    {
        return Err(ResolveError::Missing {
            package: package.clone(),
        });
    }

    for (package, version) in selected.iter() {
        let constraints = canonical_constraints(&active, package);
        if constraints
            .iter()
            .any(|constraint| !matches(*version, *constraint))
        {
            return Err(ResolveError::Conflict {
                package: package.clone(),
                selected: *version,
                requirements: constraints,
            });
        }
    }

    for package in active.keys() {
        if selected.contains_key(package) {
            continue;
        }
        let definition = catalog.iter().find(|p| p.name == *package).unwrap();
        let constraints = canonical_constraints(&active, package);
        if !definition.releases.iter().any(|release| {
            constraints
                .iter()
                .all(|constraint| matches(release.version, *constraint))
        }) {
            return Err(ResolveError::NoMatch {
                package: package.clone(),
                requirements: constraints,
            });
        }
    }

    let next = active.keys().find(|package| !selected.contains_key(*package));
    let Some(package) = next else {
        return Ok(());
    };

    let definition = catalog.iter().find(|p| p.name == *package).unwrap();
    let constraints = canonical_constraints(&active, package);
    let version = definition
        .releases
        .iter()
        .filter(|release| {
            constraints
                .iter()
                .all(|constraint| matches(release.version, *constraint))
        })
        .map(|release| release.version)
        .max()
        .unwrap();

    selected.insert(package.clone(), version);
    resolve_node(catalog, roots_active, selected)
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

    let mut active = std::collections::BTreeMap::<String, Vec<Constraint>>::new();
    for root in roots {
        active
            .entry(root.package.clone())
            .or_default()
            .push(root.constraint);
    }

    let mut selected = std::collections::BTreeMap::new();
    resolve_node(catalog, &active, &mut selected)?;

    Ok(selected
        .into_iter()
        .map(|(package, version)| Selection { package, version })
        .collect())
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

fn encode_text(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn constraints_text(constraints: &[Constraint]) -> String {
    canonicalize(constraints.to_vec())
        .into_iter()
        .map(constraint_text)
        .collect::<Vec<_>>()
        .join(",")
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