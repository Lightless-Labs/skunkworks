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

fn canonical(mut cs: Vec<Constraint>) -> Vec<Constraint> {
    cs.sort();
    cs.dedup();
    cs
}

fn constraints_text(cs: &[Constraint]) -> String {
    constraint_texts(cs)
        .join(",")
}

fn constraint_texts(cs: &[Constraint]) -> Vec<String> {
    canonical(cs.to_vec())
        .into_iter()
        .map(constraint_text)
        .collect()
}

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
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

fn permits(version: Version, constraint: Constraint) -> bool {
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

type Active = BTreeMap<String, Vec<Constraint>>;
type Chosen = BTreeMap<String, Version>;

fn active_constraints(roots: &[Requirement], chosen: &Chosen, catalog: &BTreeMap<String, Package>) -> Active {
    let mut active: Active = BTreeMap::new();
    for root in roots {
        active
            .entry(root.package.clone())
            .or_default()
            .push(root.constraint);
    }
    for (name, version) in chosen {
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
    roots: &'a [Requirement],
    catalog: &'a BTreeMap<String, Package>,
    locks: &'a BTreeMap<String, Version>,
    all_names: &'a [String],
    budget: usize,
    attempts: usize,
    best: Option<(usize, Vec<Option<Version>>, Chosen, Active)>,
    first_error: Option<ResolveError>,
    limited: bool,
}

impl Search<'_> {
    fn visit(&mut self, chosen: &mut Chosen) {
        if self.limited {
            return;
        }

        let active = active_constraints(self.roots, chosen, self.catalog);

        if let Some(name) = active.keys().filter(|n| !self.catalog.contains_key(*n)).next() {
            self.remember_error(ResolveError::Missing {
                package: name.clone(),
            });
            return;
        }

        for (name, version) in chosen.iter() {
            if let Some(constraints) = active.get(name) {
                if !constraints.iter().all(|c| permits(*version, *c)) {
                    self.remember_error(ResolveError::Conflict {
                        package: name.clone(),
                        selected: *version,
                        requirements: constraints.clone(),
                    });
                    return;
                }
            }
        }

        for (name, constraints) in &active {
            if !chosen.contains_key(name) {
                let package = &self.catalog[name];
                if !package.releases.iter().any(|release| {
                    constraints.iter().all(|c| permits(release.version, *c))
                }) {
                    self.remember_error(ResolveError::NoMatch {
                        package: name.clone(),
                        requirements: constraints.clone(),
                    });
                    return;
                }
            }
        }

        let next = active.keys().find(|n| !chosen.contains_key(*n)).cloned();
        let Some(name) = next else {
            let cost = self
                .locks
                .iter()
                .filter(|(n, v)| chosen.get(*n) != Some(*v))
                .count();
            let vector = self
                .all_names
                .iter()
                .map(|n| chosen.get(n).copied())
                .collect::<Vec<_>>();
            let replace = match &self.best {
                None => true,
                Some((best_cost, best_vector, _, _)) => {
                    cost < *best_cost || (cost == *best_cost && vector > *best_vector)
                }
            };
            if replace {
                self.best = Some((cost, vector, chosen.clone(), active));
            }
            return;
        };

        let constraints = &active[&name];
        let package = &self.catalog[&name];
        let mut candidates: Vec<Version> = package
            .releases
            .iter()
            .filter(|r| constraints.iter().all(|c| permits(r.version, *c)))
            .map(|r| r.version)
            .collect();
        candidates.sort_by(|a, b| b.cmp(a));
        if let Some(lock) = self.locks.get(&name) {
            if let Some(pos) = candidates.iter().position(|v| v == lock) {
                let v = candidates.remove(pos);
                candidates.insert(0, v);
            }
        }

        for version in candidates {
            if self.attempts == self.budget {
                self.limited = true;
                return;
            }
            self.attempts += 1;
            chosen.insert(name.clone(), version);
            self.visit(chosen);
            chosen.remove(&name);
            if self.limited {
                return;
            }
        }
    }

    fn remember_error(&mut self, error: ResolveError) {
        if self.first_error.is_none() {
            self.first_error = Some(error);
        }
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

    let catalog_map: BTreeMap<String, Package> = catalog
        .iter()
        .map(|p| (p.name.clone(), p.clone()))
        .collect();
    let lock_map: BTreeMap<String, Version> = locks
        .iter()
        .map(|l| (l.package.clone(), l.version))
        .collect();
    let mut all_names: Vec<String> = catalog.iter().map(|p| p.name.clone()).collect();
    all_names.sort();

    let mut search = Search {
        roots,
        catalog: &catalog_map,
        locks: &lock_map,
        all_names: &all_names,
        budget: max_attempts,
        attempts: 0,
        best: None,
        first_error: None,
        limited: false,
    };
    search.visit(&mut BTreeMap::new());

    if search.limited {
        return Err(ResolveError::Limit {
            attempts: max_attempts,
        });
    }

    let Some((_, _, chosen, final_active)) = search.best else {
        return Err(search.first_error.unwrap_or(ResolveError::NoMatch {
            package: String::new(),
            requirements: Vec::new(),
        }));
    };

    let selected = chosen
        .iter()
        .map(|(package, version)| Selection {
            package: package.clone(),
            version: *version,
        })
        .collect::<Vec<_>>();

    let mut names: BTreeSet<String> = lock_map.keys().cloned().collect();
    names.extend(chosen.keys().cloned());
    let mut changes = Vec::new();
    for package in names {
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
                    .is_some_and(|cs| !cs.iter().all(|c| permits(old, *c)))
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