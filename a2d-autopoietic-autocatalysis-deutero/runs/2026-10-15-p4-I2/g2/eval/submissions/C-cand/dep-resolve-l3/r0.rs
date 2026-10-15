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

    let catalog_map: std::collections::BTreeMap<&str, &Package> =
        catalog.iter().map(|p| (p.name.as_str(), p)).collect();
    let lock_map: std::collections::BTreeMap<&str, Version> = locks
        .iter()
        .map(|l| (l.package.as_str(), l.version))
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
    search.visit(std::collections::BTreeMap::new())?;

    if let Some((selected, constraints)) = search.best {
        let selected_vec: Vec<Selection> = selected
            .iter()
            .map(|(name, version)| Selection {
                package: name.clone(),
                version: *version,
            })
            .collect();
        let changes = make_changes(&selected, &constraints, catalog, locks);
        Ok(Resolution {
            selected: selected_vec,
            changes,
        })
    } else {
        Err(search.first_error.unwrap_or_else(|| ResolveError::NoMatch {
            package: String::new(),
            requirements: Vec::new(),
        }))
    }
}

fn valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.is_empty() || bytes.len() > 24 {
        return false;
    }
    if !bytes[0].is_ascii_lowercase() || bytes[bytes.len() - 1] == b'-' {
        return false;
    }
    bytes
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

fn constraint_matches(constraint: Constraint, version: Version) -> bool {
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

fn canonicalize(mut constraints: Vec<Constraint>) -> Vec<Constraint> {
    constraints.sort();
    constraints.dedup();
    constraints
}

fn collect_constraints(
    roots: &[Requirement],
    selected: &std::collections::BTreeMap<String, Version>,
    catalog: &std::collections::BTreeMap<&str, &Package>,
) -> std::collections::BTreeMap<String, Vec<Constraint>> {
    let mut map: std::collections::BTreeMap<String, Vec<Constraint>> =
        std::collections::BTreeMap::new();
    for root in roots {
        map.entry(root.package.clone())
            .or_default()
            .push(root.constraint);
    }
    for (name, version) in selected {
        if let Some(package) = catalog.get(name.as_str()) {
            if let Some(release) = package.releases.iter().find(|r| r.version == *version) {
                for dep in &release.deps {
                    map.entry(dep.package.clone())
                        .or_default()
                        .push(dep.constraint);
                }
            }
        }
    }
    for constraints in map.values_mut() {
        *constraints = canonicalize(std::mem::take(constraints));
    }
    map
}

struct Search<'a> {
    catalog: &'a std::collections::BTreeMap<&'a str, &'a Package>,
    roots: &'a [Requirement],
    locks: &'a std::collections::BTreeMap<&'a str, Version>,
    max_attempts: usize,
    attempts: usize,
    best: Option<(
        std::collections::BTreeMap<String, Version>,
        std::collections::BTreeMap<String, Vec<Constraint>>,
    )>,
    first_error: Option<ResolveError>,
}

impl Search<'_> {
    fn visit(
        &mut self,
        selected: std::collections::BTreeMap<String, Version>,
    ) -> Result<(), ResolveError> {
        let constraints = collect_constraints(self.roots, &selected, self.catalog);

        let missing: Vec<String> = constraints
            .keys()
            .filter(|name| !self.catalog.contains_key(name.as_str()))
            .cloned()
            .collect();
        if let Some(package) = missing.first() {
            self.record_error(ResolveError::Missing {
                package: package.clone(),
            });
            return Ok(());
        }

        for (name, version) in &selected {
            if let Some(reqs) = constraints.get(name) {
                if !reqs.iter().all(|c| constraint_matches(*c, *version)) {
                    self.record_error(ResolveError::Conflict {
                        package: name.clone(),
                        selected: *version,
                        requirements: reqs.clone(),
                    });
                    return Ok(());
                }
            }
        }

        for (name, reqs) in &constraints {
            if selected.contains_key(name) {
                continue;
            }
            let package = self.catalog[name.as_str()];
            if !package.releases.iter().any(|r| {
                reqs.iter()
                    .all(|c| constraint_matches(*c, r.version))
            }) {
                self.record_error(ResolveError::NoMatch {
                    package: name.clone(),
                    requirements: reqs.clone(),
                });
                return Ok(());
            }
        }

        let next = constraints
            .keys()
            .find(|name| !selected.contains_key(*name))
            .cloned();

        let Some(name) = next else {
            self.consider_solution(selected, constraints);
            return Ok(());
        };

        let reqs = &constraints[&name];
        let package = self.catalog[name.as_str()];
        let mut candidates: Vec<Version> = package
            .releases
            .iter()
            .filter(|r| reqs.iter().all(|c| constraint_matches(*c, r.version)))
            .map(|r| r.version)
            .collect();
        candidates.sort_by(|a, b| b.cmp(a));

        if let Some(locked) = self.locks.get(name.as_str()) {
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
            let mut child = selected.clone();
            child.insert(name.clone(), version);
            self.visit(child)?;
        }
        Ok(())
    }

    fn record_error(&mut self, error: ResolveError) {
        if self.first_error.is_none() {
            self.first_error = Some(error);
        }
    }

    fn consider_solution(
        &mut self,
        selected: std::collections::BTreeMap<String, Version>,
        constraints: std::collections::BTreeMap<String, Vec<Constraint>>,
    ) {
        let cost = lock_cost(&selected, self.locks);
        let replace = match &self.best {
            None => true,
            Some((best, _)) => {
                let best_cost = lock_cost(best, self.locks);
                if cost != best_cost {
                    cost < best_cost
                } else {
                    compare_version_vectors(&selected, best, self.catalog).is_gt()
                }
            }
        };
        if replace {
            self.best = Some((selected, constraints));
        }
    }
}

