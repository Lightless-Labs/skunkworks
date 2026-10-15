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

fn canonicalize(constraints: &mut Vec<Constraint>) {
    constraints.sort();
    constraints.dedup();
}

fn constraints_text(constraints: &[Constraint]) -> String {
    let mut canonical = constraints.to_vec();
    canonicalize(&mut canonical);
    canonical
        .iter()
        .map(|constraint| constraint_text(*constraint))
        .collect::<Vec<_>>()
        .join(",")
}

fn encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'.' | b'_' | b'-') {
            out.push(*byte as char);
        } else {
            out.push('%');
            out.push(char::from(b"0123456789ABCDEF"[(byte >> 4) as usize]));
            out.push(char::from(b"0123456789ABCDEF"[(byte & 0x0f) as usize]));
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
        ResolveError::Limit { attempts } => format!("limit|attempts={attempts}"),
    }
}

fn matches_constraint(version: Version, constraint: Constraint) -> bool {
    match constraint {
        Constraint::Exact(required) => version == required,
        Constraint::AtLeast(required) => version >= required,
        Constraint::Before(required) => version < required,
        Constraint::Caret(required) => {
            version >= required
                && if required.major > 0 {
                    version.major == required.major
                } else if required.minor > 0 {
                    version.major == 0 && version.minor == required.minor
                } else {
                    version == required
                }
        }
    }
}

fn matches_all(version: Version, constraints: &[Constraint]) -> bool {
    constraints
        .iter()
        .all(|constraint| matches_constraint(version, *constraint))
}

struct Search<'a> {
    catalog: &'a [Package],
    roots: &'a [Requirement],
    locks: &'a [Lock],
    max_attempts: usize,
    attempts: usize,
    best: Option<(usize, Vec<Selection>)>,
    first_error: Option<ResolveError>,
    limit_hit: bool,
}

