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

fn check_name(site: String, name: &str) -> Result<(), ResolveError> {
    if valid_name(name) {
        Ok(())
    } else {
        Err(ResolveError::InvalidName {
            site,
            name: name.to_owned(),
        })
    }
}

fn canonical_constraints(mut constraints: Vec<Constraint>) -> Vec<Constraint> {
    constraints.sort();
    constraints.dedup();
    constraints
}

fn matches(version: Version, constraints: &[Constraint]) -> bool {
    constraints.iter().all(|constraint| match *constraint {
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
    })
}

fn version_text(version: Version) -> String {
    format!("{}.{}.{}", version.major, version.minor, version.patch)
}

fn constraint_text(constraint: Constraint) -> String {
    match constraint {
        Constraint::Exact(v) => format!("={}", version_text(v)),
        Constraint::AtLeast(v) => format!(">={}", version_text(v)),
        Constraint::Before(v) => format!("<{}", version_text(v)),
        Constraint::Caret(v) => format!("^{}", version_text(v)),
    }
}

fn constraints_text(constraints: &[Constraint]) -> String {
    canonical_constraints(constraints.to_vec())
        .iter()
        .map(|c| constraint_text(*c))
        .collect::<Vec<_>>()
        .join(",")
}

fn encode(text: &str) -> String {
    let mut result = String::new();
    for byte in text.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'.' | b'_' | b'-') {
            result.push(*byte as char);
        } else {
            result.push('%');
            result.push(char::from_digit((byte >> 4) as u32, 16).unwrap().to_ascii_uppercase());
            result.push(char::from_digit((byte & 15) as u32, 16).unwrap().to_ascii_uppercase());
        }
    }
    result
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

