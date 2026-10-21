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

fn validate(catalog: &[Package], roots: &[Requirement], locks: &[Lock]) -> Result<(), ResolveError> {
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

    let mut seen = std::collections::BTreeSet::new();
    for package in catalog {
        if !seen.insert(package.name.as_str()) {
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
    seen.clear();
    for lock in locks {
        if !seen.insert(lock.package.as_str()) {
            return Err(ResolveError::DuplicateLock {
                package: lock.package.clone(),
            });
        }
    }
    Ok(())
}

fn constraint_matches(constraint: Constraint, version: Version) -> bool {
    match constraint {
        Constraint::Exact(v) => version == v,
        Constraint::AtLeast(v) => version >= v,
        Constraint::Before(v) => version < v,
        Constraint::Caret(v) => {
            if version < v {
                false
            } else if v.major > 0 {
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

fn satisfies(version: Version, constraints: &[Constraint]) -> bool {
    constraints
        .iter()
        .all(|constraint| constraint_matches(*constraint, version))
}

fn version_text(version: Version) -> String {
    format!("{}.{}.{}", version.major, version.minor, version.patch)
}

fn constraint_text(constraint: Constraint) -> String {
    match constraint {
        Constraint::Exact(v) => format!("={}", version_text(v)),
        Constraint::AtLeast(v) => format!(">={}", version_text(v)),
        Constraint::Before(v) => format!("<{}", version_text(v)),
        Constraint::Caret(v) => format!("^{}", version_text(v)),
    }
}

fn constraints_text(constraints: &[Constraint]) -> String {
    canonical(constraints.to_vec())
        .iter()
        .map(|constraint| constraint_text(*constraint))
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

#[derive(Clone)]
struct SearchResult {
    selected: std::collections::BTreeMap<String, Version>,
    constraints: std::collections::BTreeMap<String, Vec<Constraint>>,
}

struct Search<'a> {
    catalog: &'a [Package],
    locks: std::collections::BTreeMap<String, Version>,
    catalog_names: Vec<String>,
    max_attempts: usize,
    attempts: usize,
    best: Option<SearchResult>,
    best_cost: usize,
}

impl Search<'_> {
    fn inspect(
        &mut self,
        selected: &std::collections::BTreeMap<String, Version>,
        first_error: &mut Option<ResolveError>,
    ) -> Result<(), ResolveError> {
        let mut active: std::collections::BTreeMap<String, Vec<Constraint>> =
            std::collections::BTreeMap::new();
        for root in ROOTS.with(|roots| roots.borrow().clone()) {
            active.entry(root.package).or_default().push(root.constraint);
        }
        for (name, version) in selected {
            if let Some(package) = self.catalog.iter().find(|p| &p.name == name) {
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

        if let Some(name) = active
            .keys()
            .filter(|name| !self.catalog.iter().any(|p| &p.name == *name))
            .next()
        {
            first_error.get_or_insert(ResolveError::Missing {
                package: name.clone(),
            });
            return Ok(());
        }

        for (name, version) in selected {
            if let Some(constraints) = active.get(name) {
                if !satisfies(*version, constraints) {
                    first_error.get_or_insert(ResolveError::Conflict {
                        package: name.clone(),
                        selected: *version,
                        requirements: constraints.clone(),
                    });
                    return Ok(());
                }
            }
        }

        let unselected: Vec<String> = active
            .keys()
            .filter(|name| !selected.contains_key(*name))
            .cloned()
            .collect();

        for name in &unselected {
            let constraints = active.get(name).unwrap();
            let package = self.catalog.iter().find(|p| &p.name == name).unwrap();
            if !package
                .releases
                .iter()
                .any(|r| satisfies(r.version, constraints))
            {
                first_error.get_or_insert(ResolveError::NoMatch {
                    package: name.clone(),
                    requirements: constraints.clone(),
                });
                return Ok(());
            }
        }

        let Some(name) = unselected.first() else {
            let cost = self
                .locks
                .iter()
                .filter(|(name, version)| selected.get(*name) != Some(*version))
                .count();
            if self.best.is_none() || cost < self.best_cost {
                self.best_cost = cost;
                self.best = Some(SearchResult {
                    selected: selected.clone(),
                    constraints: active,
                });
            } else if cost == self.best_cost {
                let better = self.catalog_names.iter().find_map(|name| {
                    let a = selected.get(name);
                    let b = self.best.as_ref().unwrap().selected.get(name);
                    if a == b {
                        None
                    } else {
                        Some(match (a, b) {
                            (None, Some(_)) => false,
                            (Some(_), None) => true,
                            (Some(a), Some(b)) => a > b,
                            (None, None) => false,
                        })
                    }
                });
                if better == Some(true) {
                    self.best = Some(SearchResult {
                        selected: selected.clone(),
                        constraints: active,
                    });
                }
            }
            return Ok(());
        };

        let constraints = active.get(name).unwrap();
        let package = self.catalog.iter().find(|p| &p.name == name).unwrap();
        let mut candidates: Vec<Version> = package
            .releases
            .iter()
            .map(|r| r.version)
            .filter(|v| satisfies(*v, constraints))
            .collect();
        candidates.sort_by(|a, b| b.cmp(a));
        if let Some(lock) = self.locks.get(name) {
            if let Some(index) = candidates.iter().position(|v| v == lock) {
                candidates.remove(index);
                candidates.insert(0, *lock);
            }
        }

        for version in candidates {
            if self.attempts == self.max_attempts {
                return Err(ResolveError::Limit {
                    attempts: self.max_attempts,
                });
            }
            self.attempts += 1;
            let mut child = selected.clone();
            child.insert(name.clone(), version);
            self.inspect(&child, first_error)?;
        }
        Ok(())
    }
}

thread_local! {
    static ROOTS: std::cell::RefCell<Vec<Requirement>> = const { std::cell::RefCell::new(Vec::new()) };
}

pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
    locks: &[Lock],
    max_attempts: usize,
) -> Result<Resolution, ResolveError> {
    validate(catalog, roots, locks)?;
    let mut lock_map = std::collections::BTreeMap::new();
    for lock in locks {
        lock_map.insert(lock.package.clone(), lock.version);
    }
    let mut names: Vec<String> = catalog.iter().map(|p| p.name.clone()).collect();
    names.sort();
    ROOTS.with(|stored| *stored.borrow_mut() = roots.to_vec());

    let mut search = Search {
        catalog,
        locks: lock_map.clone(),
        catalog_names: names,
        max_attempts,
        attempts: 0,
        best: None,
        best_cost: usize::MAX,
    };
    let mut first_error = None;
    let empty = std::collections::BTreeMap::new();
    let result = search.inspect(&empty, &mut first_error);
    ROOTS.with(|stored| stored.borrow_mut().clear());
    result?;

    let Some(best) = search.best else {
        return Err(first_error.unwrap_or(ResolveError::NoMatch {
            package: String::new(),
            requirements: Vec::new(),
        }));
    };

    let selected: Vec<Selection> = best
        .selected
        .iter()
        .map(|(package, version)| Selection {
            package: package.clone(),
            version: *version,
        })
        .collect();

    let mut all_names: std::collections::BTreeSet<String> =
        lock_map.keys().cloned().collect();
    all_names.extend(best.selected.keys().cloned());
    let mut changes = Vec::new();
    for package in all_names {
        let before = lock_map.get(&package).copied();
        let after = best.selected.get(&package).copied();
        if before == after {
            continue;
        }
        let reason = match (before, after) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old), Some(_)) => {
                let exists = catalog
                    .iter()
                    .find(|p| p.name == package)
                    .is_some_and(|p| p.releases.iter().any(|r| r.version == old));
                if !exists {
                    ChangeReason::Unavailable
                } else if best
                    .constraints
                    .get(&package)
                    .is_some_and(|constraints| !satisfies(old, constraints))
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