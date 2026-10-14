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
    !bytes.is_empty()
        && bytes.len() <= 24
        && bytes[0].is_ascii_lowercase()
        && bytes[bytes.len() - 1] != b'-'
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

fn version_text(version: Version) -> String {
    format!("{}.{}.{}", version.major, version.minor, version.patch)
}

fn constraint_key(constraint: Constraint) -> (u8, Version) {
    match constraint {
        Constraint::Exact(v) => (0, v),
        Constraint::AtLeast(v) => (1, v),
        Constraint::Before(v) => (2, v),
        Constraint::Caret(v) => (3, v),
    }
}

fn canonicalize(mut constraints: Vec<Constraint>) -> Vec<Constraint> {
    constraints.sort_by_key(|constraint| constraint_key(*constraint));
    constraints.dedup();
    constraints
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
    canonicalize(constraints.to_vec())
        .iter()
        .map(|constraint| constraint_text(*constraint))
        .collect::<Vec<_>>()
        .join(",")
}

fn matches(version: Version, constraint: Constraint) -> bool {
    match constraint {
        Constraint::Exact(wanted) => version == wanted,
        Constraint::AtLeast(wanted) => version >= wanted,
        Constraint::Before(wanted) => version < wanted,
        Constraint::Caret(wanted) => {
            version >= wanted
                && if wanted.major > 0 {
                    version.major == wanted.major
                } else if wanted.minor > 0 {
                    version.major == 0 && version.minor == wanted.minor
                } else {
                    version == wanted
                }
        }
    }
}

fn matches_all(version: Version, constraints: &[Constraint]) -> bool {
    constraints
        .iter()
        .all(|constraint| matches(version, *constraint))
}

fn encode(text: &str) -> String {
    let mut encoded = String::new();
    const HEX: &[u8; 16] = b"0123456789ABCDEF";

    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-') {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push(HEX[(byte >> 4) as usize] as char);
            encoded.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    encoded
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
        ResolveError::Limit { attempts } => format!("limit|attempts={attempts}"),
    }
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

    let mut seen = std::collections::BTreeSet::new();
    for package in catalog {
        if !seen.insert(package.name.as_str()) {
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

    Ok(())
}

struct Search<'a> {
    catalog: &'a [Package],
    max_attempts: usize,
    attempts: usize,
}

impl Search<'_> {
    fn inspect(
        &mut self,
        roots: &[Requirement],
        selected: &mut std::collections::BTreeMap<String, Version>,
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
            if let Some(package) = self.catalog.iter().find(|p| p.name == *name) {
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

        if let Some(package) = active
            .keys()
            .find(|name| !self.catalog.iter().any(|entry| entry.name == ***name))
        {
            return Err(ResolveError::Missing {
                package: package.clone(),
            });
        }

        for (package, version) in selected.iter() {
            if let Some(constraints) = active.get(package) {
                if !matches_all(*version, constraints) {
                    return Err(ResolveError::Conflict {
                        package: package.clone(),
                        selected: *version,
                        requirements: constraints.clone(),
                    });
                }
            }
        }

        for (package, constraints) in &active {
            if selected.contains_key(package) {
                continue;
            }
            let releases = &self
                .catalog
                .iter()
                .find(|entry| entry.name == *package)
                .unwrap()
                .releases;
            if !releases
                .iter()
                .any(|release| matches_all(release.version, constraints))
            {
                return Err(ResolveError::NoMatch {
                    package: package.clone(),
                    requirements: constraints.clone(),
                });
            }
        }

        let Some(package) = active.keys().find(|name| !selected.contains_key(*name)) else {
            return Ok(selected
                .iter()
                .map(|(package, version)| Selection {
                    package: package.clone(),
                    version: *version,
                })
                .collect());
        };

        let constraints = &active[package];
        let mut candidates: Vec<Version> = self
            .catalog
            .iter()
            .find(|entry| entry.name == *package)
            .unwrap()
            .releases
            .iter()
            .filter(|release| matches_all(release.version, constraints))
            .map(|release| release.version)
            .collect();
        candidates.sort_by(|a, b| b.cmp(a));

        let mut first_error = None;
        for candidate in candidates {
            if self.attempts == self.max_attempts {
                return Err(ResolveError::Limit {
                    attempts: self.max_attempts,
                });
            }

            self.attempts += 1;
            selected.insert(package.clone(), candidate);
            let result = self.inspect(roots, selected);
            selected.remove(package);

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
            package: package.clone(),
            requirements: constraints.clone(),
        }))
    }
}

pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
    max_attempts: usize,
) -> Result<Vec<Selection>, ResolveError> {
    validate(catalog, roots)?;

    let mut search = Search {
        catalog,
        max_attempts,
        attempts: 0,
    };
    let mut selected = std::collections::BTreeMap::new();
    search.inspect(roots, &mut selected)
}