fn collect_active(
    roots: &[Requirement],
    catalog: &[Package],
    assigned: &[(String, Version)],
) -> std::collections::BTreeMap<String, Vec<Constraint>> {
    let mut active = std::collections::BTreeMap::<String, Vec<Constraint>>::new();
    for root in roots {
        active
            .entry(root.package.clone())
            .or_default()
            .push(root.constraint);
    }
    for (package_name, version) in assigned {
        if let Some(package) = catalog.iter().find(|p| p.name == *package_name) {
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

struct Search<'a> {
    catalog: &'a [Package],
    roots: &'a [Requirement],
    locks: &'a [Lock],
    max_attempts: usize,
    attempts: usize,
    best: Option<(usize, Vec<(String, Version)>)>,
    first_failure: Option<ResolveError>,
}

impl<'a> Search<'a> {
    fn visit(&mut self, assigned: &mut Vec<(String, Version)>) -> Result<(), ResolveError> {
        let active = collect_active(self.roots, self.catalog, assigned);

        if let Some(name) = active
            .keys()
            .filter(|name| !self.catalog.iter().any(|p| p.name == ***name))
            .next()
        {
            let error = ResolveError::Missing {
                package: name.clone(),
            };
            if self.first_failure.is_none() {
                self.first_failure = Some(error);
            }
            return Ok(());
        }

        let mut selected = assigned.clone();
        selected.sort_by(|a, b| a.0.cmp(&b.0));
        for (name, version) in &selected {
            let constraints = active.get(name).map(Vec::as_slice).unwrap_or(&[]);
            if !matches(*version, constraints) {
                let error = ResolveError::Conflict {
                    package: name.clone(),
                    selected: *version,
                    requirements: constraints.to_vec(),
                };
                if self.first_failure.is_none() {
                    self.first_failure = Some(error);
                }
                return Ok(());
            }
        }

        let assigned_names: std::collections::BTreeSet<&str> =
            assigned.iter().map(|(name, _)| name.as_str()).collect();

        for (name, constraints) in &active {
            if assigned_names.contains(name.as_str()) {
                continue;
            }
            let package = self.catalog.iter().find(|p| p.name == *name).unwrap();
            if !package
                .releases
                .iter()
                .any(|release| matches(release.version, constraints))
            {
                let error = ResolveError::NoMatch {
                    package: name.clone(),
                    requirements: constraints.clone(),
                };
                if self.first_failure.is_none() {
                    self.first_failure = Some(error);
                }
                return Ok(());
            }
        }

        let next_name = active
            .keys()
            .find(|name| !assigned_names.contains(name.as_str()))
            .cloned();
        let Some(name) = next_name else {
            let cost = self
                .locks
                .iter()
                .filter(|lock| {
                    !assigned
                        .iter()
                        .any(|(package, version)| package == &lock.package && *version == lock.version)
                })
                .count();
            let mut solution = assigned.clone();
            solution.sort_by(|a, b| a.0.cmp(&b.0));
            let replace = match &self.best {
                None => true,
                Some((best_cost, best_solution)) => {
                    cost < *best_cost
                        || (cost == *best_cost
                            && self.compare_vectors(&solution, best_solution).is_gt())
                }
            };
            if replace {
                self.best = Some((cost, solution));
            }
            return Ok(());
        };

        let constraints = active.get(&name).unwrap();
        let package = self.catalog.iter().find(|p| p.name == name).unwrap();
        let mut candidates: Vec<Version> = package
            .releases
            .iter()
            .map(|release| release.version)
            .filter(|version| matches(*version, constraints))
            .collect();
        candidates.sort_by(|a, b| b.cmp(a));

        if let Some(lock) = self.locks.iter().find(|lock| lock.package == name) {
            if let Some(index) = candidates.iter().position(|v| *v == lock.version) {
                let version = candidates.remove(index);
                candidates.insert(0, version);
            }
        }

        for candidate in candidates {
            if self.attempts == self.max_attempts {
                return Err(ResolveError::Limit {
                    attempts: self.max_attempts,
                });
            }
            self.attempts += 1;
            assigned.push((name.clone(), candidate));
            self.visit(assigned)?;
            assigned.pop();
        }
        Ok(())
    }

    fn compare_vectors(
        &self,
        left: &[(String, Version)],
        right: &[(String, Version)],
    ) -> std::cmp::Ordering {
        let mut names: Vec<&str> = self.catalog.iter().map(|p| p.name.as_str()).collect();
        names.sort();
        names.dedup();
        for name in names {
            let a = left.iter().find(|(n, _)| n == name).map(|(_, v)| *v);
            let b = right.iter().find(|(n, _)| n == name).map(|(_, v)| *v);
            let ordering = match (a, b) {
                (None, None) => std::cmp::Ordering::Equal,
                (None, Some(_)) => std::cmp::Ordering::Less,
                (Some(_), None) => std::cmp::Ordering::Greater,
                (Some(a), Some(b)) => a.cmp(&b),
            };
            if ordering != std::cmp::Ordering::Equal {
                return ordering;
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
        check_name(format!("root[{i}]"), &root.package)?;
    }
    for (i, package) in catalog.iter().enumerate() {
        check_name(format!("catalog[{i}]"), &package.name)?;
        for (j, release) in package.releases.iter().enumerate() {
            for (k, dep) in release.deps.iter().enumerate() {
                check_name(
                    format!("catalog[{i}].release[{j}].dep[{k}]"),
                    &dep.package,
                )?;
            }
        }
    }

    let mut package_names = std::collections::BTreeSet::new();
    for package in catalog {
        if !package_names.insert(package.name.as_str()) {
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
        check_name(format!("lock[{i}]"), &lock.package)?;
    }
    let mut lock_names = std::collections::BTreeSet::new();
    for lock in locks {
        if !lock_names.insert(lock.package.as_str()) {
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
        first_failure: None,
    };
    search.visit(&mut Vec::new())?;

    let Some((_, solution)) = search.best else {
        return Err(search.first_failure.unwrap_or(ResolveError::Missing {
            package: String::new(),
        }));
    };

    let selected: Vec<Selection> = solution
        .iter()
        .map(|(package, version)| Selection {
            package: package.clone(),
            version: *version,
        })
        .collect();

    let final_assigned = solution.clone();
    let final_active = collect_active(roots, catalog, &final_assigned);
    let mut before_after = std::collections::BTreeMap::<String, (Option<Version>, Option<Version>)>::new();
    for lock in locks {
        before_after
            .entry(lock.package.clone())
            .or_insert((Some(lock.version), None));
    }
    for item in &selected {
        before_after
            .entry(item.package.clone())
            .and_modify(|pair| pair.1 = Some(item.version))
            .or_insert((None, Some(item.version)));
    }

    let mut changes = Vec::new();
    for (package, (before, after)) in before_after {
        if before == after {
            continue;
        }
        let reason = match (before, after) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old), Some(_)) => {
                let exists = catalog
                    .iter()
                    .find(|p| p.name == package)
                    .is_some_and(|p| p.releases.iter().any(|r| r.version == old));
                if !exists {
                    ChangeReason::Unavailable
                } else if !matches(
                    old,
                    final_active
                        .get(&package)
                        .map(Vec::as_slice)
                        .unwrap_or(&[]),
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
            before,
            after,
            reason,
        });
    }

    Ok(Resolution { selected, changes })
}