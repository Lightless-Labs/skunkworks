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
pub struct Lock {
    pub package: String,
    pub version: Version,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangeReason {
    Added,
    Removed,
    Unavailable,
    Rejected,
    Rebalanced,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub package: String,
    pub before: Option<Version>,
    pub after: Option<Version>,
    pub reason: ChangeReason,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolution {
    pub selected: Vec<Selection>,
    pub changes: Vec<Change>,
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
    DuplicateLock {
        package: String,
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

fn canonical_constraints(mut constraints: Vec<Constraint>) -> Vec<Constraint> {
    constraints.sort();
    constraints.dedup();
    constraints
}

fn encode_text(s: &str) -> String {
    let mut out = String::new();
    for byte in s.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'.' | b'_' | b'-') {
            out.push(*byte as char);
        } else {
            out.push('%');
            out.push_str(&format!("{:02X}", byte));
        }
    }
    out
}

pub fn error_text(error: &ResolveError) -> String {
    match error {
        ResolveError::InvalidName { site, name } => {
            format!(
                "invalid-name|site={}|name={}",
                encode_text(site),
                encode_text(name)
            )
        }
        ResolveError::DuplicatePackage { package } => {
            format!("duplicate-package|package={}", encode_text(package))
        }
        ResolveError::DuplicateVersion { package, version } => format!(
            "duplicate-version|package={}|version={}",
            encode_text(package),
            version_text(*version)
        ),
        ResolveError::DuplicateLock { package } => {
            format!("duplicate-lock|package={}", encode_text(package))
        }
        ResolveError::Missing { package } => {
            format!("missing|package={}", encode_text(package))
        }
        ResolveError::NoMatch {
            package,
            requirements,
        } => {
            let constraints = canonical_constraints(requirements.clone())
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
            let constraints = canonical_constraints(requirements.clone())
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
        ResolveError::Limit { attempts } => format!("limit|attempts={}", attempts),
    }
}

fn matches_constraint(version: Version, constraint: Constraint) -> bool {
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

fn satisfies(version: Version, constraints: &[Constraint]) -> bool {
    constraints
        .iter()
        .all(|c| matches_constraint(version, *c))
}

fn validate_names(
    catalog: &[Package],
    roots: &[Requirement],
    locks: &[Lock],
) -> Result<(), ResolveError> {
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
    for (i, lock) in locks.iter().enumerate() {
        if !valid_name(&lock.package) {
            return Err(ResolveError::InvalidName {
                site: format!("lock[{}]", i),
                name: lock.package.clone(),
            });
        }
    }
    Ok(())
}

fn find_package<'a>(catalog: &'a [Package], name: &str) -> Option<&'a Package> {
    catalog.iter().find(|p| p.name == name)
}

type Active = std::collections::BTreeMap<String, Vec<Constraint>>;
type Assigned = std::collections::BTreeMap<String, Version>;

fn active_constraints(
    roots: &[Requirement],
    catalog: &[Package],
    assigned: &Assigned,
) -> Active {
    let mut active = Active::new();
    for root in roots {
        active
            .entry(root.package.clone())
            .or_default()
            .push(root.constraint);
    }
    for (name, version) in assigned {
        if let Some(package) = find_package(catalog, name) {
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
        *constraints = canonical_constraints(std::mem::take(constraints));
    }
    active
}

struct Search<'a> {
    catalog: &'a [Package],
    roots: &'a [Requirement],
    locks: &'a [Lock],
    attempts: usize,
    max_attempts: usize,
    best: Option<(usize, Vec<Option<Version>>, Assigned, Active)>,
    first_error: Option<ResolveError>,
}

impl<'a> Search<'a> {
    fn record_error(&mut self, error: ResolveError) {
        if self.first_error.is_none() {
            self.first_error = Some(error);
        }
    }

    fn visit(&mut self, assigned: &mut Assigned) -> Result<(), ResolveError> {
        let active = active_constraints(self.roots, self.catalog, assigned);

        let missing = active
            .keys()
            .filter(|name| find_package(self.catalog, name).is_none())
            .min();
        if let Some(name) = missing {
            self.record_error(ResolveError::Missing {
                package: name.clone(),
            });
            return Ok(());
        }

        for (name, version) in assigned.iter() {
            if let Some(constraints) = active.get(name) {
                if !satisfies(*version, constraints) {
                    self.record_error(ResolveError::Conflict {
                        package: name.clone(),
                        selected: *version,
                        requirements: constraints.clone(),
                    });
                    return Ok(());
                }
            }
        }

        let mut branch: Option<(String, Vec<Version>)> = None;
        for name in active.keys() {
            if assigned.contains_key(name) {
                continue;
            }
            let package = find_package(self.catalog, name).unwrap();
            let mut candidates: Vec<Version> = package
                .releases
                .iter()
                .map(|r| r.version)
                .filter(|v| satisfies(*v, &active[name]))
                .collect();
            candidates.sort_by(|a, b| b.cmp(a));
            candidates.dedup();
            if candidates.is_empty() {
                self.record_error(ResolveError::NoMatch {
                    package: name.clone(),
                    requirements: active[name].clone(),
                });
                return Ok(());
            }
            if branch.is_none() {
                branch = Some((name.clone(), candidates));
            }
        }

        let Some((name, mut candidates)) = branch else {
            self.consider(assigned, active);
            return Ok(());
        };

        if let Some(lock) = self.locks.iter().find(|l| l.package == name) {
            if let Some(pos) = candidates.iter().position(|v| *v == lock.version) {
                let locked_version = candidates.remove(pos);
                candidates.insert(0, locked_version);
            }
        }

        let mut had_child_error = false;
        for candidate in candidates {
            if self.attempts == self.max_attempts {
                return Err(ResolveError::Limit {
                    attempts: self.max_attempts,
                });
            }
            self.attempts += 1;
            assigned.insert(name.clone(), candidate);
            self.visit(assigned)?;
            assigned.remove(&name);
            had_child_error = true;
        }
        if !had_child_error {
            self.record_error(ResolveError::NoMatch {
                package: name,
                requirements: active.get(&name).cloned().unwrap_or_default(),
            });
        }
        Ok(())
    }

    fn consider(&mut self, assigned: &Assigned, active: Active) {
        let cost = self
            .locks
            .iter()
            .filter(|lock| assigned.get(&lock.package) != Some(&lock.version))
            .count();

        let mut catalog_names: Vec<&str> = self.catalog.iter().map(|p| p.name.as_str()).collect();
        catalog_names.sort_unstable();
        let vector: Vec<Option<Version>> = catalog_names
            .iter()
            .map(|name| assigned.get(*name).copied())
            .collect();

        let replace = match &self.best {
            None => true,
            Some((best_cost, best_vector, _, _)) => {
                cost < *best_cost || (cost == *best_cost && vector > *best_vector)
            }
        };
        if replace {
            self.best = Some((cost, vector, assigned.clone(), active));
        }
    }
}

pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
    locks: &[Lock],
    max_attempts: usize,
) -> Result<Resolution, ResolveError> {
    validate_names(catalog, roots, locks)?;

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

    let mut seen_locks = std::collections::BTreeSet::new();
    for lock in locks {
        if !seen_locks.insert(lock.package.as_str()) {
            return Err(ResolveError::DuplicateLock {
                package: lock.package.clone(),
            });
        }
    }

    let mut search = Search {
        catalog,
        roots,
        locks,
        attempts: 0,
        max_attempts,
        best: None,
        first_error: None,
    };
    search.visit(&mut Assigned::new())?;

    let Some((_, _, selected, final_active)) = search.best else {
        return Err(search.first_error.unwrap_or(ResolveError::NoMatch {
            package: String::new(),
            requirements: Vec::new(),
        }));
    };

    let mut selected_vec: Vec<Selection> = selected
        .iter()
        .map(|(package, version)| Selection {
            package: package.clone(),
            version: *version,
        })
        .collect();
    selected_vec.sort_by(|a, b| a.package.cmp(&b.package));

    let before: std::collections::BTreeMap<String, Version> = locks
        .iter()
        .map(|lock| (lock.package.clone(), lock.version))
        .collect();
    let after: std::collections::BTreeMap<String, Version> = selected.clone();
    let names: std::collections::BTreeSet<String> =
        before.keys().chain(after.keys()).cloned().collect();

    let mut changes = Vec::new();
    for package in names {
        let b = before.get(&package).copied();
        let a = after.get(&package).copied();
        if b == a {
            continue;
        }
        let reason = match (b, a) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old), Some(_)) => {
                let exists = find_package(catalog, &package)
                    .map(|p| p.releases.iter().any(|r| r.version == old))
                    .unwrap_or(false);
                if !exists {
                    ChangeReason::Unavailable
                } else if !final_active
                    .get(&package)
                    .map(|constraints| satisfies(old, constraints))
                    .unwrap_or(true)
                {
                    ChangeReason::Rejected
                } else {
                    ChangeReason::Rebalanced
                }
            }
            (None, None) => unreachable!(),
        };
        changes.push(Change {
            package,
            before: b,
            after: a,
            reason,
        });
    }

    Ok(Resolution {
        selected: selected_vec,
        changes,
    })
}