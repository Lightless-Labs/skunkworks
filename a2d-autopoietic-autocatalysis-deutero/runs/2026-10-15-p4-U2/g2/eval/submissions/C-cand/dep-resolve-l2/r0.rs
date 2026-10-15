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
    if bytes.is_empty() || bytes.len() > 24 {
        return false;
    }
    if !bytes[0].is_ascii_lowercase() || bytes[bytes.len() - 1] == b'-' {
        return false;
    }
    bytes
        .iter()
        .all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
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
    let canonical = canonicalize(constraints.to_vec());
    canonical
        .iter()
        .map(|c| constraint_text(*c))
        .collect::<Vec<_>>()
        .join(",")
}

fn encode(value: &str) -> String {
    let mut result = String::new();
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'.' | b'_' | b'-') {
            result.push(*byte as char);
        } else {
            result.push('%');
            result.push_str(&format!("{:02X}", byte));
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
        ResolveError::Limit { attempts } => format!("limit|attempts={}", attempts),
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
        .all(|&constraint| matches_constraint(version, constraint))
}

fn validate_names(catalog: &[Package], roots: &[Requirement]) -> Result<(), ResolveError> {
    for (i, root) in roots.iter().enumerate() {
        if !valid_name(&root.package) {
            return Err(ResolveError::InvalidName {
                site: format!("root[{}]", i),
                name: root.package.clone(),
            });
        }
    }

    for (i, package) in catalog.iter().enumerate() {
        if !valid_name(&package.name) {
            return Err(ResolveError::InvalidName {
                site: format!("catalog[{}]", i),
                name: package.name.clone(),
            });
        }
        for (j, release) in package.releases.iter().enumerate() {
            for (k, dependency) in release.deps.iter().enumerate() {
                if !valid_name(&dependency.package) {
                    return Err(ResolveError::InvalidName {
                        site: format!("catalog[{}].release[{}].dep[{}]", i, j, k),
                        name: dependency.package.clone(),
                    });
                }
            }
        }
    }
    Ok(())
}

fn validate_duplicates(catalog: &[Package]) -> Result<(), ResolveError> {
    for i in 0..catalog.len() {
        if catalog[..i]
            .iter()
            .any(|previous| previous.name == catalog[i].name)
        {
            return Err(ResolveError::DuplicatePackage {
                package: catalog[i].name.clone(),
            });
        }
    }

    for package in catalog {
        for i in 0..package.releases.len() {
            if package.releases[..i]
                .iter()
                .any(|previous| previous.version == package.releases[i].version)
            {
                return Err(ResolveError::DuplicateVersion {
                    package: package.name.clone(),
                    version: package.releases[i].version,
                });
            }
        }
    }
    Ok(())
}

fn active_constraints(
    roots: &[Requirement],
    selected: &[(String, Version)],
    catalog: &[Package],
) -> std::collections::BTreeMap<String, Vec<Constraint>> {
    let mut active = std::collections::BTreeMap::new();

    for root in roots {
        active
            .entry(root.package.clone())
            .or_insert_with(Vec::new)
            .push(root.constraint);
    }

    for (package_name, version) in selected {
        if let Some(package) = catalog.iter().find(|p| p.name == *package_name) {
            if let Some(release) = package.releases.iter().find(|r| r.version == *version) {
                for dependency in &release.deps {
                    active
                        .entry(dependency.package.clone())
                        .or_insert_with(Vec::new)
                        .push(dependency.constraint);
                }
            }
        }
    }

    for constraints in active.values_mut() {
        *constraints = canonicalize(std::mem::take(constraints));
    }
    active
}

fn search(
    catalog: &[Package],
    roots: &[Requirement],
    selected: &mut Vec<(String, Version)>,
    attempts: &mut usize,
    max_attempts: usize,
) -> Result<Vec<Selection>, ResolveError> {
    let active = active_constraints(roots, selected, catalog);

    if let Some(package) = active
        .keys()
        .find(|name| !catalog.iter().any(|p| p.name == ***name))
    {
        return Err(ResolveError::Missing {
            package: package.clone(),
        });
    }

    for (name, version) in selected.iter() {
        if let Some(constraints) = active.get(name) {
            if !matches_all(*version, constraints) {
                return Err(ResolveError::Conflict {
                    package: name.clone(),
                    selected: *version,
                    requirements: constraints.clone(),
                });
            }
        }
    }

    let mut branch_package: Option<&str> = None;
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
        if branch_package.is_none() {
            branch_package = Some(name);
        }
    }

    let Some(package_name) = branch_package else {
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

    let constraints = active.get(package_name).unwrap();
    let package = catalog.iter().find(|p| p.name == package_name).unwrap();
    let mut candidates = package
        .releases
        .iter()
        .filter(|release| matches_all(release.version, constraints))
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
        selected.push((package_name.to_string(), version));

        match search(catalog, roots, selected, attempts, max_attempts) {
            Ok(solution) => return Ok(solution),
            Err(error @ ResolveError::Limit { .. }) => return Err(error),
            Err(error) => {
                selected.pop();
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }

    Err(first_error.unwrap_or_else(|| ResolveError::NoMatch {
        package: package_name.to_string(),
        requirements: constraints.clone(),
    }))
}

pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
    max_attempts: usize,
) -> Result<Vec<Selection>, ResolveError> {
    validate_names(catalog, roots)?;
    validate_duplicates(catalog)?;

    let mut attempts = 0;
    let mut selected = Vec::new();
    search(catalog, roots, &mut selected, &mut attempts, max_attempts)
}

fn main() {}