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

fn canonical(mut constraints: Vec<Constraint>) -> Vec<Constraint> {
    constraints.sort();
    constraints.dedup();
    constraints
}

fn constraints_text(constraints: &[Constraint]) -> String {
    let mut canonical = canonical(constraints.to_vec());
    canonical
        .drain(..)
        .map(constraint_text)
        .collect::<Vec<_>>()
        .join(",")
}

fn encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-') {
            out.push(byte as char);
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

fn matches_all(version: Version, constraints: &[Constraint]) -> bool {
    constraints
        .iter()
        .all(|constraint| matches_constraint(version, *constraint))
}

fn validated_inputs(
    catalog: &[Package],
    roots: &[Requirement],
    locks: &[Lock],
) -> Result<(), ResolveError> {
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

    let mut names = std::collections::BTreeSet::new();
    for package in catalog {
        if !names.insert(package.name.as_str()) {
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
                site: format!("lock[{i}]"),
                name: lock.package.clone(),
            });
        }
    }

    let mut lock_names = std::collections::BTreeSet::new();
    for lock in locks {
        if !lock_names.insert(lock.package.as_str()) {
            return Err(ResolveError::DuplicateLock {
                package: lock.package.clone(),
            });
        }
    }

    Ok(())
}

type Assignment = std::collections::BTreeMap<String, Version>;
type Active = std::collections::BTreeMap<String, Vec<Constraint>>;

