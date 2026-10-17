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

fn constraint_rank(c: Constraint) -> (u8, Version) {
    match c {
        Constraint::Exact(v) => (0, v),
        Constraint::AtLeast(v) => (1, v),
        Constraint::Before(v) => (2, v),
        Constraint::Caret(v) => (3, v),
    }
}

fn constraint_text(c: Constraint) -> String {
    match c {
        Constraint::Exact(v) => format!("={}", version_text(v)),
        Constraint::AtLeast(v) => format!(">={}", version_text(v)),
        Constraint::Before(v) => format!("<{}", version_text(v)),
        Constraint::Caret(v) => format!("^{}", version_text(v)),
    }
}

fn canonical_constraints(mut constraints: Vec<Constraint>) -> Vec<Constraint> {
    constraints.sort_by_key(|c| constraint_rank(*c));
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

fn encode(s: &str) -> String {
    let mut out = String::new();
    for &byte in s.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{:02X}", byte));
        }
    }
    out
}

fn evidence_sorted(mut evidence: Vec<Evidence>) -> Vec<Evidence> {
    evidence.sort_by(|a, b| {
        constraint_rank(a.constraint)
            .cmp(&constraint_rank(b.constraint))
            .then_with(|| a.path.cmp(&b.path))
    });
    evidence
}

fn append_evidence(mut first: String, evidence: &[Evidence]) -> String {
    for item in evidence_sorted(evidence.to_vec()) {
        first.push_str("\n  C:");
        first.push_str(&constraint_text(item.constraint));
        first.push_str(" via ");
        first.push_str(&item.path.join(" -> "));
    }
    first
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
        ResolveError::Missing { package, evidence } => append_evidence(
            format!("missing|package={}", encode(package)),
            evidence,
        ),
        ResolveError::NoMatch {
            package,
            requirements,
            evidence,
        } => append_evidence(
            format!(
                "no-match|package={}|constraints={}",
                encode(package),
                constraints_text(requirements)
            ),
            evidence,
        ),
        ResolveError::Conflict {
            package,
            selected,
            requirements,
            evidence,
        } => append_evidence(
            format!(
                "conflict|package={}|selected={}|constraints={}",
                encode(package),
                version_text(*selected),
                constraints_text(requirements)
            ),
            evidence,
        ),
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

fn satisfies(version: Version, constraint: Constraint) -> bool {
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

fn target_matches(platform: Platform, target: Target) -> bool {
    platform == Platform::Any
        || matches!(
            (platform, target),
            (Platform::Linux, Target::Linux)
                | (Platform::Mac, Target::Mac)
                | (Platform::Windows, Target::Windows)
        )
}

fn sorted_features(features: &[String]) -> Vec<String> {
    let mut result = features.to_vec();
    result.sort();
    result.dedup();
    result
}

fn root_token(root: &Requirement, index: usize) -> String {
    format!(
        "root[{}]:{}@{}+{{{}}}",
        index,
        root.package,
        constraint_text(root.constraint),
        sorted_features(&root.features).join(",")
    )
}

fn dependency_token(source: &str, version: Version, index: usize, dep: &Dependency) -> String {
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
        sorted_features(&dep.features).join(","),
        gate,
        platform
    )
}

fn better_path(a: &[String], b: &[String]) -> bool {
    a.len() < b.len() || (a.len() == b.len() && a < b)
}

#[derive(Clone, Debug)]
struct Origin {
    constraint: Constraint,
    path: Vec<String>,
}

#[derive(Clone, Debug, Default)]
struct PackageInfo {
    origins: Vec<Origin>,
    features: BTreeSet<String>,
}

#[derive(Clone, Debug, Default)]
struct NodeInfo {
    packages: BTreeMap<String, PackageInfo>,
}

impl NodeInfo {
    fn add_origin(&mut self, package: String, constraint: Constraint, path: Vec<String>) -> bool {
        self.packages
            .entry(package)
            .or_default()
            .origins
            .push(Origin { constraint, path });
        true
    }

    fn add_feature(&mut self, package: String, feature: String) -> bool {
        self.packages
            .entry(package)
            .or_default()
            .features
            .insert(feature)
    }
}

fn canonical_path(info: &PackageInfo) -> Vec<String> {
    info.origins
        .iter()
        .map(|origin| origin.path.clone())
        .min_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)))
        .unwrap_or_default()
}

