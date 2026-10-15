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
        && bytes.iter().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-'
        })
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

fn encode(text: &str) -> String {
    let mut output = String::new();
    for byte in text.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'.' | b'_' | b'-') {
            output.push(*byte as char);
        } else {
            output.push_str(&format!("%{:02X}", byte));
        }
    }
    output
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

fn matches_constraint(version: Version, constraint: Constraint) -> bool {
    match constraint {
        Constraint::Exact(x) => version == x,
        Constraint::AtLeast(x) => version >= x,
        Constraint::Before(x) => version < x,
        Constraint::Caret(x) => {
            version >= x
                && if x.major > 0 {
                    version.major == x.major
                } else if x.minor > 0 {
                    version.major == 0 && version.minor == x.minor
                } else {
                    version == x
                }
        }
    }
}

fn satisfies(version: Version, constraints: &[Constraint]) -> bool {
    constraints
        .iter()
        .all(|constraint| matches_constraint(version, *constraint))
}

struct Search<'a> {
    catalog: &'a [Package],
    package_indices: std::collections::BTreeMap<String, usize>,
    locks: std::collections::BTreeMap<String, Version>,
    root_constraints: std::collections::BTreeMap<String, Vec<Constraint>>,
    attempts: usize,
    max_attempts: usize,
    best_cost: Option<usize>,
    best_vector: Option<Vec<(String, Version)>>,
    best_selection: Option<std::collections::BTreeMap<String, Version>>,
    first_error: Option<ResolveError>,
}

impl<'a> Search<'a> {
    fn inspect(
        &mut self,
        selected: &mut std::collections::BTreeMap<String, Version>,
    ) -> Result<(), ResolveError> {
        let mut active = self.root_constraints.clone();

        for (name, version) in selected.iter() {
            if let Some(&index) = self.package_indices.get(name) {
                if let Some(release) = self.catalog[index]
                    .releases
                    .iter()
                    .find(|release| release.version == *version)
                {
                    for dependency in &release.deps {
                        active
                            .entry(dependency.package.clone())
                            .or_default()
                            .push(dependency.constraint);
                    }
                }
            }
        }

        if let Some(package) = active
            .keys()
            .find(|name| !self.package_indices.contains_key(*name))
            .cloned()
        {
            return Err(ResolveError::Missing { package });
        }

        for (package, version) in selected.iter() {
            if let Some(constraints) = active.get(package) {
                if !satisfies(*version, constraints) {
                    return Err(ResolveError::Conflict {
                        package: package.clone(),
                        selected: *version,
                        requirements: canonical(constraints.clone()),
                    });
                }
            }
        }

        let mut unselected = Vec::new();
        for (package, constraints) in &active {
            if selected.contains_key(package) {
                continue;
            }
            let index = self.package_indices[package];
            if !self.catalog[index]
                .releases
                .iter()
                .any(|release| satisfies(release.version, constraints))
            {
                return Err(ResolveError::NoMatch {
                    package: package.clone(),
                    requirements: canonical(constraints.clone()),
                });
            }
            unselected.push(package.clone());
        }

        if unselected.is_empty() {
            self.record_solution(selected);
            return Ok(());
        }

        let package = unselected[0].clone();
        let index = self.package_indices[&package];
        let mut candidates: Vec<Version> = self.catalog[index]
            .releases
            .iter()
            .map(|release| release.version)
            .filter(|version| satisfies(*version, &active[&package]))
            .collect();
        candidates.sort_by(|a, b| b.cmp(a));
        candidates.dedup();

        if let Some(lock_version) = self.locks.get(&package) {
            if let Some(position) = candidates.iter().position(|v| v == lock_version) {
                let locked = candidates.remove(position);
                candidates.insert(0, locked);
            }
        }

        for candidate in candidates {
            if self.attempts == self.max_attempts {
                return Err(ResolveError::Limit {
                    attempts: self.max_attempts,
                });
            }

            self.attempts += 1;
            selected.insert(package.clone(), candidate);
            match self.inspect(selected) {
                Ok(()) => {}
                Err(error @ ResolveError::Limit { .. }) => {
                    selected.remove(&package);
                    return Err(error);
                }
                Err(error) => {
                    if self.first_error.is_none() {
                        self.first_error = Some(error);
                    }
                }
            }
            selected.remove(&package);
        }

        Ok(())
    }

