use std::cmp::Ordering;
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

fn constraint_key(c: &Constraint) -> (u8, Version) {
    match *c {
        Constraint::Exact(v) => (0, v),
        Constraint::AtLeast(v) => (1, v),
        Constraint::Before(v) => (2, v),
        Constraint::Caret(v) => (3, v),
    }
}

fn constraint_text(c: &Constraint) -> String {
    match *c {
        Constraint::Exact(v) => format!("={}", version_text(v)),
        Constraint::AtLeast(v) => format!(">={}", version_text(v)),
        Constraint::Before(v) => format!("<{}", version_text(v)),
        Constraint::Caret(v) => format!("^{}", version_text(v)),
    }
}

fn canonicalize(mut constraints: Vec<Constraint>) -> Vec<Constraint> {
    constraints.sort_by_key(constraint_key);
    constraints.dedup();
    constraints
}

fn constraints_text(constraints: &[Constraint]) -> String {
    canonicalize(constraints.to_vec())
        .iter()
        .map(constraint_text)
        .collect::<Vec<_>>()
        .join(",")
}

fn encode(text: &str) -> String {
    let mut out = String::new();
    for &byte in text.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-') {
            out.push(byte as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push('%');
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
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
        .all(|constraint| matches_constraint(version, *constraint))
}

struct Search<'a> {
    by_name: BTreeMap<&'a str, &'a Package>,
    roots: &'a [Requirement],
    locks: BTreeMap<&'a str, Version>,
    attempts: usize,
    max_attempts: usize,
    first_error: Option<ResolveError>,
    best: Option<(usize, Vec<Selection>)>,
    catalog_names: Vec<&'a str>,
}

