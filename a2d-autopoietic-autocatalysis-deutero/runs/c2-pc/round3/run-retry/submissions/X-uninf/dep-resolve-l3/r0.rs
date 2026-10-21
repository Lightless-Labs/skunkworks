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

fn matches(version: Version, constraint: Constraint) -> bool {
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

fn canonical(mut constraints: Vec<Constraint>) -> Vec<Constraint> {
    constraints.sort();
    constraints.dedup();
    constraints
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

fn constraints_text(cs: &[Constraint]) -> String {
    canonical(cs.to_vec())
        .into_iter()
        .map(constraint_text)
        .collect::<Vec<_>>()
        .join(",")
}

fn encode(text: &str) -> String {
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
        ResolveError::DuplicateLock { package } => {
            format!("duplicate-lock|package={}", encode(package))
        }
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

fn validate_inputs(
    catalog: &[Package],
    roots: &[Requirement],
    locks: &[Lock],
) -> Result<(), ResolveError> {
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

    let mut seen_packages = std::collections::BTreeSet::new();
    for package in catalog {
        if !seen_packages.insert(package.name.as_str()) {
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

    for (i, lock) in locks.iter().enumerate() {
        if !valid_name(&lock.package) {
            return Err(ResolveError::InvalidName {
                site: format!("lock[{i}]"),
                name: lock.package.clone(),
            });
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
    Ok(())
}

type Active = std::collections::BTreeMap<String, Vec<Constraint>>;
type Assigned = std::collections::BTreeMap<String, Version>;

fn active_constraints(roots: &[Requirement], assigned: &Assigned, catalog: &[Package]) -> Active {
    let mut active = Active::new();
    for root in roots {
        active
            .entry(root.package.clone())
            .or_default()
            .push(root.constraint);
    }
    for (name, version) in assigned {
        if let Some(package) = catalog.iter().find(|p| p.name == *name) {
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
        *constraints = canonical(std::mem::take(constraints));
    }
    active
}

enum NodeResult {
    Error(ResolveError),
    Solutions(Vec<Assigned>),
}

struct Search<'a> {
    catalog: &'a [Package],
    roots: &'a [Requirement],
    locks: &'a [Lock],
    max_attempts: usize,
    attempts: usize,
}

impl Search<'_> {
    fn inspect(&mut self, assigned: &mut Assigned) -> NodeResult {
        let active = active_constraints(self.roots, assigned, self.catalog);

        if let Some(name) = active
            .keys()
            .filter(|name| !self.catalog.iter().any(|p| p.name == ***name))
            .min()
        {
            return NodeResult::Error(ResolveError::Missing {
                package: name.clone(),
            });
        }

        for (name, version) in assigned.iter() {
            if let Some(constraints) = active.get(name) {
                if !constraints.iter().all(|c| matches(*version, *c)) {
                    return NodeResult::Error(ResolveError::Conflict {
                        package: name.clone(),
                        selected: *version,
                        requirements: constraints.clone(),
                    });
                }
            }
        }

        for (name, constraints) in &active {
            if !assigned.contains_key(name) {
                let package = self.catalog.iter().find(|p| p.name == *name).unwrap();
                if !package
                    .releases
                    .iter()
                    .any(|r| constraints.iter().all(|c| matches(r.version, *c)))
                {
                    return NodeResult::Error(ResolveError::NoMatch {
                        package: name.clone(),
                        requirements: constraints.clone(),
                    });
                }
            }
        }

        let next = active.keys().find(|name| !assigned.contains_key(*name));
        let Some(name) = next else {
            return NodeResult::Solutions(vec![assigned.clone()]);
        };

        let constraints = &active[name];
        let package = self.catalog.iter().find(|p| p.name == *name).unwrap();
        let mut candidates: Vec<Version> = package
            .releases
            .iter()
            .map(|r| r.version)
            .filter(|v| constraints.iter().all(|c| matches(*v, *c)))
            .collect();
        candidates.sort_by(|a, b| b.cmp(a));

        if let Some(lock) = self.locks.iter().find(|l| l.package == *name) {
            if let Some(index) = candidates.iter().position(|v| *v == lock.version) {
                let locked = candidates.remove(index);
                candidates.insert(0, locked);
            }
        }

        let mut solutions = Vec::new();
        let mut first_error = None;
        for candidate in candidates {
            if self.attempts == self.max_attempts {
                return NodeResult::Error(ResolveError::Limit {
                    attempts: self.max_attempts,
                });
            }
            self.attempts += 1;
            assigned.insert(name.clone(), candidate);
            match self.inspect(assigned) {
                NodeResult::Error(ResolveError::Limit { attempts }) => {
                    assigned.remove(name);
                    return NodeResult::Error(ResolveError::Limit { attempts });
                }
                NodeResult::Error(error) => {
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
                NodeResult::Solutions(found) => solutions.extend(found),
            }
            assigned.remove(name);
        }

        if !solutions.is_empty() {
            NodeResult::Solutions(solutions)
        } else {
            NodeResult::Error(first_error.unwrap_or_else(|| ResolveError::NoMatch {
                package: name.clone(),
                requirements: constraints.clone(),
            }))
        }
    }
}

pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
    locks: &[Lock],
    max_attempts: usize,
) -> Result<Resolution, ResolveError> {
    validate_inputs(catalog, roots, locks)?;

    let mut search = Search {
        catalog,
        roots,
        locks,
        max_attempts,
        attempts: 0,
    };
    let mut assigned = Assigned::new();
    let solutions = match search.inspect(&mut assigned) {
        NodeResult::Error(error) => return Err(error),
        NodeResult::Solutions(solutions) => solutions,
    };

    let lock_map: std::collections::BTreeMap<&str, Version> = locks
        .iter()
        .map(|lock| (lock.package.as_str(), lock.version))
        .collect();

    let mut best: Option<Assigned> = None;
    let mut best_cost = usize::MAX;
    for solution in solutions {
        let cost = lock_map
            .iter()
            .filter(|(name, version)| solution.get(**name) != Some(*version))
            .count();
        let better = if cost < best_cost {
            true
        } else if cost == best_cost {
            match &best {
                None => true,
                Some(current) => {
                    let names: std::collections::BTreeSet<&str> = catalog
                        .iter()
                        .map(|p| p.name.as_str())
                        .collect();
                    names.into_iter().find_map(|name| {
                        let a = solution.get(name);
                        let b = current.get(name);
                        if a != b {
                            Some(match (a, b) {
                                (None, Some(_)) => false,
                                (Some(_), None) => true,
                                (Some(a), Some(b)) => a > b,
                                (None, None) => false,
                            })
                        } else {
                            None
                        }
                    }).unwrap_or(false)
                }
            }
        } else {
            false
        };
        if better {
            best_cost = cost;
            best = Some(solution);
        }
    }

    let best = best.unwrap_or_default();
    let selected: Vec<Selection> = best
        .iter()
        .map(|(package, version)| Selection {
            package: package.clone(),
            version: *version,
        })
        .collect();
    let final_active = active_constraints(roots, &best, catalog);

    let mut all_names = std::collections::BTreeSet::new();
    all_names.extend(lock_map.keys().copied());
    all_names.extend(best.keys().map(String::as_str));

    let mut changes = Vec::new();
    for name in all_names {
        let before = lock_map.get(name).copied();
        let after = best.get(name).copied();
        if before == after {
            continue;
        }
        let reason = match (before, after) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old), Some(_)) => {
                let exists = catalog
                    .iter()
                    .find(|p| p.name == name)
                    .map(|p| p.releases.iter().any(|r| r.version == old))
                    .unwrap_or(false);
                if !exists {
                    ChangeReason::Unavailable
                } else if final_active
                    .get(name)
                    .map(|cs| !cs.iter().all(|c| matches(old, *c)))
                    .unwrap_or(false)
                {
                    ChangeReason::Rejected
                } else {
                    ChangeReason::Rebalanced
                }
            }
            (None, None) => unreachable!(),
        };
        changes.push(Change {
            package: name.to_string(),
            before,
            after,
            reason,
        });
    }

    Ok(Resolution { selected, changes })
}