fn active_constraints(roots: &[Requirement], assignment: &Assignment, catalog: &[Package]) -> Active {
    let mut active = Active::new();
    for root in roots {
        active
            .entry(root.package.clone())
            .or_default()
            .push(root.constraint);
    }

    for (name, version) in assignment {
        if let Some(package) = catalog.iter().find(|package| package.name == *name) {
            if let Some(release) = package.releases.iter().find(|release| release.version == *version)
            {
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

fn find_node_error(
    catalog: &[Package],
    active: &Active,
    assignment: &Assignment,
) -> Option<ResolveError> {
    let catalog_names: std::collections::BTreeSet<&str> =
        catalog.iter().map(|package| package.name.as_str()).collect();

    if let Some(name) = active
        .keys()
        .filter(|name| !catalog_names.contains(name.as_str()))
        .min()
    {
        return Some(ResolveError::Missing {
            package: name.clone(),
        });
    }

    for (name, version) in assignment {
        if let Some(constraints) = active.get(name) {
            if !matches_all(*version, constraints) {
                return Some(ResolveError::Conflict {
                    package: name.clone(),
                    selected: *version,
                    requirements: constraints.clone(),
                });
            }
        }
    }

    for (name, constraints) in active {
        if assignment.contains_key(name) {
            continue;
        }
        let package = catalog.iter().find(|package| package.name == *name)?;
        let any = package
            .releases
            .iter()
            .any(|release| matches_all(release.version, constraints));
        if !any {
            return Some(ResolveError::NoMatch {
                package: name.clone(),
                requirements: constraints.clone(),
            });
        }
    }

    None
}

struct Search<'a> {
    catalog: &'a [Package],
    roots: &'a [Requirement],
    locks: &'a [Lock],
    max_attempts: usize,
    attempts: usize,
    best: Option<(usize, Vec<Version>, Assignment)>,
    catalog_names: Vec<String>,
}

impl Search<'_> {
    fn lock_cost(&self, assignment: &Assignment) -> usize {
        self.locks
            .iter()
            .filter(|lock| assignment.get(&lock.package) != Some(&lock.version))
            .count()
    }

    fn vector(&self, assignment: &Assignment) -> Vec<Version> {
        self.catalog_names
            .iter()
            .map(|name| assignment.get(name).copied().unwrap_or(Version {
                major: 0,
                minor: 0,
                patch: 0,
            }))
            .collect()
    }

    fn record_solution(&mut self, assignment: &Assignment) {
        let cost = self.lock_cost(assignment);
        let vector = self.vector(assignment);
        let improve = match &self.best {
            None => true,
            Some((best_cost, best_vector, _)) => {
                cost < *best_cost || (cost == *best_cost && vector > *best_vector)
            }
        };
        if improve {
            self.best = Some((cost, vector, assignment.clone()));
        }
    }

    fn visit(&mut self, assignment: &mut Assignment) -> Result<(), ResolveError> {
        let active = active_constraints(self.roots, assignment, self.catalog);
        if let Some(error) = find_node_error(self.catalog, &active, assignment) {
            return Err(error);
        }

        let next = active
            .keys()
            .find(|name| !assignment.contains_key(*name))
            .cloned();

        let Some(package_name) = next else {
            self.record_solution(assignment);
            return Ok(());
        };

        let package = self
            .catalog
            .iter()
            .find(|package| package.name == package_name)
            .expect("active package was checked for existence");

        let constraints = active
            .get(&package_name)
            .expect("branch package is active");
        let mut candidates: Vec<Version> = package
            .releases
            .iter()
            .map(|release| release.version)
            .filter(|version| matches_all(*version, constraints))
            .collect();
        candidates.sort_by(|a, b| b.cmp(a));

        if let Some(lock) = self.locks.iter().find(|lock| lock.package == package_name) {
            if let Some(index) = candidates.iter().position(|version| *version == lock.version) {
                let locked = candidates.remove(index);
                candidates.insert(0, locked);
            }
        }

        let mut first_error = None;
        for candidate in candidates {
            if self.attempts == self.max_attempts {
                return Err(ResolveError::Limit {
                    attempts: self.max_attempts,
                });
            }
            self.attempts += 1;
            assignment.insert(package_name.clone(), candidate);
            let result = self.visit(assignment);
            assignment.remove(&package_name);

            match result {
                Ok(()) => {}
                Err(ResolveError::Limit { attempts }) => {
                    return Err(ResolveError::Limit { attempts });
                }
                Err(error) => {
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
        }

        Err(first_error.unwrap_or_else(|| ResolveError::NoMatch {
            package: package_name,
            requirements: constraints.clone(),
        }))
    }
}

pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
    locks: &[Lock],
    max_attempts: usize,
) -> Result<Resolution, ResolveError> {
    validated_inputs(catalog, roots, locks)?;

    let mut catalog_names: Vec<String> = catalog.iter().map(|package| package.name.clone()).collect();
    catalog_names.sort();

    let mut search = Search {
        catalog,
        roots,
        locks,
        max_attempts,
        attempts: 0,
        best: None,
        catalog_names,
    };

    let mut assignment = Assignment::new();
    let search_result = search.visit(&mut assignment);

    if let Err(ResolveError::Limit { attempts }) = search_result {
        return Err(ResolveError::Limit { attempts });
    }

    let Some((_, _, solution)) = search.best else {
        return Err(search_result.expect_err("a search without solutions has a failing leaf"));
    };

    let active = active_constraints(roots, &solution, catalog);
    let mut selected: Vec<Selection> = solution
        .iter()
        .map(|(package, version)| Selection {
            package: package.clone(),
            version: *version,
        })
        .collect();
    selected.sort_by(|a, b| a.package.cmp(&b.package));

    let before: std::collections::BTreeMap<String, Version> = locks
        .iter()
        .map(|lock| (lock.package.clone(), lock.version))
        .collect();
    let after: std::collections::BTreeMap<String, Version> = solution.clone();

    let all_names: std::collections::BTreeSet<String> =
        before.keys().chain(after.keys()).cloned().collect();
    let mut changes = Vec::new();

    for package in all_names {
        let old = before.get(&package).copied();
        let new = after.get(&package).copied();
        if old == new {
            continue;
        }

        let reason = match (old, new) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old_version), Some(_)) => {
                let exists = catalog
                    .iter()
                    .find(|entry| entry.name == package)
                    .is_some_and(|entry| {
                        entry.releases.iter().any(|release| release.version == old_version)
                    });
                if !exists {
                    ChangeReason::Unavailable
                } else if active
                    .get(&package)
                    .is_some_and(|constraints| !matches_all(old_version, constraints))
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