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

fn valid_name(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b.len() <= 24
        && b[0].is_ascii_lowercase()
        && b[b.len() - 1] != b'-'
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
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

fn canonical(mut cs: Vec<Constraint>) -> Vec<Constraint> {
    cs.sort();
    cs.dedup();
    cs
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

fn constraints_text(cs: &[Constraint]) -> String {
    canonical(cs.to_vec())
        .iter()
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

fn active_constraints(
    roots: &[Requirement],
    selected: &[(usize, usize)],
    catalog: &[Package],
) -> std::collections::BTreeMap<String, Vec<Constraint>> {
    let mut active = std::collections::BTreeMap::<String, Vec<Constraint>>::new();
    for r in roots {
        active
            .entry(r.package.clone())
            .or_default()
            .push(r.constraint);
    }
    for (pi, ri) in selected {
        for dep in &catalog[*pi].releases[*ri].deps {
            active
                .entry(dep.package.clone())
                .or_default()
                .push(dep.constraint);
        }
    }
    for cs in active.values_mut() {
        *cs = canonical(std::mem::take(cs));
    }
    active
}

struct Search<'a> {
    catalog: &'a [Package],
    roots: &'a [Requirement],
    locks: &'a [Lock],
    max_attempts: usize,
    attempts: usize,
    best: Option<(usize, Vec<(String, Version)>)>,
    first_error: Option<ResolveError>,
}

impl<'a> Search<'a> {
    fn record_error(&mut self, e: ResolveError) {
        if self.first_error.is_none() {
            self.first_error = Some(e);
        }
    }

    fn score(&self, selected: &[(usize, usize)]) -> usize {
        self.locks
            .iter()
            .filter(|l| {
                !selected.iter().any(|(pi, ri)| {
                    self.catalog[*pi].name == l.package
                        && self.catalog[*pi].releases[*ri].version == l.version
                })
            })
            .count()
    }

    fn vector(&self, selected: &[(usize, usize)]) -> Vec<(String, Version)> {
        let mut names = self
            .catalog
            .iter()
            .map(|p| p.name.clone())
            .collect::<Vec<_>>();
        names.sort();
        names.dedup();
        names
            .into_iter()
            .filter_map(|name| {
                selected.iter().find_map(|(pi, ri)| {
                    if self.catalog[*pi].name == name {
                        Some((name.clone(), self.catalog[*pi].releases[*ri].version))
                    } else {
                        None
                    }
                })
            })
            .collect()
    }

    fn vector_better(a: &[(String, Version)], b: &[(String, Version)]) -> bool {
        let mut names = a.iter().map(|x| x.0.clone()).collect::<Vec<_>>();
        names.extend(b.iter().map(|x| x.0.clone()));
        names.sort();
        names.dedup();
        for name in names {
            let av = a.iter().find(|x| x.0 == name).map(|x| x.1);
            let bv = b.iter().find(|x| x.0 == name).map(|x| x.1);
            match av.cmp(&bv) {
                std::cmp::Ordering::Greater => return true,
                std::cmp::Ordering::Less => return false,
                std::cmp::Ordering::Equal => {}
            }
        }
        false
    }

    fn visit(&mut self, selected: &mut Vec<(usize, usize)>) -> Result<(), ResolveError> {
        let active = active_constraints(self.roots, selected, self.catalog);

        let missing = active
            .keys()
            .filter(|name| !self.catalog.iter().any(|p| &p.name == *name))
            .next()
            .cloned();
        if let Some(package) = missing {
            self.record_error(ResolveError::Missing { package });
            return Ok(());
        }

        let mut selected_by_name = std::collections::BTreeMap::new();
        for (pi, ri) in selected.iter() {
            selected_by_name.insert(
                self.catalog[*pi].name.clone(),
                self.catalog[*pi].releases[*ri].version,
            );
        }
        for (package, constraints) in &active {
            if let Some(version) = selected_by_name.get(package) {
                if !constraints.iter().all(|c| matches(*version, *c)) {
                    self.record_error(ResolveError::Conflict {
                        package: package.clone(),
                        selected: *version,
                        requirements: constraints.clone(),
                    });
                    return Ok(());
                }
            }
        }

        for (package, constraints) in &active {
            if selected_by_name.contains_key(package) {
                continue;
            }
            let pi = self.catalog.iter().position(|p| &p.name == package).unwrap();
            if !self.catalog[pi]
                .releases
                .iter()
                .any(|r| constraints.iter().all(|c| matches(r.version, *c)))
            {
                self.record_error(ResolveError::NoMatch {
                    package: package.clone(),
                    requirements: constraints.clone(),
                });
                return Ok(());
            }
        }

        let next = active
            .keys()
            .find(|name| !selected_by_name.contains_key(*name))
            .cloned();
        let Some(package) = next else {
            let score = self.score(selected);
            let vector = self.vector(selected);
            let replace = match &self.best {
                None => true,
                Some((old_score, old_vector)) => {
                    score < *old_score
                        || (score == *old_score && Self::vector_better(&vector, old_vector))
                }
            };
            if replace {
                self.best = Some((score, vector));
            }
            return Ok(());
        };

        let pi = self.catalog.iter().position(|p| p.name == package).unwrap();
        let constraints = &active[&package];
        let mut candidates = self.catalog[pi]
            .releases
            .iter()
            .enumerate()
            .filter(|(_, r)| constraints.iter().all(|c| matches(r.version, *c)))
            .map(|(ri, r)| (ri, r.version))
            .collect::<Vec<_>>();
        candidates.sort_by(|a, b| b.1.cmp(&a.1));

        if let Some(lock) = self.locks.iter().find(|l| l.package == package) {
            if let Some(pos) = candidates.iter().position(|(_, v)| *v == lock.version) {
                let item = candidates.remove(pos);
                candidates.insert(0, item);
            }
        }

        for (ri, _) in candidates {
            if self.attempts == self.max_attempts {
                return Err(ResolveError::Limit {
                    attempts: self.max_attempts,
                });
            }
            self.attempts += 1;
            selected.push((pi, ri));
            let result = self.visit(selected);
            selected.pop();
            result?;
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

    let mut seen = std::collections::BTreeSet::new();
    for package in catalog {
        if !seen.insert(package.name.clone()) {
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
    search.visit(&mut Vec::new())?;

    let Some((_, vector)) = search.best else {
        return Err(search.first_error.unwrap_or(ResolveError::Missing {
            package: String::new(),
        }));
    };

    let selected = vector
        .iter()
        .map(|(package, version)| Selection {
            package: package.clone(),
            version: *version,
        })
        .collect::<Vec<_>>();

    let active = {
        let pairs = selected
            .iter()
            .filter_map(|s| {
                let pi = catalog.iter().position(|p| p.name == s.package)?;
                let ri = catalog[pi]
                    .releases
                    .iter()
                    .position(|r| r.version == s.version)?;
                Some((pi, ri))
            })
            .collect::<Vec<_>>();
        active_constraints(roots, &pairs, catalog)
    };

    let mut before = std::collections::BTreeMap::<String, Version>::new();
    for lock in locks {
        before.insert(lock.package.clone(), lock.version);
    }
    let after = selected
        .iter()
        .map(|s| (s.package.clone(), s.version))
        .collect::<std::collections::BTreeMap<_, _>>();
    let names = before
        .keys()
        .chain(after.keys())
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();

    let mut changes = Vec::new();
    for package in names {
        let b = before.get(&package).copied();
        let a = after.get(&package).copied();
        if b == a {
            continue;
        }
        let reason = match (b, a) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old), Some(_)) => {
                let exists = catalog
                    .iter()
                    .find(|p| p.name == package)
                    .map(|p| p.releases.iter().any(|r| r.version == old))
                    .unwrap_or(false);
                if !exists {
                    ChangeReason::Unavailable
                } else if active
                    .get(&package)
                    .map(|cs| !cs.iter().all(|c| matches(old, *c)))
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
            package,
            before: b,
            after: a,
            reason,
        });
    }

    Ok(Resolution { selected, changes })
}

fn main() {}