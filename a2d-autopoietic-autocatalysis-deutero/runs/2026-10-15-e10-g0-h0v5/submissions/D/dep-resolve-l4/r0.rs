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

fn canonical_constraints(input: &[Constraint]) -> Vec<Constraint> {
    let mut out = input.to_vec();
    out.sort();
    out.dedup();
    out
}

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

fn constraint_list(input: &[Constraint]) -> String {
    canonical_constraints(input)
        .iter()
        .map(|c| constraint_text(*c))
        .collect::<Vec<_>>()
        .join(",")
}

fn evidence_lines(evidence: &[Evidence]) -> Vec<String> {
    let mut entries = evidence.to_vec();
    entries.sort_by(|a, b| {
        a.constraint
            .cmp(&b.constraint)
            .then_with(|| a.path.cmp(&b.path))
    });
    entries
        .iter()
        .map(|e| {
            format!(
                "\n  C:{} via {}",
                constraint_text(e.constraint),
                e.path.join(" -> ")
            )
        })
        .collect()
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
            let mut s = format!("missing|package={}", encode(package));
            s.push_str(&evidence_lines(evidence).join(""));
            s
        }
        ResolveError::NoMatch {
            package,
            requirements,
            evidence,
        } => {
            let mut s = format!(
                "no-match|package={}|constraints={}",
                encode(package),
                constraint_list(requirements)
            );
            s.push_str(&evidence_lines(evidence).join(""));
            s
        }
        ResolveError::Conflict {
            package,
            selected,
            requirements,
            evidence,
        } => {
            let mut s = format!(
                "conflict|package={}|selected={}|constraints={}",
                encode(package),
                version_text(*selected),
                constraint_list(requirements)
            );
            s.push_str(&evidence_lines(evidence).join(""));
            s
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
        ResolveError::Limit { attempts } => format!("limit|attempts={}", attempts),
    }
}