fn lock_cost(
    selected: &std::collections::BTreeMap<String, Version>,
    locks: &std::collections::BTreeMap<&str, Version>,
) -> usize {
    locks
        .iter()
        .filter(|(name, version)| selected.get(**name).copied() != Some(**version))
        .count()
}

fn compare_version_vectors(
    left: &std::collections::BTreeMap<String, Version>,
    right: &std::collections::BTreeMap<String, Version>,
    catalog: &std::collections::BTreeMap<&str, &Package>,
) -> std::cmp::Ordering {
    for name in catalog.keys() {
        let a = left.get(*name);
        let b = right.get(*name);
        let ordering = match (a, b) {
            (None, None) => std::cmp::Ordering::Equal,
            (None, Some(_)) => std::cmp::Ordering::Less,
            (Some(_), None) => std::cmp::Ordering::Greater,
            (Some(a), Some(b)) => a.cmp(b),
        };
        if ordering != std::cmp::Ordering::Equal {
            return ordering;
        }
    }
    std::cmp::Ordering::Equal
}

fn make_changes(
    selected: &std::collections::BTreeMap<String, Version>,
    constraints: &std::collections::BTreeMap<String, Vec<Constraint>>,
    catalog: &[Package],
    locks: &[Lock],
) -> Vec<Change> {
    let catalog_map: std::collections::BTreeMap<&str, &Package> =
        catalog.iter().map(|p| (p.name.as_str(), p)).collect();
    let before: std::collections::BTreeMap<&str, Version> = locks
        .iter()
        .map(|l| (l.package.as_str(), l.version))
        .collect();
    let mut names = std::collections::BTreeSet::new();
    names.extend(before.keys().copied());
    names.extend(selected.keys().map(String::as_str));

    let mut changes = Vec::new();
    for name in names {
        let old = before.get(name).copied();
        let new = selected.get(name).copied();
        if old == new {
            continue;
        }
        let reason = match (old, new) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old_version), Some(_)) => {
                let exists = catalog_map
                    .get(name)
                    .is_some_and(|p| p.releases.iter().any(|r| r.version == old_version));
                if !exists {
                    ChangeReason::Unavailable
                } else if constraints
                    .get(name)
                    .is_some_and(|reqs| !reqs.iter().all(|c| constraint_matches(*c, old_version)))
                {
                    ChangeReason::Rejected
                } else {
                    ChangeReason::Rebalanced
                }
            }
            (None, None) => continue,
        };
        changes.push(Change {
            package: name.to_owned(),
            before: old,
            after: new,
            reason,
        });
    }
    changes
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
    canonicalize(constraints.to_vec())
        .iter()
        .map(|c| constraint_text(*c))
        .collect::<Vec<_>>()
        .join(",")
}

fn encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'.' | b'_' | b'-') {
            out.push(*byte as char);
        } else {
            out.push('%');
            out.push(char::from(b"0123456789ABCDEF"[(byte >> 4) as usize]));
            out.push(char::from(b"0123456789ABCDEF"[(byte & 0x0f) as usize]));
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

fn main() {}