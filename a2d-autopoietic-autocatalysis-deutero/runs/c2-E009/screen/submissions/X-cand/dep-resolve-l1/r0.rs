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

use std::collections::{BTreeMap, BTreeSet};

fn valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.is_empty() || bytes.len() > 24 || !bytes[0].is_ascii_lowercase() {
        return false;
    }

    bytes
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
        && bytes.last().is_some_and(|b| *b != b'-')
}

fn version_text(version: Version) -> String {
    format!("{}.{}.{}", version.major, version.minor, version.patch)
}

fn constraint_order(constraint: &Constraint) -> (u8, Version) {
    match *constraint {
        Constraint::Exact(v) => (0, v),
        Constraint::AtLeast(v) => (1, v),
        Constraint::Before(v) => (2, v),
        Constraint::Caret(v) => (3, v),
    }
}

fn canonicalize(constraints: &[Constraint]) -> Vec<Constraint> {
    let mut result = constraints.to_vec();
    result.sort_by_key(constraint_order);
    result.dedup();
    result
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
    canonicalize(constraints)
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
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            encoded.push('%');
            encoded.push(HEX[(byte >> 4) as usize] as char);
            encoded.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    encoded
}

fn permits(constraint: Constraint, version: Version) -> bool {
    match constraint {
        Constraint::Exact(required) => version == required,
        Constraint::AtLeast(required) => version >= required,
        Constraint::Before(bound) => version < bound,
        Constraint::Caret(required) => {
            if version < required {
                false
            } else if required.major > 0 {
                version.major == required.major
            } else if required.minor > 0 {
                version.major == 0 && version.minor == required.minor
            } else {
                version == required
            }
        }
    }
}

fn satisfies(version: Version, constraints: &[Constraint]) -> bool {
    constraints
        .iter()
        .all(|constraint| permits(*constraint, version))
}

fn canonical_error_constraints(constraints: &[Constraint]) -> String {
    constraints_text(constraints)
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
            canonical_error_constraints(requirements)
        ),
        ResolveError::Conflict {
            package,
            selected,
            requirements,
        } => format!(
            "conflict|package={}|selected={}|constraints={}",
            encode_text(package),
            version_text(*selected),
            canonical_error_constraints(requirements)
        ),
    }
}

fn invalid_name(site: String, name: &str) -> Option<ResolveError> {
    if valid_name(name) {
        None
    } else {
        Some(ResolveError::InvalidName {
            site,
            name: name.to_owned(),
        })
    }
}

pub fn resolve(catalog: &[Package], roots: &[Requirement]) -> Result<Vec<Selection>, ResolveError> {
    for (i, root) in roots.iter().enumerate() {
        if let Some(error) = invalid_name(format!("root[{i}]"), &root.package) {
            return Err(error);
        }
    }

    for (i, package) in catalog.iter().enumerate() {
        if let Some(error) = invalid_name(format!("catalog[{i}]"), &package.name) {
            return Err(error);
        }
        for (j, release) in package.releases.iter().enumerate() {
            for (k, dependency) in release.deps.iter().enumerate() {
                if let Some(error) = invalid_name(
                    format!("catalog[{i}].release[{j}].dep[{k}]"),
                    &dependency.package,
                ) {
                    return Err(error);
                }
            }
        }
    }

    let mut package_indices = BTreeMap::<String, usize>::new();
    for (i, package) in catalog.iter().enumerate() {
        if package_indices
            .insert(package.name.clone(), i)
            .is_some()
        {
            return Err(ResolveError::DuplicatePackage {
                package: package.name.clone(),
            });
        }
    }

    for package in catalog {
        let mut seen = BTreeSet::new();
        for release in &package.releases {
            if !seen.insert(release.version) {
                return Err(ResolveError::DuplicateVersion {
                    package: package.name.clone(),
                    version: release.version,
                });
            }
        }
    }

    let mut active = BTreeMap::<String, Vec<Constraint>>::new();
    for root in roots {
        active
            .entry(root.package.clone())
            .or_default()
            .push(root.constraint);
    }

    let mut selected = BTreeMap::<String, Version>::new();
    let mut missing = BTreeSet::<String>::new();
    let mut conflicts = BTreeSet::<String>::new();
    let mut no_matches = BTreeSet::<String>::new();

    fn refresh_status(
        name: &str,
        active: &BTreeMap<String, Vec<Constraint>>,
        package_indices: &BTreeMap<String, usize>,
        catalog: &[Package],
        selected: &BTreeMap<String, Version>,
        missing: &mut BTreeSet<String>,
        conflicts: &mut BTreeSet<String>,
        no_matches: &mut BTreeSet<String>,
    ) {
        missing.remove(name);
        conflicts.remove(name);
        no_matches.remove(name);

        let Some(constraints) = active.get(name) else {
            return;
        };

        let Some(&index) = package_indices.get(name) else {
            missing.insert(name.to_owned());
            return;
        };

        if let Some(&version) = selected.get(name) {
            if !satisfies(version, constraints) {
                conflicts.insert(name.to_owned());
            }
            return;
        }

        if !catalog[index]
            .releases
            .iter()
            .any(|release| satisfies(release.version, constraints))
        {
            no_matches.insert(name.to_owned());
        }
    }

    for name in active.keys() {
        refresh_status(
            name,
            &active,
            &package_indices,
            catalog,
            &selected,
            &mut missing,
            &mut conflicts,
            &mut no_matches,
        );
    }

    loop {
        if let Some(name) = missing.iter().next() {
            return Err(ResolveError::Missing {
                package: name.clone(),
            });
        }

        if let Some(name) = conflicts.iter().next() {
            return Err(ResolveError::Conflict {
                package: name.clone(),
                selected: selected[name],
                requirements: canonicalize(&active[name]),
            });
        }

        if let Some(name) = no_matches.iter().next() {
            return Err(ResolveError::NoMatch {
                package: name.clone(),
                requirements: canonicalize(&active[name]),
            });
        }

        let Some(name) = active.keys().find(|name| !selected.contains_key(*name)).cloned() else {
            break;
        };

        let index = package_indices[&name];
        let constraints = &active[&name];
        let best = catalog[index]
            .releases
            .iter()
            .filter(|release| satisfies(release.version, constraints))
            .map(|release| release.version)
            .max();

        let Some(version) = best else {
            return Err(ResolveError::NoMatch {
                package: name,
                requirements: canonicalize(constraints),
            });
        };

        selected.insert(name.clone(), version);
        refresh_status(
            &name,
            &active,
            &package_indices,
            catalog,
            &selected,
            &mut missing,
            &mut conflicts,
            &mut no_matches,
        );

        let release = catalog[index]
            .releases
            .iter()
            .find(|release| release.version == version)
            .expect("selected release must exist");

        for dependency in &release.deps {
            active
                .entry(dependency.package.clone())
                .or_default()
                .push(dependency.constraint);
            refresh_status(
                &dependency.package,
                &active,
                &package_indices,
                catalog,
                &selected,
                &mut missing,
                &mut conflicts,
                &mut no_matches,
            );
        }
    }

    Ok(selected
        .into_iter()
        .map(|(package, version)| Selection { package, version })
        .collect())
}