fn package_evidence(info: &PackageInfo) -> Vec<Evidence> {
    evidence_sorted(
        info.origins
            .iter()
            .map(|origin| Evidence {
                constraint: origin.constraint,
                path: origin.path.clone(),
            })
            .collect(),
    )
}

fn package_constraints(info: &PackageInfo) -> Vec<Constraint> {
    canonical_constraints(
        info.origins
            .iter()
            .map(|origin| origin.constraint)
            .collect(),
    )
}

fn release_matches(release: &Release, constraints: &[Constraint]) -> bool {
    constraints
        .iter()
        .all(|constraint| satisfies(release.version, *constraint))
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
        for (f, feature) in root.features.iter().enumerate() {
            if !valid_name(feature) {
                return Err(ResolveError::InvalidName {
                    site: format!("root[{}].feature[{}]", i, f),
                    name: feature.clone(),
                });
            }
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
                if let Some(gate) = &dep.gate {
                    if !valid_name(gate) {
                        return Err(ResolveError::InvalidName {
                            site: format!("catalog[{}].release[{}].dep[{}].gate", i, j, k),
                            name: gate.clone(),
                        });
                    }
                }
                for (f, feature) in dep.features.iter().enumerate() {
                    if !valid_name(feature) {
                        return Err(ResolveError::InvalidName {
                            site: format!(
                                "catalog[{}].release[{}].dep[{}].feature[{}]",
                                i, j, k, f
                            ),
                            name: feature.clone(),
                        });
                    }
                }
            }
            for (f, feature) in release.features.iter().enumerate() {
                if !valid_name(&feature.name) {
                    return Err(ResolveError::InvalidName {
                        site: format!("catalog[{}].release[{}].feature[{}]", i, j, f),
                        name: feature.name.clone(),
                    });
                }
                for (e, enabled) in feature.enables.iter().enumerate() {
                    if !valid_name(enabled) {
                        return Err(ResolveError::InvalidName {
                            site: format!(
                                "catalog[{}].release[{}].feature[{}].enable[{}]",
                                i, j, f, e
                            ),
                            name: enabled.clone(),
                        });
                    }
                }
            }
        }
    }

    let mut packages = BTreeSet::new();
    for package in catalog {
        if !packages.insert(package.name.as_str()) {
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
                if !features.insert(feature.name.as_str()) {
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
        if !valid_name(&lock.package) {
            return Err(ResolveError::InvalidName {
                site: format!("lock[{}]", i),
                name: lock.package.clone(),
            });
        }
    }
    let mut lock_names = BTreeSet::new();
    for lock in locks {
        if !lock_names.insert(lock.package.as_str()) {
            return Err(ResolveError::DuplicateLock {
                package: lock.package.clone(),
            });
        }
    }

    Ok(())
}

#[derive(Clone)]
struct SearchResult {
    selected: BTreeMap<String, Version>,
    constraints: BTreeMap<String, Vec<Constraint>>,
}

struct Search<'a> {
    catalog: &'a [Package],
    roots: &'a [Requirement],
    locks: &'a [Lock],
    target: Target,
    max_attempts: usize,
    attempts: usize,
    first_error: Option<ResolveError>,
    best: Option<SearchResult>,
    best_cost: usize,
}

impl<'a> Search<'a> {
    fn catalog_map(&self) -> BTreeMap<&str, &Package> {
        self.catalog
            .iter()
            .map(|package| (package.name.as_str(), package))
            .collect()
    }

