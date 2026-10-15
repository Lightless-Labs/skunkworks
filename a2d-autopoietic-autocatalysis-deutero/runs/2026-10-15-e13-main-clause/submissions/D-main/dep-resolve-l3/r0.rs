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
    bytes.iter().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

fn version_text(v: Version) -> String {
    format!("{}.{}.{}", v.major, v.minor, v.patch)
}

fn constraint_rank(c: &Constraint) -> u8 {
    match c {
        Constraint::Exact(_) => 0,
        Constraint::AtLeast(_) => 1,
        Constraint::Before(_) => 2,
        Constraint::Caret(_) => 3,
    }
}

fn constraint_version(c: &Constraint) -> Version {
    match c {
        Constraint::Exact(v)
        | Constraint::AtLeast(v)
        | Constraint::Before(v)
        | Constraint::Caret(v) => *v,
    }
}

fn canonicalize(mut cs: Vec<Constraint>) -> Vec<Constraint> {
    cs.sort_by(|a, b| {
        constraint_rank(a)
            .cmp(&constraint_rank(b))
            .then_with(|| constraint_version(a).cmp(&constraint_version(b)))
    });
    cs.dedup();
    cs
}

fn constraint_text(c: &Constraint) -> String {
    match c {
        Constraint::Exact(v) => format!("={}", version_text(*v)),
        Constraint::AtLeast(v) => format!(">={}", version_text(*v)),
        Constraint::Before(v) => format!("<{}", version_text(*v)),
        Constraint::Caret(v) => format!("^{}", version_text(*v)),
    }
}

