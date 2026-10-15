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
    if bytes[bytes.len() - 1] == b'-' {
        return false;
    }
    bytes
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

fn validate_input(catalog: &[Package], roots: &[Requirement]) -> Result<(), ResolveError> {
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

fn canonicalize(mut constraints: Vec<Constraint>) -> Vec<Constraint> {
    constraints.sort();
    constraints.dedup();
    constraints
}

fn matches(version: Version, constraints: &[Constraint]) -> bool {
    constraints.iter().all(|constraint| match *constraint {
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
    })
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
    canonicalize(constraints.to_vec())
        .iter()
        .map(|c| constraint_text(*c))
        .collect::<Vec<_>>()
        .join(",")
}

fn encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'.' | b'_' | b'-') {
            out.push(*byte as char);
        } else {
            out.push('%');
            out.push_str(&format!("{byte:02X}"));
        }
    }
    out
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

type Active = std::collections::BTreeMap<String, Vec<Constraint>>;
type Assigned = std::collections::BTreeMap<String, Version>;

struct Resolver<'a> {
    packages: std::collections::BTreeMap<&'a str, &'a Package>,
    max_attempts: usize,
    attempts: usize,
}

impl Resolver<'_> {
    fn inspect(
        &self,
        roots: &Active,
        assigned: &Assigned,
    ) -> Result<(Active, Option<String>), ResolveError> {
        let mut active = roots.clone();

        for (name, version) in assigned {
            if let Some(package) = self.packages.get(name.as_str()) {
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

        for name in active.keys() {
            if !self.packages.contains_key(name.as_str()) {
                return Err(ResolveError::Missing {
                    package: name.clone(),
                });
            }
        }

        for (name, version) in assigned {
            if let Some(constraints) = active.get(name) {
                if !matches(*version, constraints) {
                    return Err(ResolveError::Conflict {
                        package: name.clone(),
                        selected: *version,
                        requirements: constraints.clone(),
                    });
                }
            }
        }

        for (name, constraints) in &active {
            if assigned.contains_key(name) {
                continue;
            }
            let package = self.packages[name.as_str()];
            if !package
                .releases
                .iter()
                .any(|release| matches(release.version, constraints))
            {
                return Err(ResolveError::NoMatch {
                    package: name.clone(),
                    requirements: constraints.clone(),
                });
            }
        }

        let next = active.keys().find(|name| !assigned.contains_key(*name)).cloned();
        Ok((active, next))
    }

    fn search(&mut self, roots: &Active, assigned: &mut Assigned) -> Result<(), ResolveError> {
        let (active, next) = self.inspect(roots, assigned)?;
        let Some(package_name) = next else {
            return Ok(());
        };

        let package = self.packages[package_name.as_str()];
        let constraints = &active[&package_name];
        let mut candidates: Vec<Version> = package
            .releases
            .iter()
            .map(|release| release.version)
            .filter(|version| matches(*version, constraints))
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
            assigned.insert(package_name.clone(), version);
            let result = self.search(roots, assigned);
            assigned.remove(&package_name);

            match result {
                Ok(()) => {
                    assigned.insert(package_name, version);
                    return Ok(());
                }
                Err(error @ ResolveError::Limit { .. }) => return Err(error),
                Err(error) => {
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
}

pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
    max_attempts: usize,
) -> Result<Vec<Selection>, ResolveError> {
    validate_input(catalog, roots)?;

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

    let mut root_constraints = Active::new();
    for root in roots {
        root_constraints
            .entry(root.package.clone())
            .or_default()
            .push(root.constraint);
    }
    for constraints in root_constraints.values_mut() {
        *constraints = canonicalize(std::mem::take(constraints));
    }

    let packages = catalog
        .iter()
        .map(|package| (package.name.as_str(), package))
        .collect();

    let mut resolver = Resolver {
        packages,
        max_attempts,
        attempts: 0,
    };
    let mut assigned = Assigned::new();
    resolver.search(&root_constraints, &mut assigned)?;

    Ok(assigned
        .into_iter()
        .map(|(package, version)| Selection { package, version })
        .collect())
}