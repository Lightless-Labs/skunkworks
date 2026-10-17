use std::collections::{BTreeMap, BTreeSet};

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

fn constraint_rank(c: &Constraint) -> (u8, Version) {
    match *c {
        Constraint::Exact(v) => (0, v),
        Constraint::AtLeast(v) => (1, v),
        Constraint::Before(v) => (2, v),
        Constraint::Caret(v) => (3, v),
    }
}

fn canonical_constraints(cs: &[Constraint]) -> Vec<Constraint> {
    let mut result = cs.to_vec();
    result.sort_by_key(constraint_rank);
    result.dedup();
    result
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
    canonical_constraints(cs)
        .into_iter()
        .map(constraint_text)
        .collect::<Vec<_>>()
        .join(",")
}

fn encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-' => {
                out.push(*byte as char)
            }
            b => out.push_str(&format!("%{:02X}", b)),
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
        ResolveError::Limit { attempts } => format!("limit|attempts={}", attempts),
    }
}

fn matches(v: Version, c: Constraint) -> bool {
    match c {
        Constraint::Exact(x) => v == x,
        Constraint::AtLeast(x) => v >= x,
        Constraint::Before(x) => v < x,
        Constraint::Caret(x) => {
            if v < x {
                false
            } else if x.major > 0 {
                v.major == x.major
            } else if x.minor > 0 {
                v.major == 0 && v.minor == x.minor
            } else {
                v == x
            }
        }
    }
}

type Active = BTreeMap<String, Vec<Constraint>>;
type Selected = BTreeMap<String, Version>;

struct Search<'a> {
    catalog: &'a BTreeMap<String, &'a Package>,
    locks: &'a BTreeMap<String, Version>,
    max_attempts: usize,
    attempts: usize,
    best: Option<(usize, Vec<Selection>)>,
    first_error: Option<ResolveError>,
    limit_hit: bool,
    catalog_names: Vec<String>,
}

