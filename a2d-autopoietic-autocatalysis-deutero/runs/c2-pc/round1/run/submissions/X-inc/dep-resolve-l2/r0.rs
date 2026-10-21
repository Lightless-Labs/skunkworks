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
    if !(1..=24).contains(&bytes.len()) || !bytes[0].is_ascii_lowercase() {
        return false;
    }
    if bytes[bytes.len() - 1] == b'-' {
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
        .iter()
        .map(|c| constraint_text(*c))
        .collect::<Vec<_>>()
        .join(",")
}

fn encode_text(text: &str) -> String {
    let mut encoded = String::new();
    for byte in text.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'.' | b'_' | b'-') {
            encoded.push(*byte as char);
        } else {
            encoded.push_str(&format!("%{:02X}", byte));
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

struct Search<'a> {
    catalog: &'a [Package],
    roots: &'a [Requirement],
    max_attempts: usize,
    attempts: usize,
}

impl<'a> Search<'a> {
    fn catalog_package(&self, name: &str) -> Option<&'a Package> {
        self.catalog.iter().find(|package| package.name == name)
    }

    fn active_constraints(
        &self,
        selected: &[(String, &'a Release)],
    ) -> BTreeMap<String, Vec<Constraint>> {
        let mut active = BTreeMap::new();
        for root in self.roots {
            active
                .entry(root.package.clone())
                .or_insert_with(Vec::new)
                .push(root.constraint);
        }
        for (_, release) in selected {
            for dep in &release.deps {
                active
                    .entry(dep.package.clone())
                    .or_insert_with(Vec::new)
                    .push(dep.constraint);
            }
        }
        for constraints in active.values_mut() {
            *constraints = canonical_constraints(std::mem::take(constraints));
        }
        active
    }

    fn inspect(
        &mut self,
        selected: &mut Vec<(String, &'a Release)>,
    ) -> Result<Vec<Selection>, ResolveError> {
        let active = self.active_constraints(selected);
        let selected_map: BTreeMap<&str, &'a Release> = selected
            .iter()
            .map(|(name, release)| (name.as_str(), *release))
            .collect();

        if let Some(name) = active
            .keys()
            .find(|name| self.catalog_package(name).is_none())
        {
            return Err(ResolveError::Missing {
                package: name.clone(),
            });
        }

        for (name, constraints) in &active {
            if let Some(release) = selected_map.get(name.as_str()) {
                if !constraints.iter().all(|c| matches(release.version, *c)) {
                    return Err(ResolveError::Conflict {
                        package: name.clone(),
                        selected: release.version,
                        requirements: constraints.clone(),
                    });
                }
            }
        }

        for (name, constraints) in &active {
            if selected_map.contains_key(name.as_str()) {
                continue;
            }
            let package = self.catalog_package(name).expect("missing checked above");
            if !package
                .releases
                .iter()
                .any(|release| constraints.iter().all(|c| matches(release.version, *c)))
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
            let mut result = selected
                .iter()
                .map(|(package, release)| Selection {
                    package: package.clone(),
                    version: release.version,
                })
                .collect::<Vec<_>>();
            result.sort_by(|a, b| a.package.cmp(&b.package));
            return Ok(result);
        };

        let constraints = &active[name];
        let package = self.catalog_package(name).expect("missing checked above");
        let mut candidates: Vec<&'a Release> = package
            .releases
            .iter()
            .filter(|release| constraints.iter().all(|c| matches(release.version, *c)))
            .collect();
        candidates.sort_by(|a, b| b.version.cmp(&a.version));

        let mut first_error = None;
        for release in candidates {
            if self.attempts == self.max_attempts {
                return Err(ResolveError::Limit {
                    attempts: self.max_attempts,
                });
            }
            self.attempts += 1;
            selected.push((name.clone(), release));
            let outcome = self.inspect(selected);
            selected.pop();

            match outcome {
                Ok(solution) => return Ok(solution),
                Err(error @ ResolveError::Limit { .. }) => return Err(error),
                Err(error) => {
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
        }
        Err(first_error.expect("candidate list was nonempty"))
    }
}

use std::collections::BTreeMap;

pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
    max_attempts: usize,
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

    if roots.is_empty() {
        return Ok(Vec::new());
    }

    Search {
        catalog,
        roots,
        max_attempts,
        attempts: 0,
    }
    .inspect(&mut Vec::new())
}