fn constraints_text(cs: &[Constraint]) -> String {
    canonicalize(cs.to_vec())
        .iter()
        .map(constraint_text)
        .collect::<Vec<_>>()
        .join(",")
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

pub fn error_text(error: &ResolveError) -> String {
    match error {
        ResolveError::InvalidName { site, name } => {
            format!("invalid-name|site={}|name={}", percent_encode(site), percent_encode(name))
        }
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
        ResolveError::NoMatch { package, requirements } => format!(
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

fn permits(c: Constraint, v: Version) -> bool {
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

fn matches_all(v: Version, cs: &[Constraint]) -> bool {
    cs.iter().all(|c| permits(*c, v))
}

fn active_constraints(
    roots: &[Requirement],
    catalog: &[Package],
    assigned: &std::collections::BTreeMap<String, Version>,
) -> std::collections::BTreeMap<String, Vec<Constraint>> {
    let mut active = std::collections::BTreeMap::<String, Vec<Constraint>>::new();
    for root in roots {
        active.entry(root.package.clone()).or_default().push(root.constraint);
    }
    for (name, version) in assigned {
        if let Some(pkg) = catalog.iter().find(|p| p.name == *name) {
            if let Some(release) = pkg.releases.iter().find(|r| r.version == *version) {
                for dep in &release.deps {
                    active.entry(dep.package.clone()).or_default().push(dep.constraint);
                }
            }
        }
    }
    for cs in active.values_mut() {
        *cs = canonicalize(std::mem::take(cs));
    }
    active
}

struct Search<'a> {
    catalog: &'a [Package],
    roots: &'a [Requirement],
    locks: &'a [Lock],
    max_attempts: usize,
    attempts: usize,
    first_error: Option<ResolveError>,
    best: Option<(usize, Vec<Selection>, std::collections::BTreeMap<String, Version>)>,
    limit: bool,
}

impl<'a> Search<'a> {
    fn inspect(&mut self, assigned: &mut std::collections::BTreeMap<String, Version>) {
        if self.limit {
            return;
        }

        let active = active_constraints(self.roots, self.catalog, assigned);

        if let Some(name) = active
            .keys()
            .filter(|name| !self.catalog.iter().any(|p| p.name == ***name))
            .next()
        {
            self.remember_error(ResolveError::Missing {
                package: name.clone(),
            });
            return;
        }

        for (name, version) in assigned.iter() {
            if let Some(cs) = active.get(name) {
                if !matches_all(*version, cs) {
                    self.remember_error(ResolveError::Conflict {
                        package: name.clone(),
                        selected: *version,
                        requirements: cs.clone(),
                    });
                    return;
                }
            }
        }

        for (name, cs) in &active {
            if !assigned.contains_key(name) {
                let pkg = self.catalog.iter().find(|p| p.name == *name).unwrap();
                if !pkg.releases.iter().any(|r| matches_all(r.version, cs)) {
                    self.remember_error(ResolveError::NoMatch {
                        package: name.clone(),
                        requirements: cs.clone(),
                    });
                    return;
                }
            }
        }

        let next = active.keys().find(|name| !assigned.contains_key(*name)).cloned();
        let Some(name) = next else {
            let mut selected = assigned
                .iter()
                .map(|(package, version)| Selection {
                    package: package.clone(),
                    version: *version,
                })
                .collect::<Vec<_>>();
            selected.sort_by(|a, b| a.package.cmp(&b.package));
            let cost = self
                .locks
                .iter()
                .filter(|lock| assigned.get(&lock.package) != Some(&lock.version))
                .count();
            let replace = match &self.best {
                None => true,
                Some((best_cost, _, best_map)) => {
                    if cost < *best_cost {
                        true
                    } else if cost > *best_cost {
                        false
                    } else {
                        self.vector_cmp(assigned, best_map).is_gt()
                    }
                }
            };
            if replace {
                self.best = Some((cost, std::mem::take(&mut selected), assigned.clone()));
            }
            return;
        };

        let pkg = self.catalog.iter().find(|p| p.name == name).unwrap();
        let cs = active.get(&name).unwrap();
        let mut candidates = pkg
            .releases
            .iter()
            .filter(|r| matches_all(r.version, cs))
            .map(|r| r.version)
            .collect::<Vec<_>>();
        candidates.sort_by(|a, b| b.cmp(a));
        if let Some(lock) = self.locks.iter().find(|l| l.package == name) {
            if let Some(pos) = candidates.iter().position(|v| *v == lock.version) {
                let v = candidates.remove(pos);
                candidates.insert(0, v);
            }
        }

        for version in candidates {
            if self.attempts == self.max_attempts {
                self.limit = true;
                return;
            }
            self.attempts += 1;
            assigned.insert(name.clone(), version);
            self.inspect(assigned);
            assigned.remove(&name);
            if self.limit {
                return;
            }
        }
    }

    fn remember_error(&mut self, error: ResolveError) {
        if self.first_error.is_none() {
            self.first_error = Some(error);
        }
    }

    fn vector_cmp(
        &self,
        a: &std::collections::BTreeMap<String, Version>,
        b: &std::collections::BTreeMap<String, Version>,
    ) -> std::cmp::Ordering {
        let mut names = self.catalog.iter().map(|p| p.name.as_str()).collect::<Vec<_>>();
        names.sort();
        names.dedup();
        for name in names {
            let ord = match (a.get(name), b.get(name)) {
                (None, None) => std::cmp::Ordering::Equal,
                (None, Some(_)) => std::cmp::Ordering::Less,
                (Some(_), None) => std::cmp::Ordering::Greater,
                (Some(x), Some(y)) => x.cmp(y),
            };
            if ord != std::cmp::Ordering::Equal {
                return ord;
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

    let mut search = Search {
        catalog,
        roots,
        locks,
        max_attempts,
        attempts: 0,
        first_error: None,
        best: None,
        limit: false,
    };
    let mut assigned = std::collections::BTreeMap::new();
    search.inspect(&mut assigned);

    if search.limit {
        return Err(ResolveError::Limit {
            attempts: max_attempts,
        });
    }

    let Some((_, selected, selected_map)) = search.best else {
        return Err(search.first_error.unwrap_or(ResolveError::NoMatch {
            package: String::new(),
            requirements: Vec::new(),
        }));
    };

    let final_active = active_constraints(roots, catalog, &selected_map);
    let mut changes = Vec::new();
    for lock in locks {
        let after = selected_map.get(&lock.package).copied();
        if after != Some(lock.version) {
            let reason = match after {
                None => ChangeReason::Removed,
                Some(_) => {
                    let exists = catalog
                        .iter()
                        .find(|p| p.name == lock.package)
                        .map(|p| p.releases.iter().any(|r| r.version == lock.version))
                        .unwrap_or(false);
                    if !exists {
                        ChangeReason::Unavailable
                    } else if final_active
                        .get(&lock.package)
                        .map(|cs| !matches_all(lock.version, cs))
                        .unwrap_or(false)
                    {
                        ChangeReason::Rejected
                    } else {
                        ChangeReason::Rebalanced
                    }
                }
            };
            changes.push(Change {
                package: lock.package.clone(),
                before: Some(lock.version),
                after,
                reason,
            });
        }
    }
    for selection in &selected {
        if !locks.iter().any(|l| l.package == selection.package) {
            changes.push(Change {
                package: selection.package.clone(),
                before: None,
                after: Some(selection.version),
                reason: ChangeReason::Added,
            });
        }
    }
    changes.sort_by(|a, b| a.package.cmp(&b.package));

    Ok(Resolution { selected, changes })
}

fn main() {}