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

fn validate_names(catalog: &[Package], roots: &[Requirement]) -> Result<(), ResolveError> {
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
    Ok(())
}

fn canonical_constraints(mut constraints: Vec<Constraint>) -> Vec<Constraint> {
    constraints.sort();
    constraints.dedup();
    constraints
}

fn matches(version: Version, constraint: Constraint) -> bool {
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
        .all(|constraint| matches(version, *constraint))
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
    canonical_constraints(constraints.to_vec())
        .into_iter()
        .map(constraint_text)
        .collect::<Vec<_>>()
        .join(",")
}

fn encode_text(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.as_bytes() {
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
        ResolveError::Limit { attempts } => format!("limit|attempts={attempts}"),
    }
}

fn validate_duplicates(catalog: &[Package]) -> Result<(), ResolveError> {
    let mut seen_packages: Vec<&str> = Vec::new();
    for package in catalog {
        if seen_packages.contains(&package.name.as_str()) {
            return Err(ResolveError::DuplicatePackage {
                package: package.name.clone(),
            });
        }
        seen_packages.push(&package.name);
    }

    for package in catalog {
        let mut seen_versions: Vec<Version> = Vec::new();
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
    Ok(())
}

fn collect_active(
    roots: &[Requirement],
    catalog: &[Package],
    selected: &[(String, Version)],
) -> Vec<(String, Vec<Constraint>)> {
    let mut active: Vec<(String, Vec<Constraint>)> = Vec::new();

    fn add(active: &mut Vec<(String, Vec<Constraint>)>, name: &str, constraint: Constraint) {
        match active.iter_mut().find(|(existing, _)| existing == name) {
            Some((_, constraints)) => constraints.push(constraint),
            None => active.push((name.to_string(), vec![constraint])),
        }
    }

    for root in roots {
        add(&mut active, &root.package, root.constraint);
    }

    for (name, version) in selected {
        if let Some(package) = catalog.iter().find(|package| package.name == *name) {
            if let Some(release) = package.releases.iter().find(|release| release.version == *version) {
                for dep in &release.deps {
                    add(&mut active, &dep.package, dep.constraint);
                }
            }
        }
    }

    for (_, constraints) in &mut active {
        *constraints = canonical_constraints(std::mem::take(constraints));
    }
    active.sort_by(|a, b| a.0.cmp(&b.0));
    active
}

fn search(
    catalog: &[Package],
    roots: &[Requirement],
    selected: &mut Vec<(String, Version)>,
    attempts: &mut usize,
    max_attempts: usize,
) -> Result<Vec<Selection>, ResolveError> {
    let active = collect_active(roots, catalog, selected);

    if let Some((name, _)) = active
        .iter()
        .filter(|(name, _)| !catalog.iter().any(|package| package.name == *name))
        .min_by(|a, b| a.0.cmp(&b.0))
    {
        return Err(ResolveError::Missing {
            package: name.clone(),
        });
    }

    for (name, constraints) in &active {
        if let Some((_, version)) = selected.iter().find(|(selected_name, _)| selected_name == name) {
            if !matches_all(*version, constraints) {
                return Err(ResolveError::Conflict {
                    package: name.clone(),
                    selected: *version,
                    requirements: constraints.clone(),
                });
            }
        }
    }

    let mut candidates_by_name: Vec<(&str, &Vec<Constraint>, Vec<&Release>)> = Vec::new();
    for (name, constraints) in &active {
        if selected.iter().any(|(selected_name, _)| selected_name == name) {
            continue;
        }
        let package = catalog.iter().find(|package| package.name == *name).unwrap();
        let mut candidates: Vec<&Release> = package
            .releases
            .iter()
            .filter(|release| matches_all(release.version, constraints))
            .collect();
        candidates.sort_by(|a, b| b.version.cmp(&a.version));
        if candidates.is_empty() {
            return Err(ResolveError::NoMatch {
                package: name.clone(),
                requirements: constraints.clone(),
            });
        }
        candidates_by_name.push((name, constraints, candidates));
    }

    if candidates_by_name.is_empty() {
        let mut result: Vec<Selection> = selected
            .iter()
            .map(|(package, version)| Selection {
                package: package.clone(),
                version: *version,
            })
            .collect();
        result.sort_by(|a, b| a.package.cmp(&b.package));
        return Ok(result);
    }

    let (name, _, candidates) = candidates_by_name.remove(0);
    let package_name = name.to_string();
    let mut first_error: Option<ResolveError> = None;

    for release in candidates {
        if *attempts == max_attempts {
            return Err(ResolveError::Limit {
                attempts: max_attempts,
            });
        }
        *attempts += 1;
        selected.push((package_name.clone(), release.version));
        let result = search(catalog, roots, selected, attempts, max_attempts);
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

    Err(first_error.expect("non-empty candidates must produce a child result"))
}

pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
    max_attempts: usize,
) -> Result<Vec<Selection>, ResolveError> {
    validate_names(catalog, roots)?;
    validate_duplicates(catalog)?;

    let mut selected = Vec::new();
    let mut attempts = 0usize;
    search(catalog, roots, &mut selected, &mut attempts, max_attempts)
}