    fn build_node(&self, assigned: &BTreeMap<String, Version>) -> NodeInfo {
        let packages = self.catalog_map();
        let mut node = NodeInfo::default();

        for (index, root) in self.roots.iter().enumerate() {
            let path = vec![root_token(root, index)];
            node.add_origin(root.package.clone(), root.constraint, path);
            for feature in &root.features {
                node.add_feature(root.package.clone(), feature.clone());
            }
        }

        // Recompute closure from scratch. Each dependency origin is identified by
        // its source package and original release dependency index.
        let mut processed_features: BTreeSet<(String, String)> = BTreeSet::new();
        let mut processed_deps: BTreeSet<(String, usize)> = BTreeSet::new();

        loop {
            let mut changed = false;
            let selected_names: Vec<String> = assigned.keys().cloned().collect();

            for source in selected_names {
                let Some(info) = node.packages.get(&source) else {
                    continue;
                };
                let Some(version) = assigned.get(&source).copied() else {
                    continue;
                };
                let Some(package) = packages.get(source.as_str()).copied() else {
                    continue;
                };
                let Some(release) = package.releases.iter().find(|r| r.version == version) else {
                    continue;
                };

                let feature_names: Vec<String> = info.features.iter().cloned().collect();
                for feature_name in feature_names {
                    if !processed_features.insert((source.clone(), feature_name.clone())) {
                        continue;
                    }
                    if let Some(feature) = release.features.iter().find(|f| f.name == feature_name) {
                        for enabled in &feature.enables {
                            node.add_feature(source.clone(), enabled.clone());
                        }
                    }
                    changed = true;
                }

                let current_features = node
                    .packages
                    .get(&source)
                    .map(|p| p.features.clone())
                    .unwrap_or_default();
                for (dep_index, dep) in release.deps.iter().enumerate() {
                    if !target_matches(dep.platform, self.target)
                        || dep
                            .gate
                            .as_ref()
                            .is_some_and(|gate| !current_features.contains(gate))
                    {
                        continue;
                    }
                    if !processed_deps.insert((source.clone(), dep_index)) {
                        continue;
                    }

                    let source_path = node
                        .packages
                        .get(&source)
                        .map(canonical_path)
                        .unwrap_or_default();
                    let mut path = source_path;
                    path.push(dependency_token(&source, version, dep_index, dep));
                    node.add_origin(dep.package.clone(), dep.constraint, path);
                    for feature in &dep.features {
                        node.add_feature(dep.package.clone(), feature.clone());
                    }
                    changed = true;
                }
            }

            if !changed {
                break;
            }
        }

        node
    }

    fn record_error<T>(&mut self, error: ResolveError) -> Result<T, ResolveError> {
        if self.first_error.is_none() {
            self.first_error = Some(error);
        }
        Err(self.first_error.as_ref().unwrap().clone())
    }

    fn vector_better(
        &self,
        a: &BTreeMap<String, Version>,
        b: &BTreeMap<String, Version>,
    ) -> bool {
        let mut names: Vec<&str> = self.catalog.iter().map(|p| p.name.as_str()).collect();
        names.sort_unstable();
        for name in names {
            match (a.get(name), b.get(name)) {
                (Some(av), Some(bv)) if av != bv => return av > bv,
                (Some(_), None) => return true,
                (None, Some(_)) => return false,
                _ => {}
            }
        }
        false
    }

