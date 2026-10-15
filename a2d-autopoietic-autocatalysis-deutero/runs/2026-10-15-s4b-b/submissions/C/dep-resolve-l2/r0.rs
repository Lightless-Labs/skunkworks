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

fn version_text(v: Version) -> String {
    format!("{}.{}.{}", v.major, v.minor, v.patch)
}

fn constraint_text(c: Constraint) -> String {
    match c {
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

fn encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
        if byte.is_ascii_alphabetic()
            || byte.is_ascii_digit()
            || matches!(*byte, b'.' | b'_' | b'-')
        {
            out.push(*byte as char);
        } else {
            out.push('%');
            out.push(char::from_digit((byte >> 4) as u32, 16).unwrap().to_ascii_uppercase());
            out.push(char::from_digit((byte & 0x0f) as u32, 16).unwrap().to_ascii_uppercase());
        }
    }
    out
}

fn constraints_text(constraints: &[Constraint]) -> String {
    canonicalize(constraints.to_vec())
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
        ResolveError::Limit { attempts } => format!("limit|attempts={}", attempts),
    }
}

fn matches_constraint(version: Version, constraint: Constraint) -> bool {
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

fn satisfies(version: Version, constraints: &[Constraint]) -> bool {
    constraints
        .iter()
        .all(|constraint| matches_constraint(version, *constraint))
}

fn validate(catalog: &[Package], roots: &[Requirement]) -> Result<(), ResolveError> {
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
            for (k, dep) in release.deps.iter().enumerate() {
                if !valid_name(&dep.package) {
                    return Err(ResolveError::InvalidName {
                        site: format!("catalog[{}].release[{}].dep[{}]", i, j, k),
                        name: dep.package.clone(),
                    });
                }
            }
        }
    }

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

    Ok(())
}

struct Search<'a> {
    catalog: &'a [Package],
    roots: &'a [Requirement],
    max_attempts: usize,
    attempts: usize,
}

impl Search<'_> {
    fn active_constraints(
        &self,
        selected: &std::collections::BTreeMap<String, Version>,
    ) -> std::collections::BTreeMap<String, Vec<Constraint>> {
        let mut active = std::collections::BTreeMap::<String, Vec<Constraint>>::new();

        for root in self.roots {
            active
                .entry(root.package.clone())
                .or_default()
                .push(root.constraint);
        }

        for (name, version) in selected {
            if let Some(package) = self.catalog.iter().find(|p| p.name == *name) {
                if let Some(release) = package.releases.iter().find(|r| r.version == *version) {
                    for dep in &release.deps {
                        active
                            .entry(dep.package.clone())
                            .or_default()
                            .push(dep.constraint);
                    }
                }
            }
        }

        for constraints in active.values_mut() {
            *constraints = canonicalize(std::mem::take(constraints));
        }
        active
    }

    fn inspect(
        &self,
        selected: &std::collections::BTreeMap<String, Version>,
        active: &std::collections::BTreeMap<String, Vec<Constraint>>,
    ) -> Result<(), ResolveError> {
        for name in active.keys() {
            if !self.catalog.iter().any(|package| package.name == *name) {
                return Err(ResolveError::Missing {
                    package: name.clone(),
                });
            }
        }

        for (name, version) in selected {
            if let Some(constraints) = active.get(name) {
                if !satisfies(*version, constraints) {
                    return Err(ResolveError::Conflict {
                        package: name.clone(),
                        selected: *version,
                        requirements: constraints.clone(),
                    });
                }
            }
        }

        for (name, constraints) in active {
            if selected.contains_key(name) {
                continue;
            }
            let package = self
                .catalog
                .iter()
                .find(|package| package.name == *name)
                .unwrap();
            if !package
                .releases
                .iter()
                .any(|release| satisfies(release.version, constraints))
            {
                return Err(ResolveError::NoMatch {
                    package: name.clone(),
                    requirements: constraints.clone(),
                });
            }
        }
        Ok(())
    }

    fn recurse(
        &mut self,
        selected: &mut std::collections::BTreeMap<String, Version>,
    ) -> Result<Vec<Selection>, ResolveError> {
        let active = self.active_constraints(selected);
        self.inspect(selected, &active)?;

        let next = active.keys().find(|name| !selected.contains_key(*name)).cloned();
        let Some(name) = next else {
            return Ok(selected
                .iter()
                .map(|(package, version)| Selection {
                    package: package.clone(),
                    version: *version,
                })
                .collect());
        };

        let package = self
            .catalog
            .iter()
            .find(|package| package.name == name)
            .unwrap();
        let mut candidates: Vec<Version> = package
            .releases
            .iter()
            .filter(|release| satisfies(release.version, &active[&name]))
            .map(|release| release.version)
            .collect();
        candidates.sort_by(|a, b| b.cmp(a));

        let mut first_error = None;
        for version in candidates {
            if self.attempts == self.max_attempts {
                return Err(ResolveError::Limit {
                    attempts: self.max_attempts,
                });
            }
            self.attempts += 1;
            selected.insert(name.clone(), version);
            match self.recurse(selected) {
                Ok(solution) => return Ok(solution),
                Err(error @ ResolveError::Limit { .. }) => return Err(error),
                Err(error) => {
                    first_error.get_or_insert(error);
                    selected.remove(&name);
                }
            }
        }

        Err(first_error.unwrap_or_else(|| ResolveError::NoMatch {
            package: name.clone(),
            requirements: active.get(&name).cloned().unwrap_or_default(),
        }))
    }
}

pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
    max_attempts: usize,
) -> Result<Vec<Selection>, ResolveError> {
    validate(catalog, roots)?;
    if roots.is_empty() {
        return Ok(Vec::new());
    }

    let mut search = Search {
        catalog,
        roots,
        max_attempts,
        attempts: 0,
    };
    search.recurse(&mut std::collections::BTreeMap::new())
}

fn main() {}