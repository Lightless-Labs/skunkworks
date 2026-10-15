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
    let b = name.as_bytes();
    !b.is_empty()
        && b.len() <= 24
        && b[0].is_ascii_lowercase()
        && b[b.len() - 1] != b'-'
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
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

fn canonical(mut cs: Vec<Constraint>) -> Vec<Constraint> {
    cs.sort();
    cs.dedup();
    cs
}

fn constraints_text(cs: &[Constraint]) -> String {
    canonical(cs.to_vec())
        .iter()
        .map(|c| constraint_text(*c))
        .collect::<Vec<_>>()
        .join(",")
}

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
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

fn meets(v: Version, cs: &[Constraint]) -> bool {
    cs.iter().all(|c| matches(v, *c))
}

fn active_constraints(
    roots: &[Requirement],
    selected: &BTreeMap<String, Version>,
    catalog: &BTreeMap<String, &Package>,
) -> BTreeMap<String, Vec<Constraint>> {
    let mut active = BTreeMap::<String, Vec<Constraint>>::new();
    for req in roots {
        active
            .entry(req.package.clone())
            .or_default()
            .push(req.constraint);
    }
    for (name, version) in selected {
        if let Some(pkg) = catalog.get(name) {
            if let Some(release) = pkg.releases.iter().find(|r| r.version == *version) {
                for req in &release.deps {
                    active
                        .entry(req.package.clone())
                        .or_default()
                        .push(req.constraint);
                }
            }
        }
    }
    for cs in active.values_mut() {
        *cs = canonical(std::mem::take(cs));
    }
    active
}

struct Search<'a> {
    roots: &'a [Requirement],
    catalog: BTreeMap<String, &'a Package>,
    locks: BTreeMap<String, Version>,
    catalog_names: Vec<String>,
    attempts: usize,
    max_attempts: usize,
    best: Option<(usize, Vec<Version>, BTreeMap<String, Version>, BTreeMap<String, Vec<Constraint>>)>,
    first_error: Option<ResolveError>,
}