    fn visit(&mut self, assigned: &mut BTreeMap<String, Version>) -> Result<(), ResolveError> {
        let node = self.build_node(assigned);
        let packages = self.catalog_map();

        for (name, info) in &node.packages {
            if !packages.contains_key(name.as_str()) {
                return self.record_error(ResolveError::Missing {
                    package: name.clone(),
                    evidence: package_evidence(info),
                });
            }
        }

        for (name, version) in assigned.iter() {
            let Some(info) = node.packages.get(name) else {
                continue;
            };
            let Some(package) = packages.get(name.as_str()).copied() else {
                continue;
            };
            let Some(release) = package.releases.iter().find(|r| r.version == *version) else {
                continue;
            };
            for feature in &info.features {
                if !release.features.iter().any(|declared| declared.name == *feature) {
                    return self.record_error(ResolveError::MissingFeature {
                        package: name.clone(),
                        selected: *version,
                        feature: feature.clone(),
                        path: canonical_path(info),
                    });
                }
            }
        }

        for (name, version) in assigned.iter() {
            if let Some(info) = node.packages.get(name) {
                let constraints = package_constraints(info);
                if !constraints.iter().all(|c| satisfies(*version, *c)) {
                    return self.record_error(ResolveError::Conflict {
                        package: name.clone(),
                        selected: *version,
                        requirements: constraints,
                        evidence: package_evidence(info),
                    });
                }
            }
        }

        for (name, info) in &node.packages {
            if assigned.contains_key(name) {
                continue;
            }
            let package = packages[name.as_str()];
            let constraints = package_constraints(info);
            if !package
                .releases
                .iter()
                .any(|release| release_matches(release, &constraints))
            {
                return self.record_error(ResolveError::NoMatch {
                    package: name.clone(),
                    requirements: constraints,
                    evidence: package_evidence(info),
                });
            }
        }

        let Some(package_name) = node
            .packages
            .keys()
            .find(|name| !assigned.contains_key(*name))
            .cloned()
        else {
            let constraints = node
                .packages
                .iter()
                .map(|(name, info)| (name.clone(), package_constraints(info)))
                .collect();
            let cost = self
                .locks
                .iter()
                .filter(|lock| assigned.get(&lock.package) != Some(&lock.version))
                .count();
            let result = SearchResult {
                selected: assigned.clone(),
                constraints,
            };
            let replace = match &self.best {
                None => true,
                Some(best) => {
                    cost < self.best_cost
                        || (cost == self.best_cost
                            && self.vector_better(&result.selected, &best.selected))
                }
            };
            if replace {
                self.best = Some(result);
                self.best_cost = cost;
            }
            return Ok(());
        };

        let info = &node.packages[&package_name];
        let constraints = package_constraints(info);
        let package = packages[package_name.as_str()];
        let mut candidates: Vec<Version> = package
            .releases
            .iter()
            .filter(|release| release_matches(release, &constraints))
            .map(|release| release.version)
            .collect();
        candidates.sort_by(|a, b| b.cmp(a));

        if let Some(lock) = self.locks.iter().find(|lock| lock.package == package_name) {
            if let Some(index) = candidates.iter().position(|version| *version == lock.version) {
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
            assigned.insert(package_name.clone(), candidate);
            let result = self.visit(assigned);
            assigned.remove(&package_name);

            if let Err(ResolveError::Limit { attempts }) = result {
                return Err(ResolveError::Limit { attempts });
            }
        }

        Ok(())
    }
}

pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
    locks: &[Lock],
    target: Target,
    max_attempts: usize,
) -> Result<Resolution, ResolveError> {
    validate_inputs(catalog, roots, locks)?;

    let mut search = Search {
        catalog,
        roots,
        locks,
        target,
        max_attempts,
        attempts: 0,
        first_error: None,
        best: None,
        best_cost: usize::MAX,
    };

    search.visit(&mut BTreeMap::new())?;

    let Some(best) = search.best else {
        return Err(search.first_error.unwrap_or_else(|| ResolveError::NoMatch {
            package: String::new(),
            requirements: Vec::new(),
            evidence: Vec::new(),
        }));
    };

    let selected: Vec<Selection> = best
        .selected
        .iter()
        .map(|(package, version)| Selection {
            package: package.clone(),
            version: *version,
        })
        .collect();

    let before: BTreeMap<String, Version> = locks
        .iter()
        .map(|lock| (lock.package.clone(), lock.version))
        .collect();
    let mut names: BTreeSet<String> = before.keys().cloned().collect();
    names.extend(best.selected.keys().cloned());

    let mut changes = Vec::new();
    for package in names {
        let old = before.get(&package).copied();
        let new = best.selected.get(&package).copied();
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
                        entry
                            .releases
                            .iter()
                            .any(|release| release.version == old_version)
                    });
                if !exists {
                    ChangeReason::Unavailable
                } else if best
                    .constraints
                    .get(&package)
                    .is_some_and(|constraints| {
                        !constraints
                            .iter()
                            .all(|constraint| satisfies(old_version, *constraint))
                    })
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
            before: old,
            after: new,
            reason,
        });
    }

    Ok(Resolution { selected, changes })
}