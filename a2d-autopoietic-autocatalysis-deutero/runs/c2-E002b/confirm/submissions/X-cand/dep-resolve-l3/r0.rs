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
    InvalidName { site: String, name: String },
    DuplicatePackage { package: String },
    DuplicateVersion { package: String, version: Version },
    DuplicateLock { package: String },
    Missing { package: String },
    NoMatch { package: String, requirements: Vec<Constraint> },
    Conflict { package: String, selected: Version, requirements: Vec<Constraint> },
    Limit { attempts: usize },
}

fn valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() >= 1
        && bytes.len() <= 24
        && bytes[0].is_ascii_lowercase()
        && bytes[bytes.len() - 1] != b'-'
        && bytes
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

fn canonical(mut constraints: Vec<Constraint>) -> Vec<Constraint> {
    constraints.sort();
    constraints.dedup();
    constraints
}

fn constraints_text(constraints: &[Constraint]) -> String {
    canonical(constraints.to_vec())
        .iter()
        .map(|c| constraint_text(*c))
        .collect::<Vec<_>>()
        .join(",")
}

fn encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(*b, b'.' | b'_' | b'-') {
            out.push(*b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
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
        ResolveError::Missing { package } => format!("missing|package={}", encode(package)),
        ResolveError::NoMatch { package, requirements } => format!(
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

fn matches_all(version: Version, constraints: &[Constraint]) -> bool {
    constraints
        .iter()
        .all(|c| matches_constraint(version, *c))
}

struct Search<'a> {
    catalog: &'a [Package],
    package_indices: std::collections::BTreeMap<String, usize>,
    locks: &'a [Lock],
    max_attempts: usize,
    attempts: usize,
    best: Option<(usize, Vec<Selection>, std::collections::BTreeMap<String, Vec<Constraint>>)>,
    first_error: Option<ResolveError>,
}

impl<'a> Search<'a> {
    fn active_constraints(
        &self,
        roots: &[Requirement],
        assigned: &std::collections::BTreeMap<String, Version>,
    ) -> std::collections::BTreeMap<String, Vec<Constraint>> {
        let mut active = std::collections::BTreeMap::<String, Vec<Constraint>>::new();
        for root in roots {
            active
                .entry(root.package.clone())
                .or_default()
                .push(root.constraint);
        }
        for (name, version) in assigned {
            if let Some(&index) = self.package_indices.get(name) {
                if let Some(release) = self.catalog[index]
                    .releases
                    .iter()
                    .find(|r| r.version == *version)
                {
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

    fn record_leaf_error(&mut self, error: ResolveError) {
        if self.first_error.is_none() {
            self.first_error = Some(error);
        }
    }

    fn search(
        &mut self,
        roots: &[Requirement],
        assigned: &mut std::collections::BTreeMap<String, Version>,
    ) -> Result<(), ResolveError> {
        let active = self.active_constraints(roots, assigned);

        if let Some(name) = active
            .keys()
            .filter(|name| !self.package_indices.contains_key(*name))
            .next()
            .cloned()
        {
            self.record_leaf_error(ResolveError::Missing { package: name });
            return Ok(());
        }

        for (name, version) in assigned.iter() {
            if let Some(constraints) = active.get(name) {
                if !matches_all(*version, constraints) {
                    self.record_leaf_error(ResolveError::Conflict {
                        package: name.clone(),
                        selected: *version,
                        requirements: constraints.clone(),
                    });
                    return Ok(());
                }
            }
        }

        for (name, constraints) in &active {
            if assigned.contains_key(name) {
                continue;
            }
            let index = self.package_indices[name];
            if !self.catalog[index]
                .releases
                .iter()
                .any(|r| matches_all(r.version, constraints))
            {
                self.record_leaf_error(ResolveError::NoMatch {
                    package: name.clone(),
                    requirements: constraints.clone(),
                });
                return Ok(());
            }
        }

        let next = active.keys().find(|name| !assigned.contains_key(*name)).cloned();
        if let Some(name) = next {
            let index = self.package_indices[&name];
            let constraints = &active[&name];
            let mut candidates: Vec<Version> = self.catalog[index]
                .releases
                .iter()
                .map(|r| r.version)
                .filter(|v| matches_all(*v, constraints))
                .collect();
            candidates.sort_by(|a, b| b.cmp(a));
            candidates.dedup();

            if let Some(lock) = self.locks.iter().find(|l| l.package == name) {
                if let Some(pos) = candidates.iter().position(|v| *v == lock.version) {
                    let value = candidates.remove(pos);
                    candidates.insert(0, value);
                }
            }

            for candidate in candidates {
                if self.attempts == self.max_attempts {
                    return Err(ResolveError::Limit {
                        attempts: self.max_attempts,
                    });
                }
                self.attempts += 1;
                assigned.insert(name.clone(), candidate);
                self.search(roots, assigned)?;
                assigned.remove(&name);
            }
            return Ok(());
        }

        let selected: Vec<Selection> = assigned
            .iter()
            .map(|(package, version)| Selection {
                package: package.clone(),
                version: *version,
            })
            .collect();
        let cost = self
            .locks
            .iter()
            .filter(|lock| {
                assigned.get(&lock.package).copied() != Some(lock.version)
            })
            .count();

        let replace = match &self.best {
            None => true,
            Some((best_cost, best_selected, _)) => {
                if cost != *best_cost {
                    cost < *best_cost
                } else {
                    let names: std::collections::BTreeSet<String> = self
                        .catalog
                        .iter()
                        .map(|p| p.name.clone())
                        .collect();
                    let old: std::collections::BTreeMap<&str, Version> = best_selected
                        .iter()
                        .map(|s| (s.package.as_str(), s.version))
                        .collect();
                    let new: std::collections::BTreeMap<&str, Version> = selected
                        .iter()
                        .map(|s| (s.package.as_str(), s.version))
                        .collect();
                    let mut ordering = std::cmp::Ordering::Equal;
                    for name in names {
                        ordering = match (new.get(name.as_str()), old.get(name.as_str())) {
                            (None, None) => std::cmp::Ordering::Equal,
                            (None, Some(_)) => std::cmp::Ordering::Less,
                            (Some(_), None) => std::cmp::Ordering::Greater,
                            (Some(a), Some(b)) => a.cmp(b),
                        };
                        if ordering != std::cmp::Ordering::Equal {
                            break;
                        }
                    }
                    ordering == std::cmp::Ordering::Greater
                }
            }
        };
        if replace {
            self.best = Some((cost, selected, active));
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
        if !seen_packages.insert(package.name.clone()) {
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

    for (i, lock) in locks.iter().enumerate() {
        if !valid_name(&lock.package) {
            return Err(ResolveError::InvalidName {
                site: format!("lock[{}]", i),
                name: lock.package.clone(),
            });
        }
    }
    let mut seen_locks = std::collections::BTreeSet::new();
    for lock in locks {
        if !seen_locks.insert(lock.package.clone()) {
            return Err(ResolveError::DuplicateLock {
                package: lock.package.clone(),
            });
        }
    }

    let package_indices = catalog
        .iter()
        .enumerate()
        .map(|(i, p)| (p.name.clone(), i))
        .collect();

    let mut search = Search {
        catalog,
        package_indices,
        locks,
        max_attempts,
        attempts: 0,
        best: None,
        first_error: None,
    };
    let mut assigned = std::collections::BTreeMap::new();
    search.search(roots, &mut assigned)?;

    let (_, selected, active) = match search.best {
        Some(best) => best,
        None => return Err(search.first_error.unwrap_or_else(|| ResolveError::Missing {
            package: String::new(),
        })),
    };

    let selected_map: std::collections::BTreeMap<String, Version> = selected
        .iter()
        .map(|s| (s.package.clone(), s.version))
        .collect();
    let lock_map: std::collections::BTreeMap<String, Version> = locks
        .iter()
        .map(|l| (l.package.clone(), l.version))
        .collect();
    let names: std::collections::BTreeSet<String> = lock_map
        .keys()
        .chain(selected_map.keys())
        .cloned()
        .collect();

    let mut changes = Vec::new();
    for name in names {
        let before = lock_map.get(&name).copied();
        let after = selected_map.get(&name).copied();
        if before == after {
            continue;
        }
        let reason = match (before, after) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old), Some(_)) => {
                let exists = search.package_indices.get(&name).is_some_and(|index| {
                    catalog[*index].releases.iter().any(|r| r.version == old)
                });
                if !exists {
                    ChangeReason::Unavailable
                } else if active
                    .get(&name)
                    .is_some_and(|constraints| !matches_all(old, constraints))
                {
                    ChangeReason::Rejected
                } else {
                    ChangeReason::Rebalanced
                }
            }
            (None, None) => continue,
        };
        changes.push(Change {
            package: name,
            before,
            after,
            reason,
        });
    }

    Ok(Resolution { selected, changes })
}