impl Search<'_> {
    fn inspect(&mut self, selected: &BTreeMap<String, Version>) -> Result<(), ResolveError> {
        let active = active_constraints(self.roots, selected, &self.catalog);

        if let Some(name) = active.keys().find(|n| !self.catalog.contains_key(*n)) {
            return Err(ResolveError::Missing {
                package: name.clone(),
            });
        }

        for (name, version) in selected {
            if let Some(cs) = active.get(name) {
                if !meets(*version, cs) {
                    return Err(ResolveError::Conflict {
                        package: name.clone(),
                        selected: *version,
                        requirements: cs.clone(),
                    });
                }
            }
        }

        for (name, cs) in &active {
            if selected.contains_key(name) {
                continue;
            }
            let pkg = self.catalog.get(name).unwrap();
            if !pkg.releases.iter().any(|r| meets(r.version, cs)) {
                return Err(ResolveError::NoMatch {
                    package: name.clone(),
                    requirements: cs.clone(),
                });
            }
        }
        Ok(())
    }

    fn solution_cost(&self, selected: &BTreeMap<String, Version>) -> usize {
        self.locks
            .iter()
            .filter(|(name, version)| selected.get(*name) != Some(*version))
            .count()
    }

    fn solution_vector(&self, selected: &BTreeMap<String, Version>) -> Vec<Version> {
        self.catalog_names
            .iter()
            .map(|name| {
                selected.get(name).copied().unwrap_or(Version {
                    major: 0,
                    minor: 0,
                    patch: 0,
                })
            })
            .collect()
    }

    fn record_solution(
        &mut self,
        selected: &BTreeMap<String, Version>,
        active: BTreeMap<String, Vec<Constraint>>,
    ) {
        let cost = self.solution_cost(selected);
        let vector = self.solution_vector(selected);
        let replace = match &self.best {
            None => true,
            Some((best_cost, best_vector, _, _)) => {
                cost < *best_cost || (cost == *best_cost && vector > *best_vector)
            }
        };
        if replace {
            self.best = Some((cost, vector, selected.clone(), active));
        }
    }

    fn search(&mut self, selected: &mut BTreeMap<String, Version>) -> Result<(), ResolveError> {
        let active = match self.inspect(selected) {
            Ok(()) => active_constraints(self.roots, selected, &self.catalog),
            Err(e) => {
                if self.first_error.is_none() {
                    self.first_error = Some(e);
                }
                return Ok(());
            }
        };

        let next = active.keys().find(|name| !selected.contains_key(*name)).cloned();
        let Some(name) = next else {
            self.record_solution(selected, active);
            return Ok(());
        };

        let cs = active.get(&name).cloned().unwrap_or_default();
        let pkg = self.catalog.get(&name).unwrap();
        let mut candidates: Vec<Version> = pkg
            .releases
            .iter()
            .map(|r| r.version)
            .filter(|v| meets(*v, &cs))
            .collect();
        candidates.sort_by(|a, b| b.cmp(a));

        if let Some(lock) = self.locks.get(&name) {
            if let Some(pos) = candidates.iter().position(|v| v == lock) {
                let locked = candidates.remove(pos);
                candidates.insert(0, locked);
            }
        }

        for version in candidates {
            if self.attempts == self.max_attempts {
                return Err(ResolveError::Limit {
                    attempts: self.max_attempts,
                });
            }
            self.attempts += 1;
            selected.insert(name.clone(), version);
            let result = self.search(selected);
            selected.remove(&name);
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
    for (i, req) in roots.iter().enumerate() {
        if !valid_name(&req.package) {
            return Err(ResolveError::InvalidName {
                site: format!("root[{i}]"),
                name: req.package.clone(),
            });
        }
    }

    for (i, pkg) in catalog.iter().enumerate() {
        if !valid_name(&pkg.name) {
            return Err(ResolveError::InvalidName {
                site: format!("catalog[{i}]"),
                name: pkg.name.clone(),
            });
        }
        for (j, release) in pkg.releases.iter().enumerate() {
            for (k, req) in release.deps.iter().enumerate() {
                if !valid_name(&req.package) {
                    return Err(ResolveError::InvalidName {
                        site: format!("catalog[{i}].release[{j}].dep[{k}]"),
                        name: req.package.clone(),
                    });
                }
            }
        }
    }

    let mut seen_packages = BTreeSet::new();
    for pkg in catalog {
        if !seen_packages.insert(pkg.name.clone()) {
            return Err(ResolveError::DuplicatePackage {
                package: pkg.name.clone(),
            });
        }
    }

    for pkg in catalog {
        let mut versions = BTreeSet::new();
        for release in &pkg.releases {
            if !versions.insert(release.version) {
                return Err(ResolveError::DuplicateVersion {
                    package: pkg.name.clone(),
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

    let mut seen_locks = BTreeSet::new();
    let mut lock_map = BTreeMap::new();
    for lock in locks {
        if !seen_locks.insert(lock.package.clone()) {
            return Err(ResolveError::DuplicateLock {
                package: lock.package.clone(),
            });
        }
        lock_map.insert(lock.package.clone(), lock.version);
    }

    let catalog_map: BTreeMap<String, &Package> =
        catalog.iter().map(|p| (p.name.clone(), p)).collect();
    let mut catalog_names: Vec<String> = catalog.iter().map(|p| p.name.clone()).collect();
    catalog_names.sort();

    let mut search = Search {
        roots,
        catalog: catalog_map,
        locks: lock_map.clone(),
        catalog_names,
        attempts: 0,
        max_attempts,
        best: None,
        first_error: None,
    };

    let mut selected = BTreeMap::new();
    search.search(&mut selected)?;

    let Some((_, _, chosen, final_constraints)) = search.best else {
        return Err(search.first_error.unwrap_or(ResolveError::NoMatch {
            package: String::new(),
            requirements: Vec::new(),
        }));
    };

    let selected_vec: Vec<Selection> = chosen
        .iter()
        .map(|(package, version)| Selection {
            package: package.clone(),
            version: *version,
        })
        .collect();

    let mut all_names = BTreeSet::new();
    all_names.extend(lock_map.keys().cloned());
    all_names.extend(chosen.keys().cloned());

    let mut changes = Vec::new();
    for package in all_names {
        let before = lock_map.get(&package).copied();
        let after = chosen.get(&package).copied();
        if before == after {
            continue;
        }
        let reason = match (before, after) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old), Some(_)) => {
                let exists = search
                    .catalog
                    .get(&package)
                    .map(|p| p.releases.iter().any(|r| r.version == old))
                    .unwrap_or(false);
                if !exists {
                    ChangeReason::Unavailable
                } else if !meets(
                    old,
                    final_constraints
                        .get(&package)
                        .map(Vec::as_slice)
                        .unwrap_or(&[]),
                ) {
                    ChangeReason::Rejected
                } else {
                    ChangeReason::Rebalanced
                }
            }
            (None, None) => continue,
        };
        changes.push(Change {
            package,
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

fn main() {}