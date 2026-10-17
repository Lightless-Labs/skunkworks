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
    Limit {
        attempts: usize,
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

fn encode_text(text: &str) -> String {
    let mut out = String::new();
    for byte in text.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'.' | b'_' | b'-') {
            out.push(*byte as char);
        } else {
            out.push_str(&format!("%{:02X}", byte));
        }
    }
    out
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
        } => {
            let constraints = canonicalize(requirements.clone())
                .iter()
                .map(|c| constraint_text(*c))
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "no-match|package={}|constraints={}",
                encode_text(package),
                constraints
            )
        }
        ResolveError::Conflict {
            package,
            selected,
            requirements,
        } => {
            let constraints = canonicalize(requirements.clone())
                .iter()
                .map(|c| constraint_text(*c))
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "conflict|package={}|selected={}|constraints={}",
                encode_text(package),
                version_text(*selected),
                constraints
            )
        }
        ResolveError::Limit { attempts } => format!("limit|attempts={attempts}"),
    }
}

fn matches_constraint(version: Version, constraint: Constraint) -> bool {
    match constraint {
        Constraint::Exact(wanted) => version == wanted,
        Constraint::AtLeast(minimum) => version >= minimum,
        Constraint::Before(bound) => version < bound,
        Constraint::Caret(base) => {
            version >= base
                && if base.major > 0 {
                    version.major == base.major
                } else if base.minor > 0 {
                    version.major == 0 && version.minor == base.minor
                } else {
                    version == base
                }
        }
    }
}

fn matches_all(version: Version, constraints: &[Constraint]) -> bool {
    constraints
        .iter()
        .all(|constraint| matches_constraint(version, *constraint))
}

fn validate(catalog: &[Package], roots: &[Requirement]) -> Result<(), ResolveError> {
    for (i, requirement) in roots.iter().enumerate() {
        if !valid_name(&requirement.package) {
            return Err(ResolveError::InvalidName {
                site: format!("root[{i}]"),
                name: requirement.package.clone(),
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
            for (k, dependency) in release.deps.iter().enumerate() {
                if !valid_name(&dependency.package) {
                    return Err(ResolveError::InvalidName {
                        site: format!("catalog[{i}].release[{j}].dep[{k}]"),
                        name: dependency.package.clone(),
                    });
                }
            }
        }
    }
    Ok(())
}

fn resolve_node(
    catalog: &[Package],
    roots: &[Requirement],
    selected: &mut Vec<(String, Version)>,
    attempts: &mut usize,
    max_attempts: usize,
) -> Result<Vec<Selection>, ResolveError> {
    let mut active: std::collections::BTreeMap<String, Vec<Constraint>> =
        std::collections::BTreeMap::new();

    for root in roots {
        active
            .entry(root.package.clone())
            .or_default()
            .push(root.constraint);
    }

    for (name, version) in selected.iter() {
        if let Some(package) = catalog.iter().find(|p| p.name == *name) {
            if let Some(release) = package.releases.iter().find(|r| r.version == *version) {
                for dependency in &release.deps {
                    active
                        .entry(dependency.package.clone())
                        .or_default()
                        .push(dependency.constraint);
                }
            }
        }
    }
    for constraints in active.values_mut() {
        *constraints = canonicalize(std::mem::take(constraints));
    }

    let missing = active
        .keys()
        .filter(|name| !catalog.iter().any(|p| p.name == **name))
        .next()
        .cloned();
    if let Some(package) = missing {
        return Err(ResolveError::Missing { package });
    }

    let mut ordered_selected = selected.clone();
    ordered_selected.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, version) in &ordered_selected {
        let constraints = active.get(name).cloned().unwrap_or_default();
        if !matches_all(*version, &constraints) {
            return Err(ResolveError::Conflict {
                package: name.clone(),
                selected: *version,
                requirements: constraints,
            });
        }
    }

    for (name, constraints) in &active {
        if selected.iter().any(|(selected_name, _)| selected_name == name) {
            continue;
        }
        let package = catalog.iter().find(|p| p.name == *name).unwrap();
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
        .find(|name| !selected.iter().any(|(selected_name, _)| selected_name == *name))
        .cloned();

    let Some(package_name) = next else {
        let mut result = selected
            .iter()
            .map(|(package, version)| Selection {
                package: package.clone(),
                version: *version,
            })
            .collect::<Vec<_>>();
        result.sort_by(|a, b| a.package.cmp(&b.package));
        return Ok(result);
    };

    let package = catalog.iter().find(|p| p.name == package_name).unwrap();
    let constraints = active.get(&package_name).cloned().unwrap_or_default();
    let mut candidates = package
        .releases
        .iter()
        .filter(|release| matches_all(release.version, &constraints))
        .map(|release| release.version)
        .collect::<Vec<_>>();
    candidates.sort_by(|a, b| b.cmp(a));

    let mut first_error = None;
    for version in candidates {
        if *attempts == max_attempts {
            return Err(ResolveError::Limit {
                attempts: max_attempts,
            });
        }
        *attempts += 1;
        selected.push((package_name.clone(), version));
        let result = resolve_node(catalog, roots, selected, attempts, max_attempts);
        selected.pop();

        match result {
            Ok(solution) => return Ok(solution),
            Err(error @ ResolveError::Limit { .. }) => return Err(error),
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }

    Err(first_error.unwrap_or_else(|| ResolveError::NoMatch {
        package: package_name,
        requirements: constraints,
    }))
}

pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
    max_attempts: usize,
) -> Result<Vec<Selection>, ResolveError> {
    validate(catalog, roots)?;

    let mut package_names = std::collections::BTreeSet::new();
    for package in catalog {
        if !package_names.insert(package.name.clone()) {
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

    let mut attempts = 0;
    let mut selected = Vec::new();
    resolve_node(catalog, roots, &mut selected, &mut attempts, max_attempts)
}