impl<'a> Search<'a> {
    fn package(&self, name: &str) -> Option<&'a Package> {
        self.catalog.iter().find(|package| package.name == name)
    }

    fn active_constraints(
        &self,
        assigned: &[(String, Version)],
    ) -> std::collections::BTreeMap<String, Vec<Constraint>> {
        let mut active = std::collections::BTreeMap::<String, Vec<Constraint>>::new();

        for root in self.roots {
            active
                .entry(root.package.clone())
                .or_default()
                .push(root.constraint);
        }

        for (name, version) in assigned {
            if let Some(package) = self.package(name) {
                if let Some(release) = package.releases.iter().find(|r| r.version == *version) {
                    for dependency in &release.deps {
                        active
                            .entry(dependency.package.clone())
                            .or_default()
                            .push(dependency.constraint);
                    }
                }
            }
        }

        for constraints in active.values_mut() {
            canonicalize(constraints);
        }

        active
    }

    fn inspect(
        &self,
        assigned: &[(String, Version)],
        active: &std::collections::BTreeMap<String, Vec<Constraint>>,
    ) -> Result<Vec<(String, Vec<Version>)>, ResolveError> {
        for name in active.keys() {
            if self.package(name).is_none() {
                return Err(ResolveError::Missing {
                    package: name.clone(),
                });
            }
        }

        for (name, constraints) in active {
            if let Some((_, version)) = assigned.iter().find(|(assigned_name, _)| assigned_name == name)
            {
                if !matches_all(*version, constraints) {
                    return Err(ResolveError::Conflict {
                        package: name.clone(),
                        selected: *version,
                        requirements: constraints.clone(),
                    });
                }
            }
        }

        let mut candidates = Vec::new();
        for (name, constraints) in active {
            if assigned.iter().any(|(assigned_name, _)| assigned_name == name) {
                continue;
            }

            let package = self.package(name).expect("missing packages were checked");
            let mut versions: Vec<Version> = package
                .releases
                .iter()
                .map(|release| release.version)
                .filter(|version| matches_all(*version, constraints))
                .collect();
            versions.sort_by(|a, b| b.cmp(a));

            if versions.is_empty() {
                return Err(ResolveError::NoMatch {
                    package: name.clone(),
                    requirements: constraints.clone(),
                });
            }

            candidates.push((name.clone(), versions));
        }

        Ok(candidates)
    }

    fn lock_cost(&self, assigned: &[(String, Version)]) -> usize {
        self.locks
            .iter()
            .filter(|lock| {
                !assigned
                    .iter()
                    .any(|(name, version)| name == &lock.package && version == &lock.version)
            })
            .count()
    }

    fn better_solution(&self, cost: usize, candidate: &[Selection]) -> bool {
        let Some((best_cost, best)) = &self.best else {
            return true;
        };

        if cost != *best_cost {
            return cost < *best_cost;
        }

        let mut names: Vec<&str> = self.catalog.iter().map(|package| package.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();

        for name in names {
            let candidate_version = candidate
                .iter()
                .find(|selection| selection.package == name)
                .map(|selection| selection.version);
            let best_version = best
                .iter()
                .find(|selection| selection.package == name)
                .map(|selection| selection.version);

            match (candidate_version, best_version) {
                (Some(a), Some(b)) if a != b => return a > b,
                (Some(_), None) => return true,
                (None, Some(_)) => return false,
                _ => {}
            }
        }

        false
    }

    fn search(&mut self, assigned: &mut Vec<(String, Version)>) {
        if self.limit_hit {
            return;
        }

        let active = self.active_constraints(assigned);
        let candidates = match self.inspect(assigned, &active) {
            Ok(candidates) => candidates,
            Err(error) => {
                if self.first_error.is_none() {
                    self.first_error = Some(error);
                }
                return;
            }
        };

        if candidates.is_empty() {
            let mut selected: Vec<Selection> = assigned
                .iter()
                .map(|(package, version)| Selection {
                    package: package.clone(),
                    version: *version,
                })
                .collect();
            selected.sort_by(|a, b| a.package.cmp(&b.package));

            let cost = self.lock_cost(assigned);
            if self.better_solution(cost, &selected) {
                self.best = Some((cost, selected));
            }
            return;
        }

        let (package_name, mut versions) = candidates.into_iter().next().unwrap();
        if let Some(lock) = self.locks.iter().find(|lock| lock.package == package_name) {
            if let Some(position) = versions.iter().position(|version| *version == lock.version) {
                let locked_version = versions.remove(position);
                versions.insert(0, locked_version);
            }
        }

        for version in versions {
            if self.attempts == self.max_attempts {
                self.limit_hit = true;
                return;
            }

            self.attempts += 1;
            assigned.push((package_name.clone(), version));
            self.search(assigned);
            assigned.pop();

            if self.limit_hit {
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
    for (index, root) in roots.iter().enumerate() {
        if !valid_name(&root.package) {
            return Err(ResolveError::InvalidName {
                site: format!("root[{index}]"),
                name: root.package.clone(),
            });
        }
    }

    for (package_index, package) in catalog.iter().enumerate() {
        if !valid_name(&package.name) {
            return Err(ResolveError::InvalidName {
                site: format!("catalog[{package_index}]"),
                name: package.name.clone(),
            });
        }

        for (release_index, release) in package.releases.iter().enumerate() {
            for (dependency_index, dependency) in release.deps.iter().enumerate() {
                if !valid_name(&dependency.package) {
                    return Err(ResolveError::InvalidName {
                        site: format!(
                            "catalog[{package_index}].release[{release_index}].dep[{dependency_index}]"
                        ),
                        name: dependency.package.clone(),
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

    for (index, lock) in locks.iter().enumerate() {
        if !valid_name(&lock.package) {
            return Err(ResolveError::InvalidName {
                site: format!("lock[{index}]"),
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
        max_attempts,
        attempts: 0,
        best: None,
        first_error: None,
        limit_hit: false,
    };
    search.search(&mut Vec::new());

    if search.limit_hit {
        return Err(ResolveError::Limit {
            attempts: max_attempts,
        });
    }

    let Some((_, selected)) = search.best.clone() else {
        return Err(search.first_error.unwrap_or(ResolveError::NoMatch {
            package: String::new(),
            requirements: Vec::new(),
        }));
    };

    let active = search.active_constraints(
        &selected
            .iter()
            .map(|selection| (selection.package.clone(), selection.version))
            .collect::<Vec<_>>(),
    );

    let before: std::collections::BTreeMap<String, Version> = locks
        .iter()
        .map(|lock| (lock.package.clone(), lock.version))
        .collect();
    let after: std::collections::BTreeMap<String, Version> = selected
        .iter()
        .map(|selection| (selection.package.clone(), selection.version))
        .collect();
    let all_names: std::collections::BTreeSet<String> =
        before.keys().chain(after.keys()).cloned().collect();

    let mut changes = Vec::new();
    for package in all_names {
        let before_version = before.get(&package).copied();
        let after_version = after.get(&package).copied();

        if before_version == after_version {
            continue;
        }

        let reason = match (before_version, after_version) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old), Some(_)) => {
                let exists = search
                    .package(&package)
                    .map(|catalog_package| {
                        catalog_package
                            .releases
                            .iter()
                            .any(|release| release.version == old)
                    })
                    .unwrap_or(false);

                if !exists {
                    ChangeReason::Unavailable
                } else if !matches_all(
                    old,
                    active.get(&package).map(Vec::as_slice).unwrap_or(&[]),
                ) {
                    ChangeReason::Rejected
                } else {
                    ChangeReason::Rebalanced
                }
            }
            (None, None) => unreachable!(),
        };

        changes.push(Change {
            package,
            before: before_version,
            after: after_version,
            reason,
        });
    }

    Ok(Resolution { selected, changes })
}