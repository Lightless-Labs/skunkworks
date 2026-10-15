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
    if !(1..=24).contains(&bytes.len())
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

fn canonical(mut constraints: Vec<Constraint>) -> Vec<Constraint> {
    constraints.sort();
    constraints.dedup();
    constraints
}

fn percent_encode(s: &str) -> String {
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

fn constraints_text(cs: &[Constraint]) -> String {
    canonical(cs.to_vec())
        .iter()
        .map(|c| constraint_text(*c))
        .collect::<Vec<_>>()
        .join(",")
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

fn matches_constraint(v: Version, c: Constraint) -> bool {
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

fn matches_all(v: Version, cs: &[Constraint]) -> bool {
    cs.iter().all(|c| matches_constraint(v, *c))
}

fn package_index(catalog: &[Package], name: &str) -> Option<usize> {
    catalog.iter().position(|p| p.name == name)
}

type Active = std::collections::BTreeMap<String, Vec<Constraint>>;
type Assigned = std::collections::BTreeMap<String, Version>;

fn active_constraints(roots: &[Requirement], assigned: &Assigned, catalog: &[Package]) -> Active {
    let mut active = Active::new();
    for r in roots {
        active
            .entry(r.package.clone())
            .or_default()
            .push(r.constraint);
    }
    for (name, version) in assigned {
        if let Some(pi) = package_index(catalog, name) {
            if let Some(release) = catalog[pi].releases.iter().find(|r| r.version == *version) {
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

#[derive(Clone)]
struct Best {
    cost: usize,
    vector: Vec<Option<Version>>,
    selected: Assigned,
    active: Active,
}

struct Search<'a> {
    catalog: &'a [Package],
    roots: &'a [Requirement],
    locks: &'a [Lock],
    max_attempts: usize,
    attempts: usize,
    names: Vec<String>,
    first_error: Option<ResolveError>,
    best: Option<Best>,
}

impl Search<'_> {
    fn record_leaf_error(&mut self, error: ResolveError) {
        if self.first_error.is_none() {
            self.first_error = Some(error);
        }
    }

    fn inspect(&mut self, assigned: &mut Assigned) -> Result<(), ResolveError> {
        let active = active_constraints(self.roots, assigned, self.catalog);

        let missing: Vec<_> = active
            .keys()
            .filter(|name| package_index(self.catalog, name).is_none())
            .cloned()
            .collect();
        if let Some(package) = missing.into_iter().next() {
            let e = ResolveError::Missing { package };
            self.record_leaf_error(e.clone());
            return Ok(());
        }

        for (name, version) in assigned.iter() {
            if let Some(cs) = active.get(name) {
                if !matches_all(*version, cs) {
                    let e = ResolveError::Conflict {
                        package: name.clone(),
                        selected: *version,
                        requirements: cs.clone(),
                    };
                    self.record_leaf_error(e);
                    return Ok(());
                }
            }
        }

        let mut candidates_by_name = std::collections::BTreeMap::new();
        for name in active.keys() {
            if assigned.contains_key(name) {
                continue;
            }
            let pi = package_index(self.catalog, name).unwrap();
            let mut releases: Vec<Version> = self.catalog[pi]
                .releases
                .iter()
                .filter(|r| matches_all(r.version, active.get(name).unwrap()))
                .map(|r| r.version)
                .collect();
            releases.sort_by(|a, b| b.cmp(a));
            releases.dedup();
            if releases.is_empty() {
                let e = ResolveError::NoMatch {
                    package: name.clone(),
                    requirements: active.get(name).unwrap().clone(),
                };
                self.record_leaf_error(e);
                return Ok(());
            }
            if let Some(lock) = self.locks.iter().find(|l| l.package == *name) {
                if let Some(pos) = releases.iter().position(|v| *v == lock.version) {
                    let v = releases.remove(pos);
                    releases.insert(0, v);
                }
            }
            candidates_by_name.insert(name.clone(), releases);
        }

        let Some((name, candidates)) = candidates_by_name.into_iter().next() else {
            self.consider_solution(assigned, active);
            return Ok(());
        };

        for candidate in candidates {
            if self.attempts == self.max_attempts {
                return Err(ResolveError::Limit {
                    attempts: self.max_attempts,
                });
            }
            self.attempts += 1;
            assigned.insert(name.clone(), candidate);
            let result = self.inspect(assigned);
            assigned.remove(&name);
            result?;
        }
        Ok(())
    }

    fn consider_solution(&mut self, assigned: &Assigned, active: Active) {
        let cost = self
            .locks
            .iter()
            .filter(|lock| assigned.get(&lock.package) != Some(&lock.version))
            .count();
        let vector: Vec<Option<Version>> = self
            .names
            .iter()
            .map(|name| assigned.get(name).copied())
            .collect();

        let replace = match &self.best {
            None => true,
            Some(best) => {
                cost < best.cost || (cost == best.cost && vector > best.vector)
            }
        };
        if replace {
            self.best = Some(Best {
                cost,
                vector,
                selected: assigned.clone(),
                active,
            });
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

    let mut names: Vec<String> = catalog.iter().map(|p| p.name.clone()).collect();
    names.sort();
    let mut search = Search {
        catalog,
        roots,
        locks,
        max_attempts,
        attempts: 0,
        names,
        first_error: None,
        best: None,
    };
    let mut assigned = Assigned::new();
    search.inspect(&mut assigned)?;

    let best = match search.best {
        Some(best) => best,
        None => return Err(search.first_error.unwrap_or(ResolveError::Missing {
            package: String::new(),
        })),
    };

    let mut selected: Vec<Selection> = best
        .selected
        .iter()
        .map(|(package, version)| Selection {
            package: package.clone(),
            version: *version,
        })
        .collect();
    selected.sort_by(|a, b| a.package.cmp(&b.package));

    let mut before = std::collections::BTreeMap::new();
    for lock in locks {
        before.insert(lock.package.clone(), lock.version);
    }
    let mut after = std::collections::BTreeMap::new();
    for (package, version) in &best.selected {
        after.insert(package.clone(), *version);
    }

    let all_packages: std::collections::BTreeSet<String> =
        before.keys().chain(after.keys()).cloned().collect();
    let mut changes = Vec::new();
    for package in all_packages {
        let b = before.get(&package).copied();
        let a = after.get(&package).copied();
        if b == a {
            continue;
        }
        let reason = match (b, a) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old), Some(_)) => {
                let exists = package_index(catalog, &package)
                    .map(|pi| catalog[pi].releases.iter().any(|r| r.version == old))
                    .unwrap_or(false);
                if !exists {
                    ChangeReason::Unavailable
                } else if best
                    .active
                    .get(&package)
                    .map(|cs| !matches_all(old, cs))
                    .unwrap_or(false)
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
            before: b,
            after: a,
            reason,
        });
    }

    Ok(Resolution { selected, changes })
}

fn main() {}