    fn record_solution(
        &mut self,
        selected: &std::collections::BTreeMap<String, Version>,
    ) {
        let cost = self
            .locks
            .iter()
            .filter(|(name, version)| selected.get(*name) != Some(*version))
            .count();

        let vector: Vec<(String, Version)> = self
            .package_indices
            .keys()
            .map(|name| {
                (
                    name.clone(),
                    selected.get(name).copied().unwrap_or(Version {
                        major: 0,
                        minor: 0,
                        patch: 0,
                    }),
                )
            })
            .collect();

        let better = match self.best_cost {
            None => true,
            Some(best_cost) if cost < best_cost => true,
            Some(best_cost) if cost > best_cost => false,
            Some(_) => match &self.best_vector {
                None => true,
                Some(best_vector) => compare_solution_vectors(&vector, best_vector)
                    == std::cmp::Ordering::Greater,
            },
        };

        if better {
            self.best_cost = Some(cost);
            self.best_vector = Some(vector);
            self.best_selection = Some(selected.clone());
        }
    }
}

fn compare_solution_vectors(
    a: &[(String, Version)],
    b: &[(String, Version)],
) -> std::cmp::Ordering {
    for ((_, a_version), (_, b_version)) in a.iter().zip(b.iter()) {
        match a_version.cmp(b_version) {
            std::cmp::Ordering::Equal => {}
            order => return order,
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
            for (k, dependency) in release.deps.iter().enumerate() {
                if !valid_name(&dependency.package) {
                    return Err(ResolveError::InvalidName {
                        site: format!("catalog[{}].release[{}].dep[{}]", i, j, k),
                        name: dependency.package.clone(),
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

    let mut lock_map = std::collections::BTreeMap::new();
    for lock in locks {
        if lock_map
            .insert(lock.package.clone(), lock.version)
            .is_some()
        {
            return Err(ResolveError::DuplicateLock {
                package: lock.package.clone(),
            });
        }
    }

    let package_indices: std::collections::BTreeMap<String, usize> = catalog
        .iter()
        .enumerate()
        .map(|(index, package)| (package.name.clone(), index))
        .collect();

    let mut root_constraints: std::collections::BTreeMap<String, Vec<Constraint>> =
        std::collections::BTreeMap::new();
    for root in roots {
        root_constraints
            .entry(root.package.clone())
            .or_default()
            .push(root.constraint);
    }

    let mut search = Search {
        catalog,
        package_indices,
        locks: lock_map.clone(),
        root_constraints,
        attempts: 0,
        max_attempts,
        best_cost: None,
        best_vector: None,
        best_selection: None,
        first_error: None,
    };

    let mut selected = std::collections::BTreeMap::new();
    match search.inspect(&mut selected) {
        Err(error @ ResolveError::Limit { .. }) => return Err(error),
        Err(error) => {
            if search.best_selection.is_none() {
                return Err(search.first_error.unwrap_or(error));
            }
        }
        Ok(()) => {}
    }

    let chosen = match search.best_selection {
        Some(selection) => selection,
        None => {
            return Err(search.first_error.unwrap_or(ResolveError::Missing {
                package: String::new(),
            }));
        }
    };

    let selected_vec = chosen
        .iter()
        .map(|(package, version)| Selection {
            package: package.clone(),
            version: *version,
        })
        .collect();

    let mut final_constraints = search.root_constraints.clone();
    for (package, version) in &chosen {
        let index = search.package_indices[package];
        if let Some(release) = catalog[index]
            .releases
            .iter()
            .find(|release| release.version == *version)
        {
            for dependency in &release.deps {
                final_constraints
                    .entry(dependency.package.clone())
                    .or_default()
                    .push(dependency.constraint);
            }
        }
    }

    let all_names: std::collections::BTreeSet<String> =
        lock_map.keys().chain(chosen.keys()).cloned().collect();
    let mut changes = Vec::new();

    for package in all_names {
        let before = lock_map.get(&package).copied();
        let after = chosen.get(&package).copied();
        if before == after {
            continue;
        }

        let reason = match (before, after) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old), Some(_)) => {
                let exists = search.package_indices.get(&package).is_some_and(|index| {
                    catalog[*index]
                        .releases
                        .iter()
                        .any(|release| release.version == old)
                });
                if !exists {
                    ChangeReason::Unavailable
                } else if final_constraints
                    .get(&package)
                    .is_some_and(|constraints| !satisfies(old, constraints))
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
            before,
            after,
            reason,
        });
    }

    Ok(Resolution {
        selected: selected_vec,
        changes,
    })
}

fn main() {}