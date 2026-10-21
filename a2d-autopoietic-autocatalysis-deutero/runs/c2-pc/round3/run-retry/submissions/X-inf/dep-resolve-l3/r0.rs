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
    if bytes[bytes.len() - 1] == b'-' {
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

fn canonicalize(mut constraints: Vec<Constraint>) -> Vec<Constraint> {
    constraints.sort();
    constraints.dedup();
    constraints
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

fn matches_all(v: Version, constraints: &[Constraint]) -> bool {
    constraints.iter().all(|c| matches(v, *c))
}

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(*b, b'.' | b'_' | b'-') {
            out.push(*b as char);
        } else {
            out.push('%');
            out.push_str(&format!("{:02X}", b));
        }
    }
    out
}

fn constraints_text(constraints: &[Constraint]) -> String {
    canonicalize(constraints.to_vec())
        .iter()
        .map(|c| constraint_text(*c))
        .collect::<Vec<_>>()
        .join(",")
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

#[derive(Clone)]
struct SearchResult {
    selected: Vec<(String, Version)>,
    constraints: std::collections::BTreeMap<String, Vec<Constraint>>,
}

struct Search<'a> {
    catalog: &'a [Package],
    package_index: std::collections::BTreeMap<String, usize>,
    locks: std::collections::BTreeMap<String, Version>,
    max_attempts: usize,
    attempts: usize,
    best: Option<SearchResult>,
    best_cost: usize,
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
            if let Some(&pi) = self.package_index.get(name) {
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
            *constraints = canonicalize(std::mem::take(constraints));
        }
        active
    }

    fn inspect(
        &self,
        active: &std::collections::BTreeMap<String, Vec<Constraint>>,
        assigned: &std::collections::BTreeMap<String, Version>,
    ) -> Result<Option<String>, ResolveError> {
        if let Some(name) = active
            .keys()
            .filter(|n| !self.package_index.contains_key(*n))
            .next()
        {
            return Err(ResolveError::Missing {
                package: name.clone(),
            });
        }

        for (name, version) in assigned {
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

        for (name, constraints) in active {
            if !assigned.contains_key(name) {
                let pi = self.package_index[name];
                if !self.catalog[pi]
                    .releases
                    .iter()
                    .any(|r| matches_all(r.version, constraints))
                {
                    return Err(ResolveError::NoMatch {
                        package: name.clone(),
                        requirements: constraints.clone(),
                    });
                }
            }
        }

        Ok(active.keys().find(|name| !assigned.contains_key(*name)).cloned())
    }

    fn visit(
        &mut self,
        roots: &[Requirement],
        assigned: &mut std::collections::BTreeMap<String, Version>,
        first_error: &mut Option<ResolveError>,
    ) -> Result<(), ResolveError> {
        let active = self.active_constraints(roots, assigned);
        let next = match self.inspect(&active, assigned) {
            Ok(next) => next,
            Err(error) => {
                if first_error.is_none() {
                    *first_error = Some(error);
                }
                return Ok(());
            }
        };

        let Some(name) = next else {
            let selected: Vec<(String, Version)> =
                assigned.iter().map(|(n, v)| (n.clone(), *v)).collect();
            let cost = self
                .locks
                .iter()
                .filter(|(name, version)| {
                    assigned.get(*name).copied() != Some(**version)
                })
                .count();

            let replace = match &self.best {
                None => true,
                Some(best) if cost < self.best_cost => true,
                Some(best) if cost == self.best_cost => {
                    self.vector_cmp(&selected, &best.selected).is_gt()
                }
                _ => false,
            };
            if replace {
                self.best_cost = cost;
                self.best = Some(SearchResult {
                    selected,
                    constraints: active,
                });
            }
            return Ok(());
        };

        let constraints = &active[&name];
        let pi = self.package_index[&name];
        let mut candidates: Vec<Version> = self.catalog[pi]
            .releases
            .iter()
            .filter(|r| matches_all(r.version, constraints))
            .map(|r| r.version)
            .collect();
        candidates.sort_by(|a, b| b.cmp(a));
        candidates.dedup();

        if let Some(lock) = self.locks.get(&name) {
            if let Some(pos) = candidates.iter().position(|v| v == lock) {
                let v = candidates.remove(pos);
                candidates.insert(0, v);
            }
        }

        let mut first_child_error = None;
        for candidate in candidates {
            if self.attempts == self.max_attempts {
                return Err(ResolveError::Limit {
                    attempts: self.max_attempts,
                });
            }
            self.attempts += 1;
            assigned.insert(name.clone(), candidate);
            let result = self.visit(roots, assigned, &mut first_child_error);
            assigned.remove(&name);
            if let Err(error) = result {
                return Err(error);
            }
        }

        if self.best.is_none() && first_error.is_none() {
            if let Some(error) = first_child_error {
                *first_error = Some(error);
            }
        }
        Ok(())
    }

    fn vector_cmp(&self, a: &[(String, Version)], b: &[(String, Version)]) -> std::cmp::Ordering {
        let amap: std::collections::BTreeMap<&str, Version> =
            a.iter().map(|(n, v)| (n.as_str(), *v)).collect();
        let bmap: std::collections::BTreeMap<&str, Version> =
            b.iter().map(|(n, v)| (n.as_str(), *v)).collect();
        for name in self.package_index.keys() {
            match (amap.get(name.as_str()), bmap.get(name.as_str())) {
                (None, None) => {}
                (None, Some(_)) => return std::cmp::Ordering::Less,
                (Some(_), None) => return std::cmp::Ordering::Greater,
                (Some(av), Some(bv)) => {
                    let ord = av.cmp(bv);
                    if !ord.is_eq() {
                        return ord;
                    }
                }
            }
        }
        std::cmp::Ordering::Equal
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

    let package_index: std::collections::BTreeMap<String, usize> = catalog
        .iter()
        .enumerate()
        .map(|(i, p)| (p.name.clone(), i))
        .collect();
    let lock_map: std::collections::BTreeMap<String, Version> = locks
        .iter()
        .map(|l| (l.package.clone(), l.version))
        .collect();

    let mut search = Search {
        catalog,
        package_index,
        locks: lock_map,
        max_attempts,
        attempts: 0,
        best: None,
        best_cost: usize::MAX,
    };
    let mut assigned = std::collections::BTreeMap::new();
    let mut first_error = None;
    search.visit(roots, &mut assigned, &mut first_error)?;

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

    let after: std::collections::BTreeMap<String, Version> =
        best.selected.iter().cloned().collect();
    let mut names = std::collections::BTreeSet::new();
    names.extend(search.locks.keys().cloned());
    names.extend(after.keys().cloned());

    let mut changes = Vec::new();
    for package in names {
        let before = search.locks.get(&package).copied();
        let new_version = after.get(&package).copied();
        if before == new_version {
            continue;
        }
        let reason = match (before, new_version) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old), Some(_)) => {
                let exists = search
                    .package_index
                    .get(&package)
                    .is_some_and(|pi| {
                        search.catalog[*pi]
                            .releases
                            .iter()
                            .any(|r| r.version == old)
                    });
                if !exists {
                    ChangeReason::Unavailable
                } else if best
                    .constraints
                    .get(&package)
                    .is_some_and(|cs| !matches_all(old, cs))
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
            after: new_version,
            reason,
        });
    }

    Ok(Resolution { selected, changes })
}