impl Search<'_> {
    fn active_constraints(
        &self,
        selected: &BTreeMap<String, Version>,
    ) -> BTreeMap<String, Vec<Constraint>> {
        let mut active = BTreeMap::new();

        for root in self.roots {
            active
                .entry(root.package.clone())
                .or_insert_with(Vec::new)
                .push(root.constraint);
        }

        for (name, version) in selected {
            if let Some(package) = self.by_name.get(name.as_str()) {
                if let Some(release) = package.releases.iter().find(|r| r.version == *version) {
                    for dep in &release.deps {
                        active
                            .entry(dep.package.clone())
                            .or_insert_with(Vec::new)
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

    fn compare_vectors(&self, left: &[Selection], right: &[Selection]) -> Ordering {
        let left_map = left
            .iter()
            .map(|s| (s.package.as_str(), s.version))
            .collect::<BTreeMap<_, _>>();
        let right_map = right
            .iter()
            .map(|s| (s.package.as_str(), s.version))
            .collect::<BTreeMap<_, _>>();

        for name in &self.catalog_names {
            match (left_map.get(name), right_map.get(name)) {
                (Some(a), Some(b)) => {
                    let order = a.cmp(b);
                    if !order.is_eq() {
                        return order;
                    }
                }
                (Some(_), None) => return Ordering::Greater,
                (None, Some(_)) => return Ordering::Less,
                (None, None) => {}
            }
        }
        Ordering::Equal
    }

    fn inspect(&mut self, selected: &mut BTreeMap<String, Version>) -> Result<(), ResolveError> {
        let active = self.active_constraints(selected);

        if let Some(name) = active
            .keys()
            .find(|name| !self.by_name.contains_key(name.as_str()))
        {
            return Err(ResolveError::Missing {
                package: name.clone(),
            });
        }

        for (name, version) in selected.iter() {
            if let Some(constraints) = active.get(name) {
                if !matches_all(*version, constraints) {
                    return Err(ResolveError::Conflict {
                        package: name.clone(),
                        selected: *version,
                        requirements: constraints.clone(),
                    });
                }
            }
        }

        for (name, constraints) in &active {
            if selected.contains_key(name) {
                continue;
            }
            let package = self.by_name[name.as_str()];
            if !package
                .releases
                .iter()
                .any(|release| matches_all(release.version, constraints))
            {
                return Err(ResolveError::NoMatch {
                    package: name.clone(),
                    requirements: constraints.clone(),
                });
            }
        }

        let Some(name) = active.keys().find(|name| !selected.contains_key(*name)) else {
            let solution = selected
                .iter()
                .map(|(package, version)| Selection {
                    package: package.clone(),
                    version: *version,
                })
                .collect::<Vec<_>>();
            let lock_cost = self
                .locks
                .iter()
                .filter(|(package, version)| selected.get(**package) != Some(*version))
                .count();

            let replace = match &self.best {
                None => true,
                Some((best_cost, _)) if lock_cost < *best_cost => true,
                Some((best_cost, best_selection)) if lock_cost == *best_cost => {
                    self.compare_vectors(&solution, best_selection).is_gt()
                }
                _ => false,
            };
            if replace {
                self.best = Some((lock_cost, solution));
            }
            return Ok(());
        };

        let constraints = active.get(name).expect("active package has constraints");
        let package = self.by_name[name.as_str()];
        let mut candidates = package
            .releases
            .iter()
            .filter(|release| matches_all(release.version, constraints))
            .map(|release| release.version)
            .collect::<Vec<_>>();
        candidates.sort_by(|a, b| b.cmp(a));

        if let Some(locked) = self.locks.get(name.as_str()) {
            if let Some(index) = candidates.iter().position(|version| version == locked) {
                let version = candidates.remove(index);
                candidates.insert(0, version);
            }
        }

        for candidate in candidates {
            if self.attempts == self.max_attempts {
                return Err(ResolveError::Limit {
                    attempts: self.max_attempts,
                });
            }
            self.attempts += 1;
            selected.insert(name.clone(), candidate);

            match self.inspect(selected) {
                Ok(()) => {}
                Err(error @ ResolveError::Limit { .. }) => {
                    selected.remove(name);
                    return Err(error);
                }
                Err(error) => {
                    if self.first_error.is_none() {
                        self.first_error = Some(error);
                    }
                }
            }

            selected.remove(name);
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
    for (index, root) in roots.iter().enumerate() {
        if !valid_name(&root.package) {
            return Err(ResolveError::InvalidName {
                site: format!("root[{}]", index),
                name: root.package.clone(),
            });
        }
    }

    for (pi, package) in catalog.iter().enumerate() {
        if !valid_name(&package.name) {
            return Err(ResolveError::InvalidName {
                site: format!("catalog[{}]", pi),
                name: package.name.clone(),
            });
        }
        for (ri, release) in package.releases.iter().enumerate() {
            for (di, dependency) in release.deps.iter().enumerate() {
                if !valid_name(&dependency.package) {
                    return Err(ResolveError::InvalidName {
                        site: format!("catalog[{}].release[{}].dep[{}]", pi, ri, di),
                        name: dependency.package.clone(),
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

    for (index, lock) in locks.iter().enumerate() {
        if !valid_name(&lock.package) {
            return Err(ResolveError::InvalidName {
                site: format!("lock[{}]", index),
                name: lock.package.clone(),
            });
        }
    }

    let mut seen_locks = BTreeSet::new();
    for lock in locks {
        if !seen_locks.insert(lock.package.as_str()) {
            return Err(ResolveError::DuplicateLock {
                package: lock.package.clone(),
            });
        }
    }

    let by_name = catalog
        .iter()
        .map(|package| (package.name.as_str(), package))
        .collect::<BTreeMap<_, _>>();
    let lock_map = locks
        .iter()
        .map(|lock| (lock.package.as_str(), lock.version))
        .collect::<BTreeMap<_, _>>();
    let mut catalog_names = catalog.iter().map(|p| p.name.as_str()).collect::<Vec<_>>();
    catalog_names.sort_unstable();

    let mut search = Search {
        by_name: by_name.clone(),
        roots,
        locks: lock_map,
        attempts: 0,
        max_attempts,
        first_error: None,
        best: None,
        catalog_names,
    };

    let mut selected_map = BTreeMap::new();
    if let Err(error) = search.inspect(&mut selected_map) {
        return Err(error);
    }

    let Some((_, selected)) = search.best.take() else {
        return Err(search.first_error.unwrap_or(ResolveError::NoMatch {
            package: roots
                .first()
                .map(|r| r.package.clone())
                .unwrap_or_default(),
            requirements: Vec::new(),
        }));
    };

    let selected_versions = selected
        .iter()
        .map(|s| (s.package.clone(), s.version))
        .collect::<BTreeMap<_, _>>();
    let final_active = search.active_constraints(&selected_versions);

    let mut names = BTreeSet::new();
    for lock in locks {
        names.insert(lock.package.as_str());
    }
    for selection in &selected {
        names.insert(selection.package.as_str());
    }

    let mut changes = Vec::new();
    for name in names {
        let before = locks
            .iter()
            .find(|lock| lock.package == name)
            .map(|lock| lock.version);
        let after = selected_versions.get(name).copied();
        if before == after {
            continue;
        }

        let reason = match (before, after) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old), Some(_)) => {
                let exists = by_name
                    .get(name)
                    .is_some_and(|package| package.releases.iter().any(|r| r.version == old));
                if !exists {
                    ChangeReason::Unavailable
                } else if !final_active
                    .get(name)
                    .is_some_and(|constraints| matches_all(old, constraints))
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

fn main() {}