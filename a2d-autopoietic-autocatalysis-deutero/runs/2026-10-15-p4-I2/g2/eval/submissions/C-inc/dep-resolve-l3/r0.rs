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
    !bytes.is_empty()
        && bytes.len() <= 24
        && bytes[0].is_ascii_lowercase()
        && bytes.last() != Some(&b'-')
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

fn version_text(v: Version) -> String {
    format!("{}.{}.{}", v.major, v.minor, v.patch)
}

fn constraint_rank(c: Constraint) -> (u8, Version) {
    match c {
        Constraint::Exact(v) => (0, v),
        Constraint::AtLeast(v) => (1, v),
        Constraint::Before(v) => (2, v),
        Constraint::Caret(v) => (3, v),
    }
}

fn canonical(mut constraints: Vec<Constraint>) -> Vec<Constraint> {
    constraints.sort_by_key(|c| constraint_rank(*c));
    constraints.dedup();
    constraints
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
        .iter()
        .map(|c| constraint_text(*c))
        .collect::<Vec<_>>()
        .join(",")
}

fn matches(v: Version, c: Constraint) -> bool {
    match c {
        Constraint::Exact(x) => v == x,
        Constraint::AtLeast(x) => v >= x,
        Constraint::Before(x) => v < x,
        Constraint::Caret(x) => {
            v >= x
                && if x.major > 0 {
                    v.major == x.major
                } else if x.minor > 0 {
                    v.major == 0 && v.minor == x.minor
                } else {
                    v == x
                }
        }
    }
}

fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
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
        ResolveError::DuplicateLock { package } => {
            format!("duplicate-lock|package={}", percent_encode(package))
        }
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
        ResolveError::Limit { attempts } => format!("limit|attempts={}", attempts),
    }
}

#[derive(Clone)]
struct SearchResult {
    selected: Vec<(String, Version)>,
    constraints: std::collections::BTreeMap<String, Vec<Constraint>>,
}

struct Search<'a> {
    catalog: &'a [Package],
    package_indices: std::collections::BTreeMap<String, usize>,
    locks: std::collections::BTreeMap<String, Version>,
    attempts: usize,
    max_attempts: usize,
    best: Option<(usize, Vec<Option<Version>>, SearchResult)>,
    first_error: Option<ResolveError>,
}