impl<'a> Search<'a> {
    fn active_for(&self, selected: &Selected, roots: &[Requirement]) -> Active {
        let mut active = Active::new();
        for root in roots {
            active
                .entry(root.package.clone())
                .or_default()
                .push(root.constraint);
        }
        for (name, version) in selected {
            if let Some(package) = self.catalog.get(name) {
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
            *constraints = canonical_constraints(constraints);
        }
        active
    }

    fn candidates(&self, name: &str, constraints: &[Constraint]) -> Vec<Version> {
        let mut versions: Vec<Version> = self.catalog[name]
            .releases
            .iter()
            .map(|r| r.version)
            .filter(|v| constraints.iter().all(|c| matches(*v, *c)))
            .collect();
        versions.sort_by(|a, b| b.cmp(a));
        if let Some(locked) = self.locks.get(name) {
            if let Some(pos) = versions.iter().position(|v| v == locked) {
                let version = versions.remove(pos);
                versions.insert(0, version);
            }
        }
        versions
    }

    fn solution_cost(&self, selected: &Selected) -> usize {
        self.locks
            .iter()
            .filter(|(name, version)| selected.get(*name) != Some(*version))
            .count()
    }

    fn better_solution(&self, cost: usize, selected: &[Selection]) -> bool {
        let Some((best_cost, best_selected)) = &self.best else {
            return true;
        };
        if cost != *best_cost {
            return cost < *best_cost;
        }
        let current: BTreeMap<&str, Version> = selected
            .iter()
            .map(|s| (s.package.as_str(), s.version))
            .collect();
        let previous: BTreeMap<&str, Version> = best_selected
            .iter()
            .map(|s| (s.package.as_str(), s.version))
            .collect();
        for name in &self.catalog_names {
            let a = current.get(name.as_str());
            let b = previous.get(name.as_str());
            match (a, b) {
                (None, None) => {}
                (None, Some(_)) => return false,
                (Some(_), None) => return true,
                (Some(a), Some(b)) if a != b => return a > b,
                _ => {}
            }
        }
        false
    }

    fn record_error(&mut self, error: ResolveError) {
        if self.first_error.is_none() {
            self.first_error = Some(error);
        }
    }

    fn visit(&mut self, roots: &[Requirement], selected: &mut Selected) {
        if self.limit_hit {
            return;
        }

        let active = self.active_for(selected, roots);

        if let Some(name) = active.keys().find(|name| !self.catalog.contains_key(*name)) {
            self.record_error(ResolveError::Missing {
                package: name.clone(),
            });
            return;
        }

        for (name, version) in selected.iter() {
            if let Some(constraints) = active.get(name) {
                if !constraints.iter().all(|c| matches(*version, *c)) {
                    self.record_error(ResolveError::Conflict {
                        package: name.clone(),
                        selected: *version,
                        requirements: constraints.clone(),
                    });
                    return;
                }
            }
        }

        let mut candidate_map = BTreeMap::new();
        for (name, constraints) in &active {
            if selected.contains_key(name) {
                continue;
            }
            let candidates = self.candidates(name, constraints);
            if candidates.is_empty() {
                self.record_error(ResolveError::NoMatch {
                    package: name.clone(),
                    requirements: constraints.clone(),
                });
                return;
            }
            candidate_map.insert(name.clone(), candidates);
        }

        let Some((name, candidates)) = candidate_map.into_iter().next() else {
            let solution: Vec<Selection> = selected
                .iter()
                .map(|(package, version)| Selection {
                    package: package.clone(),
                    version: *version,
                })
                .collect();
            let cost = self.solution_cost(selected);
            if self.better_solution(cost, &solution) {
                self.best = Some((cost, solution));
            }
            return;
        };

        for version in candidates {
            if self.attempts == self.max_attempts {
                self.limit_hit = true;
                return;
            }
            self.attempts += 1;
            selected.insert(name.clone(), version);
            self.visit(roots, selected);
            selected.remove(&name);
            if self.limit_hit {
                return;
            }
        }
    }
}

fn collect_catalog_names(catalog: &[Package]) -> Vec<String> {
    let mut names: Vec<String> = catalog.iter().map(|p| p.name.clone()).collect();
    names.sort();
    names
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

    let mut seen_packages = BTreeSet::new();
    for package in catalog {
        if !seen_packages.insert(package.name.clone()) {
            return Err(ResolveError::DuplicatePackage {
                package: package.name.clone(),
            });
        }
    }

    for package in catalog {
        let mut seen_versions = BTreeSet::new();
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
    let mut seen_locks = BTreeSet::new();
    for lock in locks {
        if !seen_locks.insert(lock.package.clone()) {
            return Err(ResolveError::DuplicateLock {
                package: lock.package.clone(),
            });
        }
    }

    let catalog_map: BTreeMap<String, &Package> =
        catalog.iter().map(|p| (p.name.clone(), p)).collect();
    let lock_map: BTreeMap<String, Version> = locks
        .iter()
        .map(|lock| (lock.package.clone(), lock.version))
        .collect();

    let mut search = Search {
        catalog: &catalog_map,
        locks: &lock_map,
        max_attempts,
        attempts: 0,
        best: None,
        first_error: None,
        limit_hit: false,
        catalog_names: collect_catalog_names(catalog),
    };

    let mut selected = Selected::new();
    search.visit(roots, &mut selected);

    if search.limit_hit {
        return Err(ResolveError::Limit {
            attempts: max_attempts,
        });
    }

    let Some((_, selected_vec)) = search.best else {
        return Err(search.first_error.unwrap_or_else(|| ResolveError::NoMatch {
            package: String::new(),
            requirements: Vec::new(),
        }));
    };

    let selected_map: BTreeMap<String, Version> = selected_vec
        .iter()
        .map(|s| (s.package.clone(), s.version))
        .collect();
    let mut active = Active::new();
    for root in roots {
        active
            .entry(root.package.clone())
            .or_default()
            .push(root.constraint);
    }
    for selection in &selected_vec {
        if let Some(package) = catalog_map.get(&selection.package) {
            if let Some(release) = package
                .releases
                .iter()
                .find(|r| r.version == selection.version)
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
    for cs in active.values_mut() {
        *cs = canonical_constraints(cs);
    }

    let mut all_names: BTreeSet<String> = lock_map.keys().cloned().collect();
    all_names.extend(selected_map.keys().cloned());
    let mut changes = Vec::new();
    for name in all_names {
        let before = lock_map.get(&name).copied();
        let after = selected_map.get(&name).copied();
        if before == after {
            continue;
        }
        let reason = match (before, after) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old), Some(_)) => {
                let exists = catalog_map
                    .get(&name)
                    .is_some_and(|p| p.releases.iter().any(|r| r.version == old));
                if !exists {
                    ChangeReason::Unavailable
                } else if active
                    .get(&name)
                    .is_some_and(|cs| !cs.iter().all(|c| matches(old, *c)))
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

    Ok(Resolution {
        selected: selected_vec,
        changes,
    })
}