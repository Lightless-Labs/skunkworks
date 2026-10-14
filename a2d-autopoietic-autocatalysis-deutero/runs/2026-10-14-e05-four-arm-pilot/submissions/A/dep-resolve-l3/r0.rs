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
    (1..=24).contains(&bytes.len())
        && bytes[0].is_ascii_lowercase()
        && bytes[bytes.len() - 1] != b'-'
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-') {
            out.push(b as char);
        } else {
            out.push('%');
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 15) as usize] as char);
        }
    }
    out
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

fn active_constraints(
    roots: &[Requirement],
    selected: &[(String, Version)],
    catalog: &[Package],
) -> std::collections::BTreeMap<String, Vec<Constraint>> {
    let mut active = std::collections::BTreeMap::<String, Vec<Constraint>>::new();
    for root in roots {
        active
            .entry(root.package.clone())
            .or_default()
            .push(root.constraint);
    }
    for (name, version) in selected {
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
        *constraints = canonical(std::mem::take(constraints));
    }
    active
}

struct Search<'a> {
    catalog: &'a [Package],
    roots: &'a [Requirement],
    locks: &'a [Lock],
    attempts: usize,
    max_attempts: usize,
    best: Option<Vec<(String, Version)>>,
    best_cost: usize,
    first_failure: Option<ResolveError>,
    limit: bool,
}

impl Search<'_> {
    fn record_failure(&mut self, error: ResolveError) {
        if self.first_failure.is_none() {
            self.first_failure = Some(error);
        }
    }

    fn lock_cost(&self, selected: &[(String, Version)]) -> usize {
        self.locks
            .iter()
            .filter(|lock| {
                !selected
                    .iter()
                    .any(|(name, version)| *name == lock.package && *version == lock.version)
            })
            .count()
    }

    fn better_vector(&self, candidate: &[(String, Version)], current: &[(String, Version)]) -> bool {
        let names: std::collections::BTreeSet<&str> = self
            .catalog
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        for name in names {
            let a = candidate.iter().find(|(n, _)| n == name).map(|(_, v)| *v);
            let b = current.iter().find(|(n, _)| n == name).map(|(_, v)| *v);
            match (a, b) {
                (Some(a), Some(b)) if a != b => return a > b,
                (Some(_), None) => return true,
                (None, Some(_)) => return false,
                _ => {}
            }
        }
        false
    }

    fn visit(&mut self, selected: &mut Vec<(String, Version)>) {
        if self.limit {
            return;
        }

        let active = active_constraints(self.roots, selected, self.catalog);
        let selected_map: std::collections::BTreeMap<&str, Version> = selected
            .iter()
            .map(|(name, version)| (name.as_str(), *version))
            .collect();

        if let Some(name) = active
            .keys()
            .find(|name| !self.catalog.iter().any(|p| p.name == ***name))
        {
            self.record_failure(ResolveError::Missing {
                package: name.clone(),
            });
            return;
        }

        for (name, constraints) in &active {
            if let Some(version) = selected_map.get(name.as_str()) {
                if !constraints.iter().all(|c| matches(**version, *c)) {
                    self.record_failure(ResolveError::Conflict {
                        package: name.clone(),
                        selected: **version,
                        requirements: constraints.clone(),
                    });
                    return;
                }
            }
        }

        for (name, constraints) in &active {
            if selected_map.contains_key(name.as_str()) {
                continue;
            }
            let package = self.catalog.iter().find(|p| p.name == *name).unwrap();
            if !package
                .releases
                .iter()
                .any(|r| constraints.iter().all(|c| matches(r.version, *c)))
            {
                self.record_failure(ResolveError::NoMatch {
                    package: name.clone(),
                    requirements: constraints.clone(),
                });
                return;
            }
        }

        let next = active
            .keys()
            .find(|name| !selected_map.contains_key(name.as_str()))
            .cloned();
        let Some(name) = next else {
            let cost = self.lock_cost(selected);
            let should_replace = match &self.best {
                None => true,
                Some(current) if cost < self.best_cost => true,
                Some(current) if cost == self.best_cost => self.better_vector(selected, current),
                _ => false,
            };
            if should_replace {
                self.best = Some(selected.clone());
                self.best_cost = cost;
            }
            return;
        };

        let constraints = active.get(&name).unwrap();
        let package = self.catalog.iter().find(|p| p.name == name).unwrap();
        let mut candidates: Vec<Version> = package
            .releases
            .iter()
            .filter(|r| constraints.iter().all(|c| matches(r.version, *c)))
            .map(|r| r.version)
            .collect();
        candidates.sort_by(|a, b| b.cmp(a));
        candidates.dedup();

        if let Some(lock) = self.locks.iter().find(|l| l.package == name) {
            if let Some(pos) = candidates.iter().position(|v| *v == lock.version) {
                let version = candidates.remove(pos);
                candidates.insert(0, version);
            }
        }

        for version in candidates {
            if self.attempts == self.max_attempts {
                self.limit = true;
                return;
            }
            self.attempts += 1;
            selected.push((name.clone(), version));
            self.visit(selected);
            selected.pop();
            if self.limit {
                return;
            }
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

    let mut seen_packages = std::collections::BTreeSet::new();
    for package in catalog {
        if !seen_packages.insert(package.name.as_str()) {
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
                site: format!("lock[{i}]"),
                name: lock.package.clone(),
            });
        }
    }
    let mut seen_locks = std::collections::BTreeSet::new();
    for lock in locks {
        if !seen_locks.insert(lock.package.as_str()) {
            return Err(ResolveError::DuplicateLock {
                package: lock.package.clone(),
            });
        }
    }

    let mut search = Search {
        catalog,
        roots,
        locks,
        attempts: 0,
        max_attempts,
        best: None,
        best_cost: usize::MAX,
        first_failure: None,
        limit: false,
    };
    let mut selected = Vec::new();
    search.visit(&mut selected);

    if search.limit {
        return Err(ResolveError::Limit {
            attempts: max_attempts,
        });
    }

    let Some(mut solution) = search.best else {
        return Err(search.first_failure.unwrap_or(ResolveError::NoMatch {
            package: String::new(),
            requirements: Vec::new(),
        }));
    };
    solution.sort_by(|a, b| a.0.cmp(&b.0));

    let final_active = active_constraints(roots, &solution, catalog);
    let selected_out: Vec<Selection> = solution
        .iter()
        .map(|(package, version)| Selection {
            package: package.clone(),
            version: *version,
        })
        .collect();

    let before_map: std::collections::BTreeMap<&str, Version> = locks
        .iter()
        .map(|lock| (lock.package.as_str(), lock.version))
        .collect();
    let after_map: std::collections::BTreeMap<&str, Version> = solution
        .iter()
        .map(|(package, version)| (package.as_str(), *version))
        .collect();
    let names: std::collections::BTreeSet<&str> = before_map
        .keys()
        .chain(after_map.keys())
        .copied()
        .collect();

    let mut changes = Vec::new();
    for name in names {
        let before = before_map.get(name).copied();
        let after = after_map.get(name).copied();
        if before == after {
            continue;
        }
        let reason = match (before, after) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old), Some(_)) => {
                let exists = catalog
                    .iter()
                    .find(|p| p.name == name)
                    .map(|p| p.releases.iter().any(|r| r.version == old))
                    .unwrap_or(false);
                if !exists {
                    ChangeReason::Unavailable
                } else if !final_active
                    .get(name)
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
            package: name.to_string(),
            before,
            after,
            reason,
        });
    }

    Ok(Resolution {
        selected: selected_out,
        changes,
    })
}