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

fn constraint_texts(constraints: Vec<Constraint>) -> String {
    canonical(constraints)
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
        ResolveError::Missing { package } => format!("missing|package={}", encode(package)),
        ResolveError::NoMatch {
            package,
            requirements,
        } => format!(
            "no-match|package={}|constraints={}",
            encode(package),
            constraint_texts(requirements.clone())
        ),
        ResolveError::Conflict {
            package,
            selected,
            requirements,
        } => format!(
            "conflict|package={}|selected={}|constraints={}",
            encode(package),
            version_text(*selected),
            constraint_texts(requirements.clone())
        ),
        ResolveError::Limit { attempts } => format!("limit|attempts={}", attempts),
    }
}

type Assignment = std::collections::BTreeMap<String, Version>;
type Active = std::collections::BTreeMap<String, Vec<Constraint>>;

struct Search<'a> {
    catalog: &'a std::collections::BTreeMap<String, &'a Package>,
    roots: &'a [Requirement],
    locks: &'a std::collections::BTreeMap<String, Version>,
    budget: usize,
    attempts: usize,
    best: Option<(usize, Vec<Option<Version>>, Assignment, Active)>,
    first_error: Option<ResolveError>,
    names: Vec<String>,
}

impl<'a> Search<'a> {
    fn active(&self, assigned: &Assignment) -> Active {
        let mut active = Active::new();
        for root in self.roots {
            active
                .entry(root.package.clone())
                .or_default()
                .push(root.constraint);
        }
        for (name, version) in assigned {
            if let Some(pkg) = self.catalog.get(name) {
                if let Some(release) = pkg.releases.iter().find(|r| r.version == *version) {
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
            *cs = canonical(std::mem::take(cs));
        }
        active
    }

    fn inspect(&mut self, assigned: &mut Assignment) -> Result<(), ResolveError> {
        let active = self.active(assigned);

        if let Some(name) = active.keys().find(|name| !self.catalog.contains_key(*name)) {
            return Err(ResolveError::Missing {
                package: name.clone(),
            });
        }

        for (name, version) in assigned.iter() {
            if let Some(cs) = active.get(name) {
                if !cs.iter().all(|c| matches(*version, *c)) {
                    return Err(ResolveError::Conflict {
                        package: name.clone(),
                        selected: *version,
                        requirements: cs.clone(),
                    });
                }
            }
        }

        for (name, cs) in &active {
            if assigned.contains_key(name) {
                continue;
            }
            let pkg = self.catalog[name];
            if !pkg
                .releases
                .iter()
                .any(|r| cs.iter().all(|c| matches(r.version, *c)))
            {
                return Err(ResolveError::NoMatch {
                    package: name.clone(),
                    requirements: cs.clone(),
                });
            }
        }

        let next = active.keys().find(|n| !assigned.contains_key(*n)).cloned();
        let Some(name) = next else {
            let cost = self
                .locks
                .iter()
                .filter(|(n, v)| assigned.get(*n) != Some(*v))
                .count();
            let vector = self
                .names
                .iter()
                .map(|n| assigned.get(n).copied())
                .collect::<Vec<_>>();
            let replace = self.best.as_ref().is_none_or(|(best_cost, best_vector, _, _)| {
                cost < *best_cost || (cost == *best_cost && vector > *best_vector)
            });
            if replace {
                self.best = Some((cost, vector, assigned.clone(), active));
            }
            return Ok(());
        };

        let mut candidates: Vec<Version> = self.catalog[&name]
            .releases
            .iter()
            .filter(|r| active[&name].iter().all(|c| matches(r.version, *c)))
            .map(|r| r.version)
            .collect();
        candidates.sort_by(|a, b| b.cmp(a));

        if let Some(locked) = self.locks.get(&name) {
            if let Some(i) = candidates.iter().position(|v| v == locked) {
                let v = candidates.remove(i);
                candidates.insert(0, v);
            }
        }

        for version in candidates {
            if self.attempts == self.budget {
                return Err(ResolveError::Limit {
                    attempts: self.budget,
                });
            }
            self.attempts += 1;
            assigned.insert(name.clone(), version);
            match self.inspect(assigned) {
                Ok(()) => {}
                Err(ResolveError::Limit { attempts }) => {
                    assigned.remove(&name);
                    return Err(ResolveError::Limit { attempts });
                }
                Err(e) => {
                    if self.first_error.is_none() {
                        self.first_error = Some(e);
                    }
                }
            }
            assigned.remove(&name);
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
    for (i, pkg) in catalog.iter().enumerate() {
        if !valid_name(&pkg.name) {
            return Err(ResolveError::InvalidName {
                site: format!("catalog[{}]", i),
                name: pkg.name.clone(),
            });
        }
        for (j, release) in pkg.releases.iter().enumerate() {
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

    let mut seen = std::collections::BTreeSet::new();
    for pkg in catalog {
        if !seen.insert(pkg.name.clone()) {
            return Err(ResolveError::DuplicatePackage {
                package: pkg.name.clone(),
            });
        }
    }
    for pkg in catalog {
        let mut versions = std::collections::BTreeSet::new();
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
                site: format!("lock[{}]", i),
                name: lock.package.clone(),
            });
        }
    }
    let mut lock_map = std::collections::BTreeMap::new();
    for lock in locks {
        if lock_map.insert(lock.package.clone(), lock.version).is_some() {
            return Err(ResolveError::DuplicateLock {
                package: lock.package.clone(),
            });
        }
    }

    let cat: std::collections::BTreeMap<String, &Package> =
        catalog.iter().map(|p| (p.name.clone(), p)).collect();
    let mut names = catalog.iter().map(|p| p.name.clone()).collect::<Vec<_>>();
    names.sort();
    let mut search = Search {
        catalog: &cat,
        roots,
        locks: &lock_map,
        budget: max_attempts,
        attempts: 0,
        best: None,
        first_error: None,
        names,
    };
    search.inspect(&mut Assignment::new())?;

    let Some((_, _, assignment, active)) = search.best else {
        return Err(search.first_error.unwrap_or(ResolveError::NoMatch {
            package: roots
                .first()
                .map(|r| r.package.clone())
                .unwrap_or_default(),
            requirements: Vec::new(),
        }));
    };

    let selected = assignment
        .iter()
        .map(|(package, version)| Selection {
            package: package.clone(),
            version: *version,
        })
        .collect::<Vec<_>>();

    let mut all_names = std::collections::BTreeSet::new();
    all_names.extend(lock_map.keys().cloned());
    all_names.extend(assignment.keys().cloned());
    let mut changes = Vec::new();
    for package in all_names {
        let before = lock_map.get(&package).copied();
        let after = assignment.get(&package).copied();
        if before == after {
            continue;
        }
        let reason = match (before, after) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old), Some(_)) => {
                let exists = cat
                    .get(&package)
                    .is_some_and(|p| p.releases.iter().any(|r| r.version == old));
                if !exists {
                    ChangeReason::Unavailable
                } else if active
                    .get(&package)
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
            package,
            before,
            after,
            reason,
        });
    }

    Ok(Resolution { selected, changes })
}