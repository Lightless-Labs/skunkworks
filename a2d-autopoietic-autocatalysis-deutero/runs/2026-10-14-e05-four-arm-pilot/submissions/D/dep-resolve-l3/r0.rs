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
        && bytes.last() != Some(&b'-')
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

fn constraint_key(c: Constraint) -> (u8, Version) {
    match c {
        Constraint::Exact(v) => (0, v),
        Constraint::AtLeast(v) => (1, v),
        Constraint::Before(v) => (2, v),
        Constraint::Caret(v) => (3, v),
    }
}

fn canonical_constraints(mut constraints: Vec<Constraint>) -> Vec<Constraint> {
    constraints.sort_by_key(|c| constraint_key(*c));
    constraints.dedup();
    constraints
}

fn constraints_text(constraints: &[Constraint]) -> String {
    canonical_constraints(constraints.to_vec())
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
            out.push_str(&format!("{:02X}", byte));
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

fn validate_inputs(
    catalog: &[Package],
    roots: &[Requirement],
    locks: &[Lock],
) -> Result<(), ResolveError> {
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
        let mut seen_versions = Vec::new();
        for release in &package.releases {
            if seen_versions.contains(&release.version) {
                return Err(ResolveError::DuplicateVersion {
                    package: package.name.clone(),
                    version: release.version,
                });
            }
            seen_versions.push(release.version);
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

    Ok(())
}

#[derive(Clone)]
struct SearchState {
    selected: std::collections::BTreeMap<String, Version>,
}

struct SearchContext<'a> {
    catalog: &'a [Package],
    roots: &'a [Requirement],
    locks: &'a [Lock],
    names: Vec<String>,
    attempts: usize,
    max_attempts: usize,
    best: Option<(usize, Vec<Option<Version>>, SearchState)>,
    first_error: Option<ResolveError>,
    limit: bool,
}

impl<'a> SearchContext<'a> {
    fn package(&self, name: &str) -> Option<&'a Package> {
        self.catalog.iter().find(|package| package.name == name)
    }

    fn active_constraints(
        &self,
        state: &SearchState,
    ) -> std::collections::BTreeMap<String, Vec<Constraint>> {
        let mut active = std::collections::BTreeMap::<String, Vec<Constraint>>::new();
        for root in self.roots {
            active
                .entry(root.package.clone())
                .or_default()
                .push(root.constraint);
        }
        for (name, version) in &state.selected {
            if let Some(package) = self.package(name) {
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
            *constraints = canonical_constraints(std::mem::take(constraints));
        }
        active
    }

    fn inspect_node(
        &self,
        state: &SearchState,
        active: &std::collections::BTreeMap<String, Vec<Constraint>>,
    ) -> Result<Option<String>, ResolveError> {
        if let Some(name) = active.keys().find(|name| self.package(name).is_none()) {
            return Err(ResolveError::Missing {
                package: name.clone(),
            });
        }

        for (name, version) in &state.selected {
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
            if state.selected.contains_key(name) {
                continue;
            }
            let package = self.package(name).expect("missing packages checked above");
            if !package
                .releases
                .iter()
                .any(|release| satisfies(release.version, constraints))
            {
                return Err(ResolveError::NoMatch {
                    package: name.clone(),
                    requirements: constraints.clone(),
                });
            }
        }

        Ok(active
            .keys()
            .find(|name| !state.selected.contains_key(*name))
            .cloned())
    }

    fn lock_cost(&self, state: &SearchState) -> usize {
        self.locks
            .iter()
            .filter(|lock| state.selected.get(&lock.package) != Some(&lock.version))
            .count()
    }

    fn vector(&self, state: &SearchState) -> Vec<Option<Version>> {
        self.names
            .iter()
            .map(|name| state.selected.get(name).copied())
            .collect()
    }

    fn record_solution(&mut self, state: &SearchState) {
        let cost = self.lock_cost(state);
        let vector = self.vector(state);
        let better = match &self.best {
            None => true,
            Some((best_cost, best_vector, _)) => {
                cost < *best_cost || (cost == *best_cost && vector > *best_vector)
            }
        };
        if better {
            self.best = Some((cost, vector, state.clone()));
        }
    }

    fn search(&mut self, state: SearchState) {
        if self.limit {
            return;
        }

        let active = self.active_constraints(&state);
        let next = match self.inspect_node(&state, &active) {
            Ok(next) => next,
            Err(error) => {
                if self.first_error.is_none() {
                    self.first_error = Some(error);
                }
                return;
            }
        };

        let Some(name) = next else {
            self.record_solution(&state);
            return;
        };

        let constraints = active.get(&name).cloned().unwrap_or_default();
        let package = self.package(&name).expect("active package exists");
        let mut candidates: Vec<Version> = package
            .releases
            .iter()
            .map(|release| release.version)
            .filter(|version| satisfies(*version, &constraints))
            .collect();
        candidates.sort_by(|a, b| b.cmp(a));

        if let Some(lock) = self.locks.iter().find(|lock| lock.package == name) {
            if let Some(index) = candidates.iter().position(|version| *version == lock.version) {
                let locked = candidates.remove(index);
                candidates.insert(0, locked);
            }
        }

        for candidate in candidates {
            if self.attempts == self.max_attempts {
                self.limit = true;
                return;
            }

            self.attempts += 1;
            let mut child = state.clone();
            child.selected.insert(name.clone(), candidate);
            self.search(child);

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
    validate_inputs(catalog, roots, locks)?;

    let mut names: Vec<String> = catalog.iter().map(|p| p.name.clone()).collect();
    names.sort();

    let mut context = SearchContext {
        catalog,
        roots,
        locks,
        names,
        attempts: 0,
        max_attempts,
        best: None,
        first_error: None,
        limit: false,
    };

    context.search(SearchState {
        selected: std::collections::BTreeMap::new(),
    });

    if context.limit {
        return Err(ResolveError::Limit {
            attempts: max_attempts,
        });
    }

    let state = match context.best.as_ref() {
        Some((_, _, state)) => state.clone(),
        None => {
            return Err(context.first_error.unwrap_or(ResolveError::NoMatch {
                package: String::new(),
                requirements: Vec::new(),
            }));
        }
    };

    let active = context.active_constraints(&state);
    let selected: Vec<Selection> = state
        .selected
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
    let after = state.selected.clone();

    let mut changed_names = std::collections::BTreeSet::new();
    changed_names.extend(before.keys().cloned());
    changed_names.extend(after.keys().cloned());

    let mut changes = Vec::new();
    for package in changed_names {
        let old = before.get(&package).copied();
        let new = after.get(&package).copied();
        if old == new {
            continue;
        }

        let reason = match (old, new) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old_version), Some(_)) => {
                let exists = context.package(&package).is_some_and(|catalog_package| {
                    catalog_package
                        .releases
                        .iter()
                        .any(|release| release.version == old_version)
                });

                if !exists {
                    ChangeReason::Unavailable
                } else if active
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