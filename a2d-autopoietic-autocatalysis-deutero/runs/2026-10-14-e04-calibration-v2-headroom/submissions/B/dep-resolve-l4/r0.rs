use std::collections::{BTreeMap, BTreeSet};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Linux,
    Mac,
    Windows,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Any,
    Linux,
    Mac,
    Windows,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Requirement {
    pub package: String,
    pub constraint: Constraint,
    pub features: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dependency {
    pub package: String,
    pub constraint: Constraint,
    pub features: Vec<String>,
    pub gate: Option<String>,
    pub platform: Platform,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Feature {
    pub name: String,
    pub enables: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    pub deps: Vec<Dependency>,
    pub features: Vec<Feature>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Evidence {
    pub constraint: Constraint,
    pub path: Vec<String>,
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
    DuplicateFeature {
        package: String,
        version: Version,
        feature: String,
    },
    DuplicateLock {
        package: String,
    },
    Missing {
        package: String,
        evidence: Vec<Evidence>,
    },
    NoMatch {
        package: String,
        requirements: Vec<Constraint>,
        evidence: Vec<Evidence>,
    },
    Conflict {
        package: String,
        selected: Version,
        requirements: Vec<Constraint>,
        evidence: Vec<Evidence>,
    },
    MissingFeature {
        package: String,
        selected: Version,
        feature: String,
        path: Vec<String>,
    },
    Limit {
        attempts: usize,
    },
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

fn canonical_constraints(cs: &[Constraint]) -> Vec<Constraint> {
    let mut out = cs.to_vec();
    out.sort();
    out.dedup();
    out
}

fn constraints_text(cs: &[Constraint]) -> String {
    canonical_constraints(cs)
        .iter()
        .map(|c| constraint_text(*c))
        .collect::<Vec<_>>()
        .join(",")
}

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn sort_evidence(evidence: &[Evidence]) -> Vec<Evidence> {
    let mut out = evidence.to_vec();
    out.sort_by(|a, b| a.constraint.cmp(&b.constraint).then_with(|| a.path.cmp(&b.path)));
    out
}

fn append_evidence(line: &mut String, evidence: &[Evidence]) {
    for item in sort_evidence(evidence) {
        line.push_str("\n  ");
        line.push_str(&constraint_text(item.constraint));
        line.push_str(" via ");
        line.push_str(&item.path.join(" -> "));
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
        ResolveError::DuplicateFeature {
            package,
            version,
            feature,
        } => format!(
            "duplicate-feature|package={}|version={}|feature={}",
            encode(package),
            version_text(*version),
            encode(feature)
        ),
        ResolveError::DuplicateLock { package } => {
            format!("duplicate-lock|package={}", encode(package))
        }
        ResolveError::Missing { package, evidence } => {
            let mut line = format!("missing|package={}", encode(package));
            append_evidence(&mut line, evidence);
            line
        }
        ResolveError::NoMatch {
            package,
            requirements,
            evidence,
        } => {
            let mut line = format!(
                "no-match|package={}|constraints={}",
                encode(package),
                constraints_text(requirements)
            );
            append_evidence(&mut line, evidence);
            line
        }
        ResolveError::Conflict {
            package,
            selected,
            requirements,
            evidence,
        } => {
            let mut line = format!(
                "conflict|package={}|selected={}|constraints={}",
                encode(package),
                version_text(*selected),
                constraints_text(requirements)
            );
            append_evidence(&mut line, evidence);
            line
        }
        ResolveError::MissingFeature {
            package,
            selected,
            feature,
            path,
        } => format!(
            "missing-feature|package={}|selected={}|feature={}\n  F:{} via {}",
            encode(package),
            version_text(*selected),
            encode(feature),
            encode(feature),
            path.join(" -> ")
        ),
        ResolveError::Limit { attempts } => format!("limit|attempts={attempts}"),
    }
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

fn permits(c: Constraint, version: Version) -> bool {
    match c {
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

fn platform_active(platform: Platform, target: Target) -> bool {
    matches!(
        (platform, target),
        (Platform::Any, _)
            | (Platform::Linux, Target::Linux)
            | (Platform::Mac, Target::Mac)
            | (Platform::Windows, Target::Windows)
    )
}

#[derive(Clone)]
struct Origin {
    constraint: Constraint,
    path: Vec<String>,
}

#[derive(Clone, Default)]
struct Closure {
    constraints: BTreeMap<String, Vec<Constraint>>,
    origins: BTreeMap<String, Vec<Origin>>,
    requested: BTreeMap<String, BTreeSet<String>>,
    activated: BTreeMap<String, BTreeSet<String>>,
    paths: BTreeMap<String, Vec<String>>,
}

fn canonical_path_insert(paths: &mut BTreeMap<String, Vec<String>>, package: String, path: Vec<String>) -> bool {
    match paths.get(&package) {
        Some(current) if (current.len(), current) <= (path.len(), &path) => false,
        _ => {
            paths.insert(package, path);
            true
        }
    }
}

fn build_closure(
    catalog: &BTreeMap<String, &Package>,
    roots: &[Requirement],
    selected: &BTreeMap<String, Version>,
    target: Target,
) -> Closure {
    let mut closure = Closure::default();

    for (i, root) in roots.iter().enumerate() {
        let mut features = root.features.clone();
        features.sort();
        features.dedup();
        let token = format!(
            "root[{}]:{}@{}+{{{}}}",
            i,
            root.package,
            constraint_text(root.constraint),
            features.join(",")
        );
        closure
            .constraints
            .entry(root.package.clone())
            .or_default()
            .push(root.constraint);
        closure
            .origins
            .entry(root.package.clone())
            .or_default()
            .push(Origin {
                constraint: root.constraint,
                path: vec![token.clone()],
            });
        closure.requested.entry(root.package.clone()).or_default().extend(features);
        canonical_path_insert(&mut closure.paths, root.package.clone(), vec![token]);
    }

    loop {
        let mut changed = false;

        // Propagate enabled features in the selected releases.
        let selected_names: Vec<String> = selected.keys().cloned().collect();
        for package_name in selected_names {
            let Some(package) = catalog.get(&package_name) else {
                continue;
            };
            let version = selected[&package_name];
            let Some(release) = package.releases.iter().find(|r| r.version == version) else {
                continue;
            };

            let requested: Vec<String> = closure
                .requested
                .get(&package_name)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect();

            for feature_name in requested {
                if closure
                    .activated
                    .entry(package_name.clone())
                    .or_default()
                    .insert(feature_name.clone())
                {
                    changed = true;
                }
                if let Some(feature) = release.features.iter().find(|f| f.name == feature_name) {
                    for enabled in &feature.enables {
                        if closure
                            .requested
                            .entry(package_name.clone())
                            .or_default()
                            .insert(enabled.clone())
                        {
                            changed = true;
                        }
                    }
                }
            }
        }

        // Propagate active dependencies from every selected package.
        let selected_names: Vec<String> = selected.keys().cloned().collect();
        for package_name in selected_names {
            let Some(package) = catalog.get(&package_name) else {
                continue;
            };
            let version = selected[&package_name];
            let Some(release) = package.releases.iter().find(|r| r.version == version) else {
                continue;
            };
            let Some(source_path) = closure.paths.get(&package_name).cloned() else {
                continue;
            };
            let activated = closure
                .activated
                .get(&package_name)
                .cloned()
                .unwrap_or_default();

            for (j, dep) in release.deps.iter().enumerate() {
                if !platform_active(dep.platform, target)
                    || dep.gate.as_ref().is_some_and(|g| !activated.contains(g))
                {
                    continue;
                }
                let mut features = dep.features.clone();
                features.sort();
                features.dedup();
                let gate = dep.gate.as_deref().unwrap_or("*");
                let platform = match dep.platform {
                    Platform::Any => "any",
                    Platform::Linux => "linux",
                    Platform::Mac => "mac",
                    Platform::Windows => "windows",
                };
                let token = format!(
                    "{}@{}/deps[{}]:{}@{}+{{{}}};gate={};platform={}",
                    package_name,
                    version_text(version),
                    j,
                    dep.package,
                    constraint_text(dep.constraint),
                    features.join(","),
                    gate,
                    platform
                );
                let mut path = source_path.clone();
                path.push(token);
                closure
                    .constraints
                    .entry(dep.package.clone())
                    .or_default()
                    .push(dep.constraint);
                closure
                    .origins
                    .entry(dep.package.clone())
                    .or_default()
                    .push(Origin {
                        constraint: dep.constraint,
                        path: path.clone(),
                    });
                let requested = closure.requested.entry(dep.package.clone()).or_default();
                for feature in features {
                    if requested.insert(feature) {
                        changed = true;
                    }
                }
                if canonical_path_insert(&mut closure.paths, dep.package.clone(), path) {
                    changed = true;
                }
            }
        }

        if !changed {
            break;
        }
    }

    for constraints in closure.constraints.values_mut() {
        *constraints = canonical_constraints(constraints);
    }
    closure
}

fn candidate_releases<'a>(
    package: &'a Package,
    constraints: &[Constraint],
    lock: Option<Version>,
) -> Vec<&'a Release> {
    let mut candidates: Vec<&Release> = package
        .releases
        .iter()
        .filter(|release| constraints.iter().all(|c| permits(*c, release.version)))
        .collect();
    candidates.sort_by(|a, b| b.version.cmp(&a.version));
    if let Some(lock_version) = lock {
        if let Some(index) = candidates.iter().position(|r| r.version == lock_version) {
            let locked = candidates.remove(index);
            candidates.insert(0, locked);
        }
    }
    candidates
}

fn evidence_for(closure: &Closure, package: &str) -> Vec<Evidence> {
    closure
        .origins
        .get(package)
        .into_iter()
        .flatten()
        .map(|origin| Evidence {
            constraint: origin.constraint,
            path: origin.path.clone(),
        })
        .collect()
}

fn inspect_node(
    catalog: &BTreeMap<String, &Package>,
    roots: &[Requirement],
    selected: &BTreeMap<String, Version>,
    target: Target,
) -> Result<(Closure, Option<String>), ResolveError> {
    let closure = build_closure(catalog, roots, selected, target);

    if let Some(package) = closure
        .constraints
        .keys()
        .filter(|name| !catalog.contains_key(*name))
        .next()
    {
        return Err(ResolveError::Missing {
            package: package.clone(),
            evidence: evidence_for(&closure, package),
        });
    }

    for (package, version) in selected {
        let Some(constraints) = closure.constraints.get(package) else {
            continue;
        };

        if let Some(feature) = closure
            .activated
            .get(package)
            .into_iter()
            .flatten()
            .find(|feature| {
                !catalog[package]
                    .releases
                    .iter()
                    .find(|r| r.version == *version)
                    .is_some_and(|r| r.features.iter().any(|f| f.name == ***feature))
            })
        {
            let path = closure.paths.get(package).cloned().unwrap_or_default();
            return Err(ResolveError::MissingFeature {
                package: package.clone(),
                selected: *version,
                feature: feature.clone(),
                path,
            });
        }

        if !constraints.iter().all(|c| permits(*c, *version)) {
            return Err(ResolveError::Conflict {
                package: package.clone(),
                selected: *version,
                requirements: constraints.clone(),
                evidence: evidence_for(&closure, package),
            });
        }
    }

    for (package, constraints) in &closure.constraints {
        if selected.contains_key(package) {
            continue;
        }
        let releases = catalog[package];
        if candidate_releases(releases, constraints, None).is_empty() {
            return Err(ResolveError::NoMatch {
                package: package.clone(),
                requirements: constraints.clone(),
                evidence: evidence_for(&closure, package),
            });
        }
    }

    let next = closure
        .constraints
        .keys()
        .find(|package| !selected.contains_key(*package))
        .cloned();
    Ok((closure, next))
}

struct Search<'a> {
    catalog: &'a BTreeMap<String, &'a Package>,
    roots: &'a [Requirement],
    locks: &'a BTreeMap<String, Version>,
    target: Target,
    max_attempts: usize,
    attempts: usize,
    best_cost: Option<usize>,
    best_vector: Vec<Option<Version>>,
    best_selected: Option<BTreeMap<String, Version>>,
    first_error: Option<ResolveError>,
    catalog_names: Vec<String>,
}

impl Search<'_> {
    fn solution_cost(&self, selected: &BTreeMap<String, Version>) -> usize {
        self.locks
            .iter()
            .filter(|(package, version)| selected.get(*package) != Some(*version))
            .count()
    }

    fn vector(&self, selected: &BTreeMap<String, Version>) -> Vec<Option<Version>> {
        self.catalog_names
            .iter()
            .map(|name| selected.get(name).copied())
            .collect()
    }

    fn visit(&mut self, selected: &mut BTreeMap<String, Version>) -> Result<(), ResolveError> {
        let (closure, next) = match inspect_node(self.catalog, self.roots, selected, self.target) {
            Ok(result) => result,
            Err(error) => {
                if self.first_error.is_none() {
                    self.first_error = Some(error);
                }
                return Ok(());
            }
        };

        let Some(package_name) = next else {
            let cost = self.solution_cost(selected);
            let vector = self.vector(selected);
            let better = self.best_cost.is_none_or(|best_cost| {
                cost < best_cost || (cost == best_cost && vector > self.best_vector)
            });
            if better {
                self.best_cost = Some(cost);
                self.best_vector = vector;
                self.best_selected = Some(selected.clone());
            }
            return Ok(());
        };

        let constraints = closure
            .constraints
            .get(&package_name)
            .cloned()
            .unwrap_or_default();
        let package = self.catalog[&package_name];
        let candidates = candidate_releases(
            package,
            &constraints,
            self.locks.get(&package_name).copied(),
        );

        for release in candidates {
            if self.attempts == self.max_attempts {
                return Err(ResolveError::Limit {
                    attempts: self.max_attempts,
                });
            }
            self.attempts += 1;
            selected.insert(package_name.clone(), release.version);
            let result = self.visit(selected);
            selected.remove(&package_name);
            result?;
        }
        Ok(())
    }
}

fn validate_name(site: String, name: &str) -> Result<(), ResolveError> {
    if valid_name(name) {
        Ok(())
    } else {
        Err(ResolveError::InvalidName {
            site,
            name: name.to_string(),
        })
    }
}

pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
    locks: &[Lock],
    target: Target,
    max_attempts: usize,
) -> Result<Resolution, ResolveError> {
    for (i, root) in roots.iter().enumerate() {
        validate_name(format!("root[{i}]"), &root.package)?;
        for (f, feature) in root.features.iter().enumerate() {
            validate_name(format!("root[{i}].feature[{f}]"), feature)?;
        }
    }

    for (i, package) in catalog.iter().enumerate() {
        validate_name(format!("catalog[{i}]"), &package.name)?;
        for (j, release) in package.releases.iter().enumerate() {
            for (k, dep) in release.deps.iter().enumerate() {
                validate_name(
                    format!("catalog[{i}].release[{j}].dep[{k}]"),
                    &dep.package,
                )?;
                if let Some(gate) = &dep.gate {
                    validate_name(
                        format!("catalog[{i}].release[{j}].dep[{k}].gate"),
                        gate,
                    )?;
                }
                for (f, feature) in dep.features.iter().enumerate() {
                    validate_name(
                        format!("catalog[{i}].release[{j}].dep[{k}].feature[{f}]"),
                        feature,
                    )?;
                }
            }
            for (f, feature) in release.features.iter().enumerate() {
                validate_name(
                    format!("catalog[{i}].release[{j}].feature[{f}]"),
                    &feature.name,
                )?;
                for (e, enabled) in feature.enables.iter().enumerate() {
                    validate_name(
                        format!("catalog[{i}].release[{j}].feature[{f}].enable[{e}]"),
                        enabled,
                    )?;
                }
            }
        }
    }

    let mut package_names = BTreeSet::new();
    for package in catalog {
        if !package_names.insert(package.name.clone()) {
            return Err(ResolveError::DuplicatePackage {
                package: package.name.clone(),
            });
        }
    }

    for package in catalog {
        let mut versions = BTreeSet::new();
        for release in &package.releases {
            if !versions.insert(release.version) {
                return Err(ResolveError::DuplicateVersion {
                    package: package.name.clone(),
                    version: release.version,
                });
            }
        }
    }

    for package in catalog {
        for release in &package.releases {
            let mut features = BTreeSet::new();
            for feature in &release.features {
                if !features.insert(feature.name.clone()) {
                    return Err(ResolveError::DuplicateFeature {
                        package: package.name.clone(),
                        version: release.version,
                        feature: feature.name.clone(),
                    });
                }
            }
        }
    }

    for (i, lock) in locks.iter().enumerate() {
        validate_name(format!("lock[{i}]"), &lock.package)?;
    }
    let mut lock_names = BTreeSet::new();
    for lock in locks {
        if !lock_names.insert(lock.package.clone()) {
            return Err(ResolveError::DuplicateLock {
                package: lock.package.clone(),
            });
        }
    }

    let catalog_map: BTreeMap<String, &Package> = catalog
        .iter()
        .map(|package| (package.name.clone(), package))
        .collect();
    let lock_map: BTreeMap<String, Version> = locks
        .iter()
        .map(|lock| (lock.package.clone(), lock.version))
        .collect();
    let mut catalog_names: Vec<String> = catalog_map.keys().cloned().collect();

    let mut search = Search {
        catalog: &catalog_map,
        roots,
        locks: &lock_map,
        target,
        max_attempts,
        attempts: 0,
        best_cost: None,
        best_vector: Vec::new(),
        best_selected: None,
        first_error: None,
        catalog_names: std::mem::take(&mut catalog_names),
    };

    let mut selected = BTreeMap::new();
    search.visit(&mut selected)?;

    let Some(final_selected) = search.best_selected else {
        return Err(search.first_error.unwrap_or(ResolveError::Missing {
            package: String::new(),
            evidence: Vec::new(),
        }));
    };

    let final_closure = build_closure(&catalog_map, roots, &final_selected, target);
    let selections: Vec<Selection> = final_selected
        .iter()
        .map(|(package, version)| Selection {
            package: package.clone(),
            version: *version,
        })
        .collect();

    let mut all_names: BTreeSet<String> = lock_map.keys().cloned().collect();
    all_names.extend(final_selected.keys().cloned());
    let mut changes = Vec::new();

    for package in all_names {
        let before = lock_map.get(&package).copied();
        let after = final_selected.get(&package).copied();
        if before == after {
            continue;
        }

        let reason = match (before, after) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old), Some(_)) => {
                let exists = catalog_map
                    .get(&package)
                    .is_some_and(|p| p.releases.iter().any(|r| r.version == old));
                if !exists {
                    ChangeReason::Unavailable
                } else if final_closure
                    .constraints
                    .get(&package)
                    .is_some_and(|cs| !cs.iter().all(|c| permits(*c, old)))
                {
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

    Ok(Resolution {
        selected: selections,
        changes,
    })
}