impl<'a> Search<'a> {
    fn active(
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
            if let Some(&pi) = self.package_indices.get(name) {
                if let Some(release) = self.catalog[pi]
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

    fn inspect(
        &self,
        roots: &[Requirement],
        assigned: &std::collections::BTreeMap<String, Version>,
    ) -> Result<
        (
            std::collections::BTreeMap<String, Vec<Constraint>>,
            Vec<(String, Vec<Version>)>,
        ),
        ResolveError,
    > {
        let active = self.active(roots, assigned);

        if let Some(name) = active
            .keys()
            .find(|name| !self.package_indices.contains_key(*name))
        {
            return Err(ResolveError::Missing {
                package: name.clone(),
            });
        }

        for (name, version) in assigned {
            if let Some(constraints) = active.get(name) {
                if !constraints.iter().all(|c| matches(*version, *c)) {
                    return Err(ResolveError::Conflict {
                        package: name.clone(),
                        selected: *version,
                        requirements: constraints.clone(),
                    });
                }
            }
        }

        let mut candidates = Vec::new();
        for (name, constraints) in &active {
            if assigned.contains_key(name) {
                continue;
            }
            let pi = self.package_indices[name];
            let mut versions: Vec<Version> = self.catalog[pi]
                .releases
                .iter()
                .map(|r| r.version)
                .filter(|v| constraints.iter().all(|c| matches(*v, *c)))
                .collect();
            versions.sort_by(|a, b| b.cmp(a));
            versions.dedup();

            if versions.is_empty() {
                return Err(ResolveError::NoMatch {
                    package: name.clone(),
                    requirements: constraints.clone(),
                });
            }

            if let Some(locked) = self.locks.get(name) {
                if let Some(pos) = versions.iter().position(|v| v == locked) {
                    let version = versions.remove(pos);
                    versions.insert(0, version);
                }
            }
            candidates.push((name.clone(), versions));
        }
        Ok((active, candidates))
    }

    fn record_solution(
        &mut self,
        assigned: &std::collections::BTreeMap<String, Version>,
        constraints: std::collections::BTreeMap<String, Vec<Constraint>>,
    ) {
        let cost = self
            .locks
            .iter()
            .filter(|(name, version)| assigned.get(*name) != Some(*version))
            .count();
        let names: Vec<String> = self.package_indices.keys().cloned().collect();
        let vector: Vec<Option<Version>> = names
            .iter()
            .map(|name| assigned.get(name).copied())
            .collect();
        let result = SearchResult {
            selected: assigned.iter().map(|(n, v)| (n.clone(), *v)).collect(),
            constraints,
        };

        let replace = match &self.best {
            None => true,
            Some((best_cost, best_vector, _)) => {
                cost < *best_cost || (cost == *best_cost && vector > *best_vector)
            }
        };
        if replace {
            self.best = Some((cost, vector, result));
        }
    }

    fn visit(
        &mut self,
        roots: &[Requirement],
        assigned: &mut std::collections::BTreeMap<String, Version>,
    ) -> Result<(), ResolveError> {
        let (active, candidates) = match self.inspect(roots, assigned) {
            Ok(value) => value,
            Err(error) => {
                if self.first_error.is_none() {
                    self.first_error = Some(error);
                }
                return Ok(());
            }
        };

        if candidates.is_empty() {
            self.record_solution(assigned, active);
            return Ok(());
        }

        let (name, versions) = candidates.into_iter().next().unwrap();
        for version in versions {
            if self.attempts == self.max_attempts {
                return Err(ResolveError::Limit {
                    attempts: self.max_attempts,
                });
            }
            self.attempts += 1;
            assigned.insert(name.clone(), version);
            let result = self.visit(roots, assigned);
            assigned.remove(&name);
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

    let package_indices: std::collections::BTreeMap<String, usize> = catalog
        .iter()
        .enumerate()
        .map(|(i, package)| (package.name.clone(), i))
        .collect();
    let lock_map: std::collections::BTreeMap<String, Version> = locks
        .iter()
        .map(|lock| (lock.package.clone(), lock.version))
        .collect();

    let mut search = Search {
        catalog,
        package_indices: package_indices.clone(),
        locks: lock_map.clone(),
        attempts: 0,
        max_attempts,
        best: None,
        first_error: None,
    };
    let mut assigned = std::collections::BTreeMap::new();
    search.visit(roots, &mut assigned)?;

    let result = if let Some((_, _, result)) = search.best {
        result
    } else {
        return Err(search.first_error.unwrap_or_else(|| ResolveError::NoMatch {
            package: String::new(),
            requirements: Vec::new(),
        }));
    };

    let selected: Vec<Selection> = result
        .selected
        .iter()
        .map(|(package, version)| Selection {
            package: package.clone(),
            version: *version,
        })
        .collect();

    let mut all_names: std::collections::BTreeSet<String> = lock_map.keys().cloned().collect();
    all_names.extend(result.selected.iter().map(|(name, _)| name.clone()));

    let mut changes = Vec::new();
    for package in all_names {
        let before = lock_map.get(&package).copied();
        let after = result
            .selected
            .iter()
            .find(|(name, _)| name == &package)
            .map(|(_, version)| *version);
        if before == after {
            continue;
        }

        let reason = match (before, after) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old), Some(_)) => {
                let exists = package_indices
                    .get(&package)
                    .map(|&pi| catalog[pi].releases.iter().any(|r| r.version == old))
                    .unwrap_or(false);
                if !exists {
                    ChangeReason::Unavailable
                } else if result
                    .constraints
                    .get(&package)
                    .map(|constraints| !constraints.iter().all(|c| matches(old, *c)))
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
            before,
            after,
            reason,
        });
    }

    Ok(Resolution { selected, changes })
}

fn main() {}