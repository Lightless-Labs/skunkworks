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
    constraint_list_text(&canonical(cs.to_vec()))
}

fn constraint_list_text(cs: &[Constraint]) -> String {
    cs.iter()
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

type Active = std::collections::BTreeMap<String, Vec<Constraint>>;
type Assigned = std::collections::BTreeMap<String, Version>;

fn active_constraints(roots: &[Requirement], assigned: &Assigned, catalog: &std::collections::BTreeMap<String, &Package>) -> Active {
    let mut active = Active::new();
    for req in roots {
        active
            .entry(req.package.clone())
            .or_default()
            .push(req.constraint);
    }
    for (name, version) in assigned {
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

fn search(
    roots: &[Requirement],
    catalog: &std::collections::BTreeMap<String, &Package>,
    locks: &std::collections::BTreeMap<String, Version>,
    assigned: &mut Assigned,
    attempts: &mut usize,
    max_attempts: usize,
    best: &mut Option<(usize, Vec<Selection>, Active)>,
    first_error: &mut Option<ResolveError>,
) -> Result<(), ResolveError> {
    let active = active_constraints(roots, assigned, catalog);

    let missing = active
        .keys()
        .filter(|name| !catalog.contains_key(*name))
        .next();
    if let Some(name) = missing {
        let e = ResolveError::Missing {
            package: name.clone(),
        };
        if first_error.is_none() {
            *first_error = Some(e);
        }
        return Ok(());
    }

    for (name, version) in assigned.iter() {
        if let Some(cs) = active.get(name) {
            if !cs.iter().all(|c| matches(*version, *c)) {
                let e = ResolveError::Conflict {
                    package: name.clone(),
                    selected: *version,
                    requirements: cs.clone(),
                };
                if first_error.is_none() {
                    *first_error = Some(e);
                }
                return Ok(());
            }
        }
    }

    for (name, cs) in &active {
        if !assigned.contains_key(name) {
            let pkg = catalog[name];
            if !pkg.releases.iter().any(|r| cs.iter().all(|c| matches(r.version, *c))) {
                let e = ResolveError::NoMatch {
                    package: name.clone(),
                    requirements: cs.clone(),
                };
                if first_error.is_none() {
                    *first_error = Some(e);
                }
                return Ok(());
            }
        }
    }

    let next = active.keys().find(|name| !assigned.contains_key(*name)).cloned();
    if let Some(name) = next {
        let pkg = catalog[&name];
        let cs = &active[&name];
        let mut candidates: Vec<Version> = pkg
            .releases
            .iter()
            .map(|r| r.version)
            .filter(|v| cs.iter().all(|c| matches(*v, *c)))
            .collect();
        candidates.sort_by(|a, b| b.cmp(a));
        candidates.dedup();
        if let Some(lock) = locks.get(&name) {
            if let Some(pos) = candidates.iter().position(|v| v == lock) {
                let v = candidates.remove(pos);
                candidates.insert(0, v);
            }
        }
        for version in candidates {
            if *attempts == max_attempts {
                return Err(ResolveError::Limit {
                    attempts: max_attempts,
                });
            }
            *attempts += 1;
            assigned.insert(name.clone(), version);
            search(
                roots, catalog, locks, assigned, attempts, max_attempts, best, first_error,
            )?;
            assigned.remove(&name);
        }
        return Ok(());
    }

    let selected: Vec<Selection> = assigned
        .iter()
        .map(|(package, version)| Selection {
            package: package.clone(),
            version: *version,
        })
        .collect();
    let cost = locks
        .iter()
        .filter(|(package, version)| {
            assigned.get(*package).copied() != Some(**version)
        })
        .count();
    let better = match best {
        None => true,
        Some((best_cost, best_selection, _)) => {
            if cost != *best_cost {
                cost < *best_cost
            } else {
                let names: std::collections::BTreeSet<String> = catalog.keys().cloned().collect();
                let current: std::collections::BTreeMap<&str, Version> = selected
                    .iter()
                    .map(|s| (s.package.as_str(), s.version))
                    .collect();
                let previous: std::collections::BTreeMap<&str, Version> = best_selection
                    .iter()
                    .map(|s| (s.package.as_str(), s.version))
                    .collect();
                let mut cmp = std::cmp::Ordering::Equal;
                for n in names {
                    let a = current.get(n.as_str());
                    let b = previous.get(n.as_str());
                    cmp = match (a, b) {
                        (None, None) => std::cmp::Ordering::Equal,
                        (None, Some(_)) => std::cmp::Ordering::Less,
                        (Some(_), None) => std::cmp::Ordering::Greater,
                        (Some(a), Some(b)) => a.cmp(b),
                    };
                    if cmp != std::cmp::Ordering::Equal {
                        break;
                    }
                }
                cmp == std::cmp::Ordering::Greater
            }
        }
    };
    if better {
        *best = Some((cost, selected, active));
    }
    Ok(())
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
    let mut seen_locks = std::collections::BTreeSet::new();
    for lock in locks {
        if !seen_locks.insert(lock.package.clone()) {
            return Err(ResolveError::DuplicateLock {
                package: lock.package.clone(),
            });
        }
    }

    let catalog_map: std::collections::BTreeMap<String, &Package> = catalog
        .iter()
        .map(|p| (p.name.clone(), p))
        .collect();
    let lock_map: std::collections::BTreeMap<String, Version> = locks
        .iter()
        .map(|l| (l.package.clone(), l.version))
        .collect();
    let mut attempts = 0usize;
    let mut assigned = Assigned::new();
    let mut best = None;
    let mut first_error = None;
    search(
        roots,
        &catalog_map,
        &lock_map,
        &mut assigned,
        &mut attempts,
        max_attempts,
        &mut best,
        &mut first_error,
    )?;

    let Some((_, selected, final_active)) = best else {
        return Err(first_error.unwrap_or_else(|| ResolveError::NoMatch {
            package: String::new(),
            requirements: Vec::new(),
        }));
    };

    let selected_map: std::collections::BTreeMap<String, Version> = selected
        .iter()
        .map(|s| (s.package.clone(), s.version))
        .collect();
    let mut names = std::collections::BTreeSet::new();
    for lock in locks {
        names.insert(lock.package.clone());
    }
    for selection in &selected {
        names.insert(selection.package.clone());
    }

    let mut changes = Vec::new();
    for name in names {
        let before = lock_map.get(&name).copied();
        let after = selected_map.get(&name).copied();
        if before == after {
            continue;
        }
        let reason = match (before, after) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old), Some(_)) => {
                let exists = catalog_map
                    .get(&name)
                    .map(|p| p.releases.iter().any(|r| r.version == old))
                    .unwrap_or(false);
                if !exists {
                    ChangeReason::Unavailable
                } else if !final_active
                    .get(&name)
                    .map(|cs| cs.iter().all(|c| matches(old, *c)))
                    .unwrap_or(true)
                {
                    ChangeReason::Rejected
                } else {
                    ChangeReason::Rebalanced
                }
            }
            (None, None) => unreachable!(),
        };
        changes.push(Change {
            package: name,
            before,
            after,
            reason,
        });
    }

    Ok(Resolution { selected, changes })
}