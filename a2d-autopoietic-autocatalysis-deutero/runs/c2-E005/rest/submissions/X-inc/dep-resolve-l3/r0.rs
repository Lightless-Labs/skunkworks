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
    let mut out = constraints.to_vec();
    out.sort();
    out.dedup();
    out
}

fn constraints_text(constraints: &[Constraint]) -> String {
    canonical_constraints(constraints)
        .iter()
        .map(|c| constraint_text(*c))
        .collect::<Vec<_>>()
        .join(",")
}

fn encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric()
            || *byte == b'.'
            || *byte == b'_'
            || *byte == b'-'
        {
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
        ResolveError::Limit { attempts } => format!("limit|attempts={attempts}"),
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
        .all(|c| matches_constraint(version, *c))
}

fn validate_name(name: &str, site: String) -> Result<(), ResolveError> {
    if valid_name(name) {
        Ok(())
    } else {
        Err(ResolveError::InvalidName {
            site,
            name: name.to_string(),
        })
    }
}

fn catalog_index(catalog: &[Package]) -> std::collections::BTreeMap<String, usize> {
    catalog
        .iter()
        .enumerate()
        .map(|(i, p)| (p.name.clone(), i))
        .collect()
}

fn active_constraints(
    roots: &[Requirement],
    catalog: &[Package],
    chosen: &std::collections::BTreeMap<String, Version>,
) -> std::collections::BTreeMap<String, Vec<Constraint>> {
    let mut active = std::collections::BTreeMap::<String, Vec<Constraint>>::new();

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

struct Search<'a> {
    catalog: &'a [Package],
    roots: &'a [Requirement],
    locks: &'a [Lock],
    attempts: usize,
    max_attempts: usize,
    first_error: Option<ResolveError>,
    best: Option<std::collections::BTreeMap<String, Version>>,
    best_cost: usize,
    catalog_names: Vec<String>,
}

impl<'a> Search<'a> {
    fn record_error(&mut self, error: ResolveError) {
        if self.first_error.is_none() {
            self.first_error = Some(error);
        }
    }

    fn cost(&self, chosen: &std::collections::BTreeMap<String, Version>) -> usize {
        self.locks
            .iter()
            .filter(|lock| chosen.get(&lock.package) != Some(&lock.version))
            .count()
    }

    fn better(
        &self,
        candidate: &std::collections::BTreeMap<String, Version>,
        cost: usize,
    ) -> bool {
        let Some(best) = &self.best else {
            return true;
        };
        if cost != self.best_cost {
            return cost < self.best_cost;
        }
        for name in &self.catalog_names {
            match (candidate.get(name), best.get(name)) {
                (Some(a), Some(b)) if a != b => return a > b,
                (Some(_), None) => return true,
                (None, Some(_)) => return false,
                _ => {}
            }
        }
        false
    }

    fn visit(
        &mut self,
        chosen: &mut std::collections::BTreeMap<String, Version>,
    ) -> Result<(), ResolveError> {
        let active = active_constraints(self.roots, self.catalog, chosen);
        let index = catalog_index(self.catalog);

        if let Some(name) = active.keys().find(|name| !index.contains_key(*name)) {
            self.record_error(ResolveError::Missing {
                package: name.clone(),
            });
            return Ok(());
        }

        for (name, version) in chosen.iter() {
            if let Some(constraints) = active.get(name) {
                if !satisfies(*version, constraints) {
                    self.record_error(ResolveError::Conflict {
                        package: name.clone(),
                        selected: *version,
                        requirements: constraints.clone(),
                    });
                    return Ok(());
                }
            }
        }

        for (name, constraints) in &active {
            if chosen.contains_key(name) {
                continue;
            }
            let package = &self.catalog[index[name]];
            if !package
                .releases
                .iter()
                .any(|release| satisfies(release.version, constraints))
            {
                self.record_error(ResolveError::NoMatch {
                    package: name.clone(),
                    requirements: constraints.clone(),
                });
                return Ok(());
            }
        }

        let Some(name) = active.keys().find(|name| !chosen.contains_key(*name)).cloned() else {
            let cost = self.cost(chosen);
            if self.better(chosen, cost) {
                self.best = Some(chosen.clone());
                self.best_cost = cost;
            }
            return Ok(());
        };

        let package = &self.catalog[index[&name]];
        let constraints = &active[&name];
        let mut candidates: Vec<Version> = package
            .releases
            .iter()
            .map(|r| r.version)
            .filter(|v| satisfies(*v, constraints))
            .collect();
        candidates.sort_by(|a, b| b.cmp(a));

        if let Some(lock) = self.locks.iter().find(|l| l.package == name) {
            if let Some(pos) = candidates.iter().position(|v| *v == lock.version) {
                let locked = candidates.remove(pos);
                candidates.insert(0, locked);
            }
        }

        for version in candidates {
            if self.attempts == self.max_attempts {
                return Err(ResolveError::Limit {
                    attempts: self.max_attempts,
                });
            }
            self.attempts += 1;
            chosen.insert(name.clone(), version);
            let result = self.visit(chosen);
            chosen.remove(&name);
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
        validate_name(&root.package, format!("root[{i}]"))?;
    }
    for (i, package) in catalog.iter().enumerate() {
        validate_name(&package.name, format!("catalog[{i}]"))?;
        for (j, release) in package.releases.iter().enumerate() {
            for (k, dep) in release.deps.iter().enumerate() {
                validate_name(
                    &dep.package,
                    format!("catalog[{i}].release[{j}].dep[{k}]"),
                )?;
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
        validate_name(&lock.package, format!("lock[{i}]"))?;
    }
    let mut seen_locks = std::collections::BTreeSet::new();
    for lock in locks {
        if !seen_locks.insert(lock.package.clone()) {
            return Err(ResolveError::DuplicateLock {
                package: lock.package.clone(),
            });
        }
    }

    let mut catalog_names: Vec<String> = catalog.iter().map(|p| p.name.clone()).collect();
    catalog_names.sort();

    let mut search = Search {
        catalog,
        roots,
        locks,
        attempts: 0,
        max_attempts,
        first_error: None,
        best: None,
        best_cost: usize::MAX,
        catalog_names,
    };
    search.visit(&mut std::collections::BTreeMap::new())?;

    let Some(chosen) = search.best else {
        return Err(search.first_error.unwrap_or(ResolveError::NoMatch {
            package: String::new(),
            requirements: Vec::new(),
        }));
    };

    let final_active = active_constraints(roots, catalog, &chosen);
    let selected: Vec<Selection> = chosen
        .iter()
        .map(|(package, version)| Selection {
            package: package.clone(),
            version: *version,
        })
        .collect();

    let before: std::collections::BTreeMap<String, Version> = locks
        .iter()
        .map(|lock| (lock.package.clone(), lock.version))
        .collect();
    let mut all_names = std::collections::BTreeSet::new();
    all_names.extend(before.keys().cloned());
    all_names.extend(chosen.keys().cloned());

    let mut changes = Vec::new();
    for package in all_names {
        let b = before.get(&package).copied();
        let a = chosen.get(&package).copied();
        if b == a {
            continue;
        }

        let reason = match (b, a) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(locked), Some(_)) => {
                let exists = catalog
                    .iter()
                    .find(|p| p.name == package)
                    .is_some_and(|p| p.releases.iter().any(|r| r.version == locked));
                if !exists {
                    ChangeReason::Unavailable
                } else if !satisfies(
                    locked,
                    final_active.get(&package).map(Vec::as_slice).unwrap_or(&[]),
                ) {
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