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

fn canonical(mut constraints: Vec<Constraint>) -> Vec<Constraint> {
    constraints.sort();
    constraints.dedup();
    constraints
}

fn active_constraints(
    roots: &[Requirement],
    selected: &[(String, Version, Vec<Requirement>)],
) -> std::collections::BTreeMap<String, Vec<Constraint>> {
    let mut active = std::collections::BTreeMap::<String, Vec<Constraint>>::new();
    for req in roots {
        active
            .entry(req.package.clone())
            .or_default()
            .push(req.constraint);
    }
    for (_, _, deps) in selected {
        for req in deps {
            active
                .entry(req.package.clone())
                .or_default()
                .push(req.constraint);
        }
    }
    for constraints in active.values_mut() {
        *constraints = canonical(std::mem::take(constraints));
    }
    active
}

fn resolve_inner(
    catalog: &[Package],
    roots: &[Requirement],
    selected: &mut Vec<(String, Version, Vec<Requirement>)>,
) -> Result<(), ResolveError> {
    let active = active_constraints(roots, selected);
    let selected_names: std::collections::BTreeSet<&str> =
        selected.iter().map(|s| s.0.as_str()).collect();

    for package in active.keys() {
        if !catalog.iter().any(|p| p.name == *package) {
            return Err(ResolveError::Missing {
                package: package.clone(),
            });
        }
    }

    for (package, constraints) in &active {
        if let Some((_, version, _)) = selected.iter().find(|s| s.0 == *package) {
            if !constraints.iter().all(|c| matches(*version, *c)) {
                return Err(ResolveError::Conflict {
                    package: package.clone(),
                    selected: *version,
                    requirements: constraints.clone(),
                });
            }
        }
    }

    for (package, constraints) in &active {
        if selected_names.contains(package.as_str()) {
            continue;
        }
        let entry = catalog.iter().find(|p| p.name == *package).unwrap();
        if !entry
            .releases
            .iter()
            .any(|r| constraints.iter().all(|c| matches(r.version, *c)))
        {
            return Err(ResolveError::NoMatch {
                package: package.clone(),
                requirements: constraints.clone(),
            });
        }
    }

    let next = active
        .keys()
        .find(|name| !selected_names.contains(name.as_str()))
        .cloned();
    let Some(package) = next else {
        return Ok(());
    };

    let constraints = &active[&package];
    let entry = catalog.iter().find(|p| p.name == package).unwrap();
    let release = entry
        .releases
        .iter()
        .filter(|r| constraints.iter().all(|c| matches(r.version, *c)))
        .max_by_key(|r| r.version)
        .unwrap();

    selected.push((package, release.version, release.deps.clone()));
    resolve_inner(catalog, roots, selected)
}

pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
) -> Result<Vec<Selection>, ResolveError> {
    validate_names(catalog, roots)?;

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

    let mut selected = Vec::new();
    resolve_inner(catalog, roots, &mut selected)?;
    selected.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(selected
        .into_iter()
        .map(|(package, version, _)| Selection { package, version })
        .collect())
}

fn encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'.' | b'_' | b'-') {
            out.push(*byte as char);
        } else {
            out.push('%');
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    out
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
    canonical(constraints.to_vec())
        .iter()
        .map(|c| constraint_text(*c))
        .collect::<Vec<_>>()
        .join(",")
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