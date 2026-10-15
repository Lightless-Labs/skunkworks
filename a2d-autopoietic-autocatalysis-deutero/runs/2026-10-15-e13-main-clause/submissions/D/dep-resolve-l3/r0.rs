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
    Conflict {
        package: String,
        selected: Version,
        requirements: Vec<Constraint>,
    },
    Limit { attempts: usize },
}

use std::collections::{BTreeMap, BTreeSet};

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
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-') {
            out.push(byte as char);
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

fn matches(version: Version, constraints: &[Constraint]) -> bool {
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

fn active_constraints(
    roots: &[Requirement],
    selected: &BTreeMap<String, Version>,
    catalog: &BTreeMap<String, &Package>,
) -> BTreeMap<String, Vec<Constraint>> {
    let mut active = BTreeMap::<String, Vec<Constraint>>::new();
    for root in roots {
        active
            .entry(root.package.clone())
            .or_default()
            .push(root.constraint);
    }
    for (name, version) in selected {
        if let Some(package) = catalog.get(name) {
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
    catalog: &'a BTreeMap<String, &'a Package>,
    roots: &'a [Requirement],
    locks: &'a BTreeMap<String, Version>,
    max_attempts: usize,
    attempts: usize,
    best: Option<(usize, BTreeMap<String, Version>)>,
    first_error: Option<ResolveError>,
}

impl Search<'_> {
    fn inspect(&mut self, selected: &BTreeMap<String, Version>) -> Result<(), ResolveError> {
        let active = active_constraints(self.roots, selected, self.catalog);

        if let Some(name) = active.keys().find(|name| !self.catalog.contains_key(*name)) {
            return Err(ResolveError::Missing {
                package: name.clone(),
            });
        }

        for (name, version) in selected {
            if let Some(requirements) = active.get(name) {
                if !matches(*version, requirements) {
                    return Err(ResolveError::Conflict {
                        package: name.clone(),
                        selected: *version,
                        requirements: requirements.clone(),
                    });
                }
            }
        }

        for (name, requirements) in &active {
            if selected.contains_key(name) {
                continue;
            }
            let package = self.catalog[name];
            if !package
                .releases
                .iter()
                .any(|release| matches(release.version, requirements))
            {
                return Err(ResolveError::NoMatch {
                    package: name.clone(),
                    requirements: requirements.clone(),
                });
            }
        }
        Ok(())
    }

    fn visit(&mut self, selected: &mut BTreeMap<String, Version>) -> Result<(), ResolveError> {
        let active = match self.inspect(selected) {
            Ok(()) => active_constraints(self.roots, selected, self.catalog),
            Err(error) => {
                if self.first_error.is_none() {
                    self.first_error = Some(error);
                }
                return Ok(());
            }
        };

        let next = active.keys().find(|name| !selected.contains_key(*name)).cloned();
        let Some(name) = next else {
            let cost = self
                .locks
                .iter()
                .filter(|(package, version)| selected.get(*package) != Some(*version))
                .count();
            let replace = match &self.best {
                None => true,
                Some((best_cost, best_solution)) => {
                    if cost < *best_cost {
                        true
                    } else if cost > *best_cost {
                        false
                    } else {
                        self.catalog.keys().any(|_| false)
                            || compare_vectors(selected, best_solution, self.catalog) == std::cmp::Ordering::Greater
                    }
                }
            };
            if replace {
                self.best = Some((cost, selected.clone()));
            }
            return Ok(());
        };

        let requirements = &active[&name];
        let mut candidates: Vec<Version> = self.catalog[&name]
            .releases
            .iter()
            .map(|release| release.version)
            .filter(|version| matches(*version, requirements))
            .collect();
        candidates.sort_by(|a, b| b.cmp(a));

        if let Some(locked) = self.locks.get(&name) {
            if let Some(index) = candidates.iter().position(|v| v == locked) {
                let version = candidates.remove(index);
                candidates.insert(0, version);
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
            let child_result = self.visit(selected);
            selected.remove(&name);
            child_result?;
        }
        Ok(())
    }
}

fn compare_vectors(
    left: &BTreeMap<String, Version>,
    right: &BTreeMap<String, Version>,
    catalog: &BTreeMap<String, &Package>,
) -> std::cmp::Ordering {
    for name in catalog.keys() {
        match (left.get(name), right.get(name)) {
            (Some(a), Some(b)) => match a.cmp(b) {
                std::cmp::Ordering::Equal => {}
                order => return order,
            },
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (None, None) => {}
        }
    }
    std::cmp::Ordering::Equal
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

    let mut seen_packages = BTreeSet::new();
    for package in catalog {
        if !seen_packages.insert(package.name.as_str()) {
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
                site: format!("lock[{i}]"),
                name: lock.package.clone(),
            });
        }
    }

    let mut lock_map = BTreeMap::new();
    for lock in locks {
        if lock_map.insert(lock.package.clone(), lock.version).is_some() {
            return Err(ResolveError::DuplicateLock {
                package: lock.package.clone(),
            });
        }
    }

    let catalog_map: BTreeMap<String, &Package> = catalog
        .iter()
        .map(|package| (package.name.clone(), package))
        .collect();

    let mut search = Search {
        catalog: &catalog_map,
        roots,
        locks: &lock_map,
        max_attempts,
        attempts: 0,
        best: None,
        first_error: None,
    };
    search.visit(&mut BTreeMap::new())?;

    let Some((_, chosen)) = search.best else {
        return Err(search.first_error.unwrap_or_else(|| ResolveError::NoMatch {
            package: String::new(),
            requirements: Vec::new(),
        }));
    };

    let selected: Vec<Selection> = chosen
        .iter()
        .map(|(package, version)| Selection {
            package: package.clone(),
            version: *version,
        })
        .collect();

    let final_active = active_constraints(roots, &chosen, &catalog_map);
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
                let exists = catalog_map
                    .get(&package)
                    .is_some_and(|p| p.releases.iter().any(|r| r.version == old));
                if !exists {
                    ChangeReason::Unavailable
                } else if final_active
                    .get(&package)
                    .is_some_and(|constraints| !matches(old, constraints))
                {
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

    Ok(Resolution { selected, changes })
}