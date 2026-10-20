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
    (1..=24).contains(&bytes.len())
        && bytes[0].is_ascii_lowercase()
        && bytes[bytes.len() - 1] != b'-'
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

fn matching(version: Version, constraints: &[Constraint]) -> bool {
    constraints.iter().all(|constraint| match *constraint {
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
    })
}

fn constraint_key(c: &Constraint) -> (u8, Version) {
    match *c {
        Constraint::Exact(v) => (0, v),
        Constraint::AtLeast(v) => (1, v),
        Constraint::Before(v) => (2, v),
        Constraint::Caret(v) => (3, v),
    }
}

fn canonical(mut constraints: Vec<Constraint>) -> Vec<Constraint> {
    constraints.sort_by_key(constraint_key);
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

fn constraints_text(constraints: &[Constraint]) -> String {
    canonical(constraints.to_vec())
        .into_iter()
        .map(constraint_text)
        .collect::<Vec<_>>()
        .join(",")
}

fn encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
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

fn active_constraints(
    roots: &[Requirement],
    catalog: &[Package],
    assigned: &[(String, Version)],
) -> std::collections::BTreeMap<String, Vec<Constraint>> {
    let mut active = std::collections::BTreeMap::<String, Vec<Constraint>>::new();
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

struct Search<'a> {
    catalog: &'a [Package],
    roots: &'a [Requirement],
    locks: &'a [Lock],
    max_attempts: usize,
    attempts: usize,
    best: Option<(usize, Vec<(String, Version)>)>,
    first_error: Option<ResolveError>,
}

impl Search<'_> {
    fn record_error(&mut self, error: ResolveError) {
        if self.first_error.is_none() {
            self.first_error = Some(error);
        }
    }

    fn lock_cost(&self, assigned: &[(String, Version)]) -> usize {
        self.locks
            .iter()
            .filter(|lock| {
                !assigned
                    .iter()
                    .any(|(name, version)| name == &lock.package && *version == lock.version)
            })
            .count()
    }

    fn better(&self, cost: usize, candidate: &[(String, Version)]) -> bool {
        let Some((best_cost, best)) = &self.best else {
            return true;
        };
        if cost != *best_cost {
            return cost < *best_cost;
        }
        let names: std::collections::BTreeSet<&str> = self
            .catalog
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        for name in names {
            let a = candidate
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, v)| *v);
            let b = best.iter().find(|(n, _)| n == name).map(|(_, v)| *v);
            match a.cmp(&b) {
                std::cmp::Ordering::Equal => {}
                ordering => return ordering == std::cmp::Ordering::Greater,
            }
        }
        false
    }

    fn visit(&mut self, assigned: &mut Vec<(String, Version)>) -> Result<(), ResolveError> {
        let active = active_constraints(self.roots, self.catalog, assigned);

        let missing = active
            .keys()
            .filter(|name| !self.catalog.iter().any(|p| &p.name == *name))
            .next()
            .cloned();
        if let Some(package) = missing {
            self.record_error(ResolveError::Missing { package });
            return Ok(());
        }

        for (name, version) in assigned.iter() {
            if let Some(constraints) = active.get(name) {
                if !matching(*version, constraints) {
                    self.record_error(ResolveError::Conflict {
                        package: name.clone(),
                        selected: *version,
                        requirements: constraints.clone(),
                    });
                    return Ok(());
                }
            }
        }

        for (name, constraints) in &active {
            if assigned.iter().any(|(n, _)| n == name) {
                continue;
            }
            let package = self.catalog.iter().find(|p| p.name == *name).unwrap();
            if !package
                .releases
                .iter()
                .any(|release| matching(release.version, constraints))
            {
                self.record_error(ResolveError::NoMatch {
                    package: name.clone(),
                    requirements: constraints.clone(),
                });
                return Ok(());
            }
        }

        let next = active
            .keys()
            .find(|name| !assigned.iter().any(|(n, _)| n == *name))
            .cloned();
        let Some(name) = next else {
            let cost = self.lock_cost(assigned);
            if self.better(cost, assigned) {
                let mut solution = assigned.clone();
                solution.sort_by(|a, b| a.0.cmp(&b.0));
                self.best = Some((cost, solution));
            }
            return Ok(());
        };

        let constraints = active.get(&name).unwrap();
        let package = self.catalog.iter().find(|p| p.name == name).unwrap();
        let mut candidates: Vec<Version> = package
            .releases
            .iter()
            .filter(|release| matching(release.version, constraints))
            .map(|release| release.version)
            .collect();
        candidates.sort_by(|a, b| b.cmp(a));

        if let Some(lock) = self.locks.iter().find(|lock| lock.package == name) {
            if let Some(index) = candidates.iter().position(|v| *v == lock.version) {
                candidates.remove(index);
                candidates.insert(0, lock.version);
            }
        }

        for version in candidates {
            if self.attempts == self.max_attempts {
                return Err(ResolveError::Limit {
                    attempts: self.max_attempts,
                });
            }
            self.attempts += 1;
            assigned.push((name.clone(), version));
            let result = self.visit(assigned);
            assigned.pop();
            result?;
        }
        Ok(())
    }
}

pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
    locks: &[Lock],
    max_attempts: usize,
) -> Result<Resolution, ResolveError> {
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

    let mut package_names = std::collections::BTreeSet::new();
    for package in catalog {
        if !package_names.insert(package.name.as_str()) {
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
    let mut lock_names = std::collections::BTreeSet::new();
    for lock in locks {
        if !lock_names.insert(lock.package.as_str()) {
            return Err(ResolveError::DuplicateLock {
                package: lock.package.clone(),
            });
        }
    }

    let mut search = Search {
        catalog,
        roots,
        locks,
        max_attempts,
        attempts: 0,
        best: None,
        first_error: None,
    };
    search.visit(&mut Vec::new())?;
    let Some((_, mut solution)) = search.best else {
        return Err(search.first_error.unwrap_or(ResolveError::NoMatch {
            package: String::new(),
            requirements: Vec::new(),
        }));
    };
    solution.sort_by(|a, b| a.0.cmp(&b.0));

    let selected: Vec<Selection> = solution
        .iter()
        .map(|(package, version)| Selection {
            package: package.clone(),
            version: *version,
        })
        .collect();

    let final_active = active_constraints(roots, catalog, &solution);
    let selected_map: std::collections::BTreeMap<&str, Version> = solution
        .iter()
        .map(|(name, version)| (name.as_str(), *version))
        .collect();
    let lock_map: std::collections::BTreeMap<&str, Version> = locks
        .iter()
        .map(|lock| (lock.package.as_str(), lock.version))
        .collect();

    let all_names: std::collections::BTreeSet<&str> =
        lock_map.keys().chain(selected_map.keys()).copied().collect();
    let mut changes = Vec::new();
    for name in all_names {
        let before = lock_map.get(name).copied();
        let after = selected_map.get(name).copied();
        if before == after {
            continue;
        }
        let reason = match (before, after) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old), Some(_)) => {
                let exists = catalog
                    .iter()
                    .find(|package| package.name == name)
                    .is_some_and(|package| {
                        package.releases.iter().any(|release| release.version == old)
                    });
                if !exists {
                    ChangeReason::Unavailable
                } else if final_active
                    .get(name)
                    .is_some_and(|constraints| !matching(old, constraints))
                {
                    ChangeReason::Rejected
                } else {
                    ChangeReason::Rebalanced
                }
            }
            (None, None) => continue,
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