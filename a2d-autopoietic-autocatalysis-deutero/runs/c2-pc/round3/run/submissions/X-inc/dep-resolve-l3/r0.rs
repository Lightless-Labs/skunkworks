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

fn canonical_constraints(constraints: &[Constraint]) -> Vec<Constraint> {
    let mut result = constraints.to_vec();
    result.sort();
    result.dedup();
    result
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
            canonical_constraints(requirements)
                .iter()
                .map(|c| constraint_text(*c))
                .collect::<Vec<_>>()
                .join(",")
        ),
        ResolveError::Conflict {
            package,
            selected,
            requirements,
        } => format!(
            "conflict|package={}|selected={}|constraints={}",
            encode(package),
            version_text(*selected),
            canonical_constraints(requirements)
                .iter()
                .map(|c| constraint_text(*c))
                .collect::<Vec<_>>()
                .join(",")
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

fn satisfies(version: Version, constraints: &[Constraint]) -> bool {
    constraints
        .iter()
        .all(|constraint| matches_constraint(version, *constraint))
}

type Active = std::collections::BTreeMap<String, Vec<Constraint>>;
type Chosen = std::collections::BTreeMap<String, Version>;

fn active_constraints(roots: &[Requirement], chosen: &Chosen, catalog: &[Package]) -> Active {
    let mut active = Active::new();
    for root in roots {
        active
            .entry(root.package.clone())
            .or_default()
            .push(root.constraint);
    }
    for (name, version) in chosen {
        if let Some(package) = catalog.iter().find(|p| p.name == *name) {
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

fn candidates<'a>(
    package: &'a Package,
    constraints: &[Constraint],
    lock: Option<Version>,
) -> Vec<&'a Release> {
    let mut result: Vec<&Release> = package
        .releases
        .iter()
        .filter(|release| satisfies(release.version, constraints))
        .collect();
    result.sort_by(|a, b| b.version.cmp(&a.version));
    if let Some(lock) = lock {
        if let Some(index) = result.iter().position(|r| r.version == lock) {
            let release = result.remove(index);
            result.insert(0, release);
        }
    }
    result
}

fn inspect_node(
    catalog: &[Package],
    active: &Active,
    chosen: &Chosen,
) -> Result<(), ResolveError> {
    if let Some(name) = active.keys().find(|name| !catalog.iter().any(|p| p.name == ***name)) {
        return Err(ResolveError::Missing {
            package: name.clone(),
        });
    }

    for (name, version) in chosen {
        if let Some(constraints) = active.get(name) {
            if !satisfies(*version, constraints) {
                return Err(ResolveError::Conflict {
                    package: name.clone(),
                    selected: *version,
                    requirements: constraints.clone(),
                });
            }
        }
    }

    for (name, constraints) in active {
        if !chosen.contains_key(name) {
            let package = catalog.iter().find(|p| p.name == *name).unwrap();
            if candidates(package, constraints, None).is_empty() {
                return Err(ResolveError::NoMatch {
                    package: name.clone(),
                    requirements: constraints.clone(),
                });
            }
        }
    }
    Ok(())
}

struct Search<'a> {
    catalog: &'a [Package],
    roots: &'a [Requirement],
    locks: &'a [Lock],
    max_attempts: usize,
    attempts: usize,
    best: Option<(usize, Chosen)>,
    first_error: Option<ResolveError>,
}

impl Search<'_> {
    fn search(&mut self, chosen: &mut Chosen) -> Result<(), ResolveError> {
        let active = active_constraints(self.roots, chosen, self.catalog);
        if let Err(error) = inspect_node(self.catalog, &active, chosen) {
            if self.first_error.is_none() {
                self.first_error = Some(error);
            }
            return Ok(());
        }

        let next = active.keys().find(|name| !chosen.contains_key(*name)).cloned();
        let Some(name) = next else {
            let cost = self
                .locks
                .iter()
                .filter(|lock| chosen.get(&lock.package) != Some(&lock.version))
                .count();
            let replace = match &self.best {
                None => true,
                Some((best_cost, best_chosen)) if cost < *best_cost => true,
                Some((best_cost, best_chosen)) if cost == *best_cost => {
                    compare_vectors(chosen, best_chosen, self.catalog) == std::cmp::Ordering::Greater
                }
                _ => false,
            };
            if replace {
                self.best = Some((cost, chosen.clone()));
            }
            return Ok(());
        };

        let package = self.catalog.iter().find(|p| p.name == name).unwrap();
        let lock = self
            .locks
            .iter()
            .find(|lock| lock.package == name)
            .map(|lock| lock.version);
        let options = candidates(package, &active[&name], lock);

        for release in options {
            if self.attempts == self.max_attempts {
                return Err(ResolveError::Limit {
                    attempts: self.max_attempts,
                });
            }
            self.attempts += 1;
            chosen.insert(name.clone(), release.version);
            self.search(chosen)?;
            chosen.remove(&name);
        }
        Ok(())
    }
}

fn compare_vectors(a: &Chosen, b: &Chosen, catalog: &[Package]) -> std::cmp::Ordering {
    let mut names: Vec<&str> = catalog.iter().map(|p| p.name.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    for name in names {
        match (a.get(name), b.get(name)) {
            (None, None) => {}
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(av), Some(bv)) if av != bv => return av.cmp(bv),
            _ => {}
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

    let mut package_names = std::collections::BTreeSet::new();
    for package in catalog {
        if !package_names.insert(package.name.clone()) {
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
                site: format!("lock[{}]", i),
                name: lock.package.clone(),
            });
        }
    }
    let mut lock_names = std::collections::BTreeSet::new();
    for lock in locks {
        if !lock_names.insert(lock.package.clone()) {
            return Err(ResolveError::DuplicateLock {
                package: lock.package.clone(),
            });
        }
    }

    let mut search = Search {
        catalog,
        roots,
        locks,
        max_attempts,
        attempts: 0,
        best: None,
        first_error: None,
    };
    search.search(&mut Chosen::new())?;
    let Some((_, chosen)) = search.best else {
        return Err(search.first_error.unwrap_or_else(|| ResolveError::NoMatch {
            package: String::new(),
            requirements: Vec::new(),
        }));
    };

    let final_active = active_constraints(roots, &chosen, catalog);
    let selected: Vec<Selection> = chosen
        .iter()
        .map(|(package, version)| Selection {
            package: package.clone(),
            version: *version,
        })
        .collect();

    let mut before = std::collections::BTreeMap::new();
    for lock in locks {
        before.insert(lock.package.clone(), lock.version);
    }
    let mut all_names: std::collections::BTreeSet<String> = before.keys().cloned().collect();
    all_names.extend(chosen.keys().cloned());

    let mut changes = Vec::new();
    for package in all_names {
        let old = before.get(&package).copied();
        let new = chosen.get(&package).copied();
        if old == new {
            continue;
        }
        let reason = match (old, new) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old_version), Some(_)) => {
                let exists = catalog
                    .iter()
                    .find(|p| p.name == package)
                    .is_some_and(|p| p.releases.iter().any(|r| r.version == old_version));
                if !exists {
                    ChangeReason::Unavailable
                } else if final_active
                    .get(&package)
                    .is_some_and(|constraints| !satisfies(old_version, constraints))
                {
                    ChangeReason::Rejected
                } else {
                    ChangeReason::Rebalanced
                }
            }
            (None, None) => unreachable!(),
        };
        changes.push(Change {
            package,
            before: old,
            after: new,
            reason,
        });
    }

    Ok(Resolution { selected, changes })
}