fn matches(c: Constraint, v: Version) -> bool {
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

fn platform_matches(platform: Platform, target: Target) -> bool {
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
struct Active {
    origins: BTreeMap<String, Vec<Origin>>,
    features: BTreeMap<String, BTreeSet<String>>,
    paths: BTreeMap<String, Vec<String>>,
}

fn canonical_path_better(a: &[String], b: &[String]) -> bool {
    a.len() < b.len() || (a.len() == b.len() && a < b)
}

fn dep_token(source: &str, version: Version, index: usize, dep: &Dependency) -> String {
    let mut fs = dep.features.clone();
    fs.sort();
    fs.dedup();
    let gate = dep.gate.as_deref().unwrap_or("*");
    let platform = match dep.platform {
        Platform::Any => "any",
        Platform::Linux => "linux",
        Platform::Mac => "mac",
        Platform::Windows => "windows",
    };
    format!(
        "{}@{}/deps[{}]:{}@{}+{{{}}};gate={};platform={}",
        source,
        version_text(version),
        index,
        dep.package,
        constraint_text(dep.constraint),
        fs.join(","),
        gate,
        platform
    )
}

fn root_token(index: usize, req: &Requirement) -> String {
    let mut fs = req.features.clone();
    fs.sort();
    fs.dedup();
    format!(
        "root[{}]:{}@{}+{{{}}}",
        index,
        req.package,
        constraint_text(req.constraint),
        fs.join(",")
    )
}

fn build_active(
    roots: &[Requirement],
    catalog: &BTreeMap<String, &Package>,
    selected: &BTreeMap<String, Version>,
    target: Target,
) -> Active {
    let mut active = Active::default();

    for (i, req) in roots.iter().enumerate() {
        active
            .origins
            .entry(req.package.clone())
            .or_default()
            .push(Origin {
                constraint: req.constraint,
                path: vec![root_token(i, req)],
            });
        active
            .features
            .entry(req.package.clone())
            .or_default()
            .extend(req.features.iter().cloned());
        active
            .paths
            .entry(req.package.clone())
            .and_modify(|old| {
                let new = vec![root_token(i, req)];
                if canonical_path_better(&new, old) {
                    *old = new;
                }
            })
            .or_insert_with(|| vec![root_token(i, req)]);
    }

    let mut processed_deps: BTreeSet<(String, Version, usize)> = BTreeSet::new();
    loop {
        let mut changed = false;

        for (package, version) in selected {
            let Some(pkg) = catalog.get(package) else {
                continue;
            };
            let Some(release) = pkg.releases.iter().find(|r| r.version == *version) else {
                continue;
            };

            let requested = active.features.get(package).cloned().unwrap_or_default();
            for feature_name in requested {
                if let Some(decl) = release.features.iter().find(|f| f.name == feature_name) {
                    let set = active.features.entry(package.clone()).or_default();
                    for enabled in &decl.enables {
                        if set.insert(enabled.clone()) {
                            changed = true;
                        }
                    }
                }
            }
        }

        for (package, version) in selected {
            let Some(pkg) = catalog.get(package) else {
                continue;
            };
            let Some(release) = pkg.releases.iter().find(|r| r.version == *version) else {
                continue;
            };

            let source_features = active.features.get(package).cloned().unwrap_or_default();
            for (index, dep) in release.deps.iter().enumerate() {
                if !platform_matches(dep.platform, target)
                    || dep
                        .gate
                        .as_ref()
                        .is_some_and(|g| !source_features.contains(g))
                {
                    continue;
                }

                let key = (package.clone(), *version, index);
                if !processed_deps.insert(key) {
                    continue;
                }
                changed = true;

                let source_path = active.paths.get(package).cloned().unwrap_or_default();
                let mut path = source_path;
                path.push(dep_token(package, *version, index, dep));

                active
                    .origins
                    .entry(dep.package.clone())
                    .or_default()
                    .push(Origin {
                        constraint: dep.constraint,
                        path: path.clone(),
                    });

                let dest_features = active.features.entry(dep.package.clone()).or_default();
                for feature in &dep.features {
                    if dest_features.insert(feature.clone()) {
                        changed = true;
                    }
                }

                match active.paths.get(&dep.package) {
                    None => {
                        active.paths.insert(dep.package.clone(), path);
                    }
                    Some(old) if canonical_path_better(&path, old) => {
                        active.paths.insert(dep.package.clone(), path);
                    }
                    _ => {}
                }
            }
        }

        if !changed {
            break;
        }
    }

    // Rebuild dependency evidence using canonical source paths after closure.
    for origins in active.origins.values_mut() {
        origins.retain(|origin| origin.path.len() == 1 && origin.path[0].starts_with("root["));
    }
    for (package, version) in selected {
        if let Some(pkg) = catalog.get(package) {
            if let Some(release) = pkg.releases.iter().find(|r| r.version == *version) {
                let source_path = active.paths.get(package).cloned().unwrap_or_default();
                let source_features = active.features.get(package).cloned().unwrap_or_default();
                for (index, dep) in release.deps.iter().enumerate() {
                    if platform_matches(dep.platform, target)
                        && dep.gate.as_ref().is_none_or(|g| source_features.contains(g))
                    {
                        let mut path = source_path.clone();
                        path.push(dep_token(package, *version, index, dep));
                        active
                            .origins
                            .entry(dep.package.clone())
                            .or_default()
                            .push(Origin {
                                constraint: dep.constraint,
                                path,
                            });
                    }
                }
            }
        }
    }

    active
}

fn requirements_for(active: &Active, package: &str) -> Vec<Constraint> {
    let constraints = active
        .origins
        .get(package)
        .into_iter()
        .flatten()
        .map(|origin| origin.constraint)
        .collect::<Vec<_>>();
    canonical_constraints(&constraints)
}

fn evidence_for(active: &Active, package: &str) -> Vec<Evidence> {
    active
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

fn node_error(
    active: &Active,
    catalog: &BTreeMap<String, &Package>,
    selected: &BTreeMap<String, Version>,
) -> Option<ResolveError> {
    for package in active.origins.keys() {
        if !catalog.contains_key(package) {
            return Some(ResolveError::Missing {
                package: package.clone(),
                evidence: evidence_for(active, package),
            });
        }
    }

    for (package, version) in selected {
        let reqs = requirements_for(active, package);
        if !reqs.iter().all(|c| matches(*c, *version)) {
            return Some(ResolveError::Conflict {
                package: package.clone(),
                selected: *version,
                requirements: reqs,
                evidence: evidence_for(active, package),
            });
        }
    }

    for (package, version) in selected {
        let Some(pkg) = catalog.get(package) else {
            continue;
        };
        let Some(release) = pkg.releases.iter().find(|r| r.version == *version) else {
            continue;
        };
        let requested = active.features.get(package).cloned().unwrap_or_default();
        if let Some(feature) = requested
            .into_iter()
            .find(|name| !release.features.iter().any(|f| f.name == *name))
        {
            return Some(ResolveError::MissingFeature {
                package: package.clone(),
                selected: *version,
                feature,
                path: active.paths.get(package).cloned().unwrap_or_default(),
            });
        }
    }

    for package in active.origins.keys() {
        if selected.contains_key(package) {
            continue;
        }
        let reqs = requirements_for(active, package);
        let pkg = catalog[package];
        if !pkg
            .releases
            .iter()
            .any(|r| reqs.iter().all(|c| matches(*c, r.version)))
        {
            return Some(ResolveError::NoMatch {
                package: package.clone(),
                requirements: reqs,
                evidence: evidence_for(active, package),
            });
        }
    }

    None
}

#[derive(Clone)]
struct SearchResult {
    selected: BTreeMap<String, Version>,
    active: Active,
}

struct Search<'a> {
    roots: &'a [Requirement],
    catalog: BTreeMap<String, &'a Package>,
    locks: BTreeMap<String, Version>,
    target: Target,
    max_attempts: usize,
    attempts: usize,
    best: Option<(usize, Vec<Option<Version>>, SearchResult)>,
    first_error: Option<ResolveError>,
}

impl Search<'_> {
    fn visit(&mut self, selected: &mut BTreeMap<String, Version>) -> Result<(), ResolveError> {
        let active = build_active(self.roots, &self.catalog, selected, self.target);
        if let Some(error) = node_error(&active, &self.catalog, selected) {
            if self.first_error.is_none() {
                self.first_error = Some(error);
            }
            return Ok(());
        }

        let next = active
            .origins
            .keys()
            .find(|package| !selected.contains_key(*package))
            .cloned();

        let Some(package) = next else {
            let cost = self
                .locks
                .iter()
                .filter(|(package, version)| selected.get(*package) != Some(*version))
                .count();
            let vector = self
                .catalog
                .keys()
                .map(|package| selected.get(package).copied())
                .collect::<Vec<_>>();
            let result = SearchResult {
                selected: selected.clone(),
                active,
            };
            let replace = match &self.best {
                None => true,
                Some((best_cost, best_vector, _)) => {
                    cost < *best_cost
                        || (cost == *best_cost && vector_lex_greater(&vector, best_vector))
                }
            };
            if replace {
                self.best = Some((cost, vector, result));
            }
            return Ok(());
        };

        let reqs = requirements_for(&active, &package);
        let mut candidates = self.catalog[&package]
            .releases
            .iter()
            .filter(|release| reqs.iter().all(|c| matches(*c, release.version)))
            .map(|release| release.version)
            .collect::<Vec<_>>();
        candidates.sort_by(|a, b| b.cmp(a));

        if let Some(lock) = self.locks.get(&package) {
            if let Some(position) = candidates.iter().position(|version| version == lock) {
                let version = candidates.remove(position);
                candidates.insert(0, version);
            }
        }

        for version in candidates {
            if self.attempts == self.max_attempts {
                return Err(ResolveError::Limit {
                    attempts: self.max_attempts,
                });
            }
            self.attempts += 1;
            selected.insert(package.clone(), version);
            self.visit(selected)?;
            selected.remove(&package);
        }
        Ok(())
    }
}

fn vector_lex_greater(a: &[Option<Version>], b: &[Option<Version>]) -> bool {
    for (x, y) in a.iter().zip(b) {
        match (x, y) {
            (Some(x), Some(y)) if x != y => return x > y,
            (Some(_), None) => return true,
            (None, Some(_)) => return false,
            _ => {}
        }
    }
    false
}

pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
    locks: &[Lock],
    target: Target,
    max_attempts: usize,
) -> Result<Resolution, ResolveError> {
    for (index, requirement) in roots.iter().enumerate() {
        if !valid_name(&requirement.package) {
            return Err(ResolveError::InvalidName {
                site: format!("root[{}]", index),
                name: requirement.package.clone(),
            });
        }
        for (feature_index, feature) in requirement.features.iter().enumerate() {
            if !valid_name(feature) {
                return Err(ResolveError::InvalidName {
                    site: format!("root[{}].feature[{}]", index, feature_index),
                    name: feature.clone(),
                });
            }
        }
    }

    for (package_index, package) in catalog.iter().enumerate() {
        if !valid_name(&package.name) {
            return Err(ResolveError::InvalidName {
                site: format!("catalog[{}]", package_index),
                name: package.name.clone(),
            });
        }
        for (release_index, release) in package.releases.iter().enumerate() {
            for (dep_index, dependency) in release.deps.iter().enumerate() {
                let dep_site = format!(
                    "catalog[{}].release[{}].dep[{}]",
                    package_index, release_index, dep_index
                );
                if !valid_name(&dependency.package) {
                    return Err(ResolveError::InvalidName {
                        site: dep_site.clone(),
                        name: dependency.package.clone(),
                    });
                }
                if let Some(gate) = &dependency.gate {
                    if !valid_name(gate) {
                        return Err(ResolveError::InvalidName {
                            site: format!("{}.gate", dep_site),
                            name: gate.clone(),
                        });
                    }
                }
                for (feature_index, feature) in dependency.features.iter().enumerate() {
                    if !valid_name(feature) {
                        return Err(ResolveError::InvalidName {
                            site: format!("{}.feature[{}]", dep_site, feature_index),
                            name: feature.clone(),
                        });
                    }
                }
            }
            for (feature_index, feature) in release.features.iter().enumerate() {
                if !valid_name(&feature.name) {
                    return Err(ResolveError::InvalidName {
                        site: format!(
                            "catalog[{}].release[{}].feature[{}]",
                            package_index, release_index, feature_index
                        ),
                        name: feature.name.clone(),
                    });
                }
                for (enable_index, enabled) in feature.enables.iter().enumerate() {
                    if !valid_name(enabled) {
                        return Err(ResolveError::InvalidName {
                            site: format!(
                                "catalog[{}].release[{}].feature[{}].enable[{}]",
                                package_index, release_index, feature_index, enable_index
                            ),
                            name: enabled.clone(),
                        });
                    }
                }
            }
        }
    }

    let mut seen_packages = BTreeSet::new();
    for package in catalog {
        if !seen_packages.insert(package.name.clone()) {
            return Err(ResolveError::DuplicatePackage {
                package: package.name.clone(),
            });
        }
    }

    for package in catalog {
        let mut seen_versions = BTreeSet::new();
        for release in &package.releases {
            if !seen_versions.insert(release.version) {
                return Err(ResolveError::DuplicateVersion {
                    package: package.name.clone(),
                    version: release.version,
                });
            }
        }
    }

    for package in catalog {
        for release in &package.releases {
            let mut seen_features = BTreeSet::new();
            for feature in &release.features {
                if !seen_features.insert(feature.name.clone()) {
                    return Err(ResolveError::DuplicateFeature {
                        package: package.name.clone(),
                        version: release.version,
                        feature: feature.name.clone(),
                    });
                }
            }
        }
    }

    for (index, lock) in locks.iter().enumerate() {
        if !valid_name(&lock.package) {
            return Err(ResolveError::InvalidName {
                site: format!("lock[{}]", index),
                name: lock.package.clone(),
            });
        }
    }
    let mut seen_locks = BTreeSet::new();
    for lock in locks {
        if !seen_locks.insert(lock.package.clone()) {
            return Err(ResolveError::DuplicateLock {
                package: lock.package.clone(),
            });
        }
    }

    let catalog_map = catalog
        .iter()
        .map(|package| (package.name.clone(), package))
        .collect::<BTreeMap<_, _>>();
    let lock_map = locks
        .iter()
        .map(|lock| (lock.package.clone(), lock.version))
        .collect::<BTreeMap<_, _>>();

    let mut search = Search {
        roots,
        catalog: catalog_map.clone(),
        locks: lock_map.clone(),
        target,
        max_attempts,
        attempts: 0,
        best: None,
        first_error: None,
    };
    search.visit(&mut BTreeMap::new())?;

    let Some((_, _, result)) = search.best else {
        return Err(search.first_error.unwrap_or(ResolveError::NoMatch {
            package: String::new(),
            requirements: vec![],
            evidence: vec![],
        }));
    };

    let selected = result
        .selected
        .iter()
        .map(|(package, version)| Selection {
            package: package.clone(),
            version: *version,
        })
        .collect::<Vec<_>>();

    let mut all_packages = BTreeSet::new();
    all_packages.extend(locks.iter().map(|lock| lock.package.clone()));
    all_packages.extend(result.selected.keys().cloned());

    let mut changes = Vec::new();
    for package in all_packages {
        let before = lock_map.get(&package).copied();
        let after = result.selected.get(&package).copied();
        if before == after {
            continue;
        }
        let reason = match (before, after) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old), Some(_)) => {
                let exists = catalog_map
                    .get(&package)
                    .is_some_and(|catalog_package| {
                        catalog_package
                            .releases
                            .iter()
                            .any(|release| release.version == old)
                    });
                if !exists {
                    ChangeReason::Unavailable
                } else {
                    let requirements = requirements_for(&result.active, &package);
                    if !requirements
                        .iter()
                        .all(|constraint| matches(*constraint, old))
                    {
                        ChangeReason::Rejected
                    } else {
                        ChangeReason::Rebalanced
                    }
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