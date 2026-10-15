use std::collections::{BTreeMap, BTreeSet};

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

fn validate_names(catalog: &[Package], roots: &[Requirement]) -> Result<(), ResolveError> {
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

fn canonical_constraints(mut constraints: Vec<Constraint>) -> Vec<Constraint> {
    constraints.sort();
    constraints.dedup();
    constraints
}

fn matches(version: Version, constraint: Constraint) -> bool {
    match constraint {
        Constraint::Exact(bound) => version == bound,
        Constraint::AtLeast(bound) => version >= bound,
        Constraint::Before(bound) => version < bound,
        Constraint::Caret(bound) => {
            version >= bound
                && if bound.major > 0 {
                    version.major == bound.major
                } else if bound.minor > 0 {
                    version.major == 0 && version.minor == bound.minor
                } else {
                    version == bound
                }
        }
    }
}

fn constraints_match(version: Version, constraints: &[Constraint]) -> bool {
    constraints
        .iter()
        .copied()
        .all(|constraint| matches(version, constraint))
}

fn percent_encode(text: &str) -> String {
    let mut encoded = String::new();
    for byte in text.as_bytes() {
        if byte.is_ascii_alphabetic()
            || byte.is_ascii_digit()
            || matches!(*byte, b'.' | b'_' | b'-')
        {
            encoded.push(*byte as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            encoded.push('%');
            encoded.push(HEX[(byte >> 4) as usize] as char);
            encoded.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    encoded
}

fn constraints_text(constraints: &[Constraint]) -> String {
    canonical_constraints(constraints.to_vec())
        .into_iter()
        .map(constraint_text)
        .collect::<Vec<_>>()
        .join(",")
}

pub fn error_text(error: &ResolveError) -> String {
    match error {
        ResolveError::InvalidName { site, name } => format!(
            "invalid-name|site={}|name={}",
            percent_encode(site),
            percent_encode(name)
        ),
        ResolveError::DuplicatePackage { package } => {
            format!("duplicate-package|package={}", percent_encode(package))
        }
        ResolveError::DuplicateVersion { package, version } => format!(
            "duplicate-version|package={}|version={}",
            percent_encode(package),
            version_text(*version)
        ),
        ResolveError::Missing { package } => {
            format!("missing|package={}", percent_encode(package))
        }
        ResolveError::NoMatch {
            package,
            requirements,
        } => format!(
            "no-match|package={}|constraints={}",
            percent_encode(package),
            constraints_text(requirements)
        ),
        ResolveError::Conflict {
            package,
            selected,
            requirements,
        } => format!(
            "conflict|package={}|selected={}|constraints={}",
            percent_encode(package),
            version_text(*selected),
            constraints_text(requirements)
        ),
        ResolveError::Limit { attempts } => format!("limit|attempts={attempts}"),
    }
}

type ActiveConstraints = BTreeMap<String, Vec<Constraint>>;
type Assignments = BTreeMap<String, Version>;

fn gather_constraints(
    roots: &[Requirement],
    assignments: &Assignments,
    catalog_by_name: &BTreeMap<String, &Package>,
) -> ActiveConstraints {
    let mut active = ActiveConstraints::new();

    for requirement in roots {
        active
            .entry(requirement.package.clone())
            .or_default()
            .push(requirement.constraint);
    }

    for (package_name, version) in assignments {
        if let Some(package) = catalog_by_name.get(package_name) {
            if let Some(release) = package
                .releases
                .iter()
                .find(|release| release.version == *version)
            {
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
        *constraints = canonical_constraints(std::mem::take(constraints));
    }
    active
}

fn inspect_node(
    catalog_by_name: &BTreeMap<String, &Package>,
    roots: &[Requirement],
    assignments: &Assignments,
) -> Result<ActiveConstraints, ResolveError> {
    let mut active = gather_constraints(roots, assignments, catalog_by_name);

    if let Some(package) = active
        .keys()
        .filter(|name| !catalog_by_name.contains_key(*name))
        .next()
    {
        return Err(ResolveError::Missing {
            package: package.clone(),
        });
    }

    for (package, selected) in assignments {
        if let Some(requirements) = active.get(package) {
            if !constraints_match(*selected, requirements) {
                return Err(ResolveError::Conflict {
                    package: package.clone(),
                    selected: *selected,
                    requirements: requirements.clone(),
                });
            }
        }
    }

    for (package, requirements) in &active {
        if !assignments.contains_key(package) {
            let catalog_package = catalog_by_name[package];
            if !catalog_package
                .releases
                .iter()
                .any(|release| constraints_match(release.version, requirements))
            {
                return Err(ResolveError::NoMatch {
                    package: package.clone(),
                    requirements: requirements.clone(),
                });
            }
        }
    }

    Ok(std::mem::take(&mut active))
}

fn search(
    catalog_by_name: &BTreeMap<String, &Package>,
    roots: &[Requirement],
    assignments: &mut Assignments,
    attempts: &mut usize,
    max_attempts: usize,
) -> Result<Vec<Selection>, ResolveError> {
    let active = inspect_node(catalog_by_name, roots, assignments)?;

    let next_package = active
        .keys()
        .find(|package| !assignments.contains_key(*package))
        .cloned();

    let Some(package_name) = next_package else {
        return Ok(assignments
            .iter()
            .map(|(package, version)| Selection {
                package: package.clone(),
                version: *version,
            })
            .collect());
    };

    let package = catalog_by_name[&package_name];
    let mut candidates: Vec<&Release> = package
        .releases
        .iter()
        .filter(|release| {
            constraints_match(release.version, &active[&package_name])
        })
        .collect();
    candidates.sort_by(|a, b| b.version.cmp(&a.version));

    let mut first_error = None;
    for release in candidates {
        if *attempts == max_attempts {
            return Err(ResolveError::Limit {
                attempts: max_attempts,
            });
        }
        *attempts += 1;
        assignments.insert(package_name.clone(), release.version);

        match search(
            catalog_by_name,
            roots,
            assignments,
            attempts,
            max_attempts,
        ) {
            Ok(solution) => return Ok(solution),
            Err(error @ ResolveError::Limit { .. }) => return Err(error),
            Err(error) => {
                assignments.remove(&package_name);
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }

    Err(first_error.unwrap_or_else(|| ResolveError::NoMatch {
        package: package_name.clone(),
        requirements: active[&package_name].clone(),
    }))
}

pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
    max_attempts: usize,
) -> Result<Vec<Selection>, ResolveError> {
    validate_names(catalog, roots)?;

    let mut seen_packages = BTreeSet::new();
    for package in catalog {
        if !seen_packages.insert(package.name.as_str()) {
            return Err(ResolveError::DuplicatePackage {
                package: package.name.clone(),
            });
        }
    }

    for package in catalog {
        let mut seen_versions = BTreeSet::new();
        for release in &package.releases {
            if !seen_versions.insert(release.version) {
                return Err(ResolveError::DuplicateVersion {
                    package: package.name.clone(),
                    version: release.version,
                });
            }
        }
    }

    let catalog_by_name: BTreeMap<String, &Package> = catalog
        .iter()
        .map(|package| (package.name.clone(), package))
        .collect();
    let mut assignments = Assignments::new();
    let mut attempts = 0;

    search(
        &catalog_by_name,
        roots,
        &mut assignments,
        &mut attempts,
        max_attempts,
    )
}

fn main() {}