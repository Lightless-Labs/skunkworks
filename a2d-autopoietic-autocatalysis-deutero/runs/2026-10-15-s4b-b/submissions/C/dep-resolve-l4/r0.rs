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

use std::collections::{BTreeMap, BTreeSet};

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

fn version_text(version: Version) -> String {
    format!("{}.{}.{}", version.major, version.minor, version.patch)
}

fn constraint_rank(constraint: Constraint) -> (u8, Version) {
    match constraint {
        Constraint::Exact(v) => (0, v),
        Constraint::AtLeast(v) => (1, v),
        Constraint::Before(v) => (2, v),
        Constraint::Caret(v) => (3, v),
    }
}

fn constraint_text(constraint: Constraint) -> String {
    match constraint {
        Constraint::Exact(v) => format!("={}", version_text(v)),
        Constraint::AtLeast(v) => format!(">={}", version_text(v)),
        Constraint::Before(v) => format!("<{}", version_text(v)),
        Constraint::Caret(v) => format!("^{}", version_text(v)),
    }
}

fn canonical_constraints(constraints: &[Constraint]) -> Vec<Constraint> {
    let mut out = constraints.to_vec();
    out.sort_by_key(|c| constraint_rank(*c));
    out.dedup();
    out
}

fn constraints_match(constraints: &[Constraint], version: Version) -> bool {
    constraints.iter().all(|c| matches_constraint(*c, version))
}

fn matches_constraint(constraint: Constraint, version: Version) -> bool {
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

fn encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'.' | b'_' | b'-') {
            out.push(*byte as char);
        } else {
            out.push_str(&format!("%{:02X}", byte));
        }
    }
    out
}

fn sort_evidence(evidence: &[Evidence]) -> Vec<Evidence> {
    let mut out = evidence.to_vec();
    out.sort_by(|a, b| {
        constraint_rank(a.constraint)
            .cmp(&constraint_rank(b.constraint))
            .then_with(|| a.path.cmp(&b.path))
    });
    out
}

fn append_evidence(out: &mut String, evidence: &[Evidence]) {
    for item in sort_evidence(evidence) {
        out.push_str(&format!(
            "\n  C:{} via {}",
            constraint_text(item.constraint),
            item.path.join(" -> ")
        ));
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
            let mut out = format!("missing|package={}", encode(package));
            append_evidence(&mut out, evidence);
            out
        }
        ResolveError::NoMatch {
            package,
            requirements,
            evidence,
        } => {
            let constraints = canonical_constraints(requirements)
                .iter()
                .map(|c| constraint_text(*c))
                .collect::<Vec<_>>()
                .join(",");
            let mut out = format!(
                "no-match|package={}|constraints={}",
                encode(package),
                constraints
            );
            append_evidence(&mut out, evidence);
            out
        }
        ResolveError::Conflict {
            package,
            selected,
            requirements,
            evidence,
        } => {
            let constraints = canonical_constraints(requirements)
                .iter()
                .map(|c| constraint_text(*c))
                .collect::<Vec<_>>()
                .join(",");
            let mut out = format!(
                "conflict|package={}|selected={}|constraints={}",
                encode(package),
                version_text(*selected),
                constraints
            );
            append_evidence(&mut out, evidence);
            out
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

fn platform_matches(platform: Platform, target: Target) -> bool {
    matches!(
        (platform, target),
        (Platform::Any, _)
            | (Platform::Linux, Target::Linux)
            | (Platform::Mac, Target::Mac)
            | (Platform::Windows, Target::Windows)
    )
}

#[derive(Clone, Debug)]
struct Origin {
    constraint: Constraint,
    path: Vec<String>,
}

#[derive(Clone, Debug, Default)]
struct Active {
    origins: Vec<Origin>,
    features: BTreeSet<String>,
    feature_paths: BTreeMap<String, Vec<String>>,
    incoming_path: Option<Vec<String>>,
}

fn root_token(requirement: &Requirement, index: usize) -> String {
    let features = requirement
        .features
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "root[{}]:{}@{}+{{{}}}",
        index,
        requirement.package,
        constraint_text(requirement.constraint),
        features
    )
}

fn dependency_token(
    source: &str,
    version: Version,
    index: usize,
    dependency: &Dependency,
) -> String {
    let features = dependency
        .features
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join(",");
    let gate = dependency.gate.as_deref().unwrap_or("*");
    let platform = match dependency.platform {
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
        dependency.package,
        constraint_text(dependency.constraint),
        features,
        gate,
        platform
    )
}

fn validate(
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
        for (j, feature) in root.features.iter().enumerate() {
            if !valid_name(feature) {
                return Err(ResolveError::InvalidName {
                    site: format!("root[{}].feature[{}]", i, j),
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
            for (k, dependency) in release.deps.iter().enumerate() {
                let site = format!("catalog[{}].release[{}].dep[{}]", i, j, k);
                if !valid_name(&dependency.package) {
                    return Err(ResolveError::InvalidName {
                        site,
                        name: dependency.package.clone(),
                    });
                }
                if let Some(gate) = &dependency.gate {
                    if !valid_name(gate) {
                        return Err(ResolveError::InvalidName {
                            site: format!(
                                "catalog[{}].release[{}].dep[{}].gate",
                                i, j, k
                            ),
                            name: gate.clone(),
                        });
                    }
                }
                for (f, feature) in dependency.features.iter().enumerate() {
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
                        site: format!(
                            "catalog[{}].release[{}].feature[{}]",
                            i, j, f
                        ),
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

    let mut package_names = BTreeSet::new();
    for package in catalog {
        if !package_names.insert(package.name.as_str()) {
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

#[derive(Clone, Debug)]
struct Candidate {
    selected: BTreeMap<String, Version>,
    active: BTreeMap<String, Active>,
}

struct Search<'a> {
    catalog: &'a [Package],
    roots: &'a [Requirement],
    locks: &'a [Lock],
    target: Target,
    max_attempts: usize,
    attempts: usize,
    catalog_index: BTreeMap<String, usize>,
    best: Option<(usize, Vec<Option<Version>>, Candidate)>,
    first_error: Option<ResolveError>,
}

impl<'a> Search<'a> {
    fn closure(
        &self,
        selected: &BTreeMap<String, Version>,
    ) -> (BTreeMap<String, Active>, Option<ResolveError>) {
        let mut active = BTreeMap::<String, Active>::new();

        for (i, root) in self.roots.iter().enumerate() {
            let token = root_token(root, i);
            let path = vec![token.clone()];
            let entry = active.entry(root.package.clone()).or_default();
            entry.origins.push(Origin {
                constraint: root.constraint,
                path: path.clone(),
            });
            if entry
                .incoming_path
                .as_ref()
                .map_or(true, |old| path < *old)
            {
                entry.incoming_path = Some(path.clone());
            }

            for feature in &root.features {
                let feature_path = path.clone();
                let item = active.entry(root.package.clone()).or_default();
                item.features.insert(feature.clone());
                if item
                    .feature_paths
                    .get(feature)
                    .map_or(true, |old| feature_path < *old)
                {
                    item.feature_paths
                        .insert(feature.clone(), feature_path);
                }
            }
        }

        loop {
            let mut changed = false;
            let package_names = active.keys().cloned().collect::<Vec<_>>();
            for package_name in package_names {
                let Some(version) = selected.get(&package_name).copied() else {
                    continue;
                };
                let Some(catalog_index) = self.catalog_index.get(&package_name).copied() else {
                    continue;
                };
                let Some(release) = self.catalog[catalog_index]
                    .releases
                    .iter()
                    .find(|release| release.version == version)
                else {
                    continue;
                };

                let requests = active[&package_name]
                    .features
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>();
                let mut activated = BTreeSet::new();
                let mut feature_queue = requests
                    .iter()
                    .filter_map(|feature| {
                        active[&package_name]
                            .feature_paths
                            .get(feature)
                            .cloned()
                            .map(|path| (feature.clone(), path))
                    })
                    .collect::<Vec<_>>();

                while let Some((feature_name, path)) = feature_queue.pop() {
                    if !activated.insert(feature_name.clone()) {
                        continue;
                    }
                    let Some(declaration) = release
                        .features
                        .iter()
                        .find(|declaration| declaration.name == feature_name)
                    else {
                        continue;
                    };

                    for enabled in &declaration.enables {
                        let mut enabled_path = path.clone();
                        enabled_path.push(format!("feature:{}=>{}", feature_name, enabled));
                        let item = active.entry(package_name.clone()).or_default();
                        item.features.insert(enabled.clone());
                        if item
                            .feature_paths
                            .get(enabled)
                            .map_or(true, |old| enabled_path < *old)
                        {
                            item.feature_paths
                                .insert(enabled.clone(), enabled_path.clone());
                            feature_queue.push((enabled.clone(), enabled_path));
                            changed = true;
                        }
                    }
                }

                let source_path = active[&package_name]
                    .incoming_path
                    .clone()
                    .unwrap_or_default();

                for (dep_index, dependency) in release.deps.iter().enumerate() {
                    let source_features = &active[&package_name].features;
                    if !platform_matches(dependency.platform, self.target)
                        || dependency
                            .gate
                            .as_ref()
                            .is_some_and(|gate| !source_features.contains(gate))
                    {
                        continue;
                    }

                    let mut path = source_path.clone();
                    path.push(dependency_token(
                        &package_name,
                        version,
                        dep_index,
                        dependency,
                    ));

                    let destination = active.entry(dependency.package.clone()).or_default();
                    if !destination.origins.iter().any(|origin| {
                        origin.constraint == dependency.constraint && origin.path == path
                    }) {
                        destination.origins.push(Origin {
                            constraint: dependency.constraint,
                            path: path.clone(),
                        });
                        changed = true;
                    }
                    if destination
                        .incoming_path
                        .as_ref()
                        .map_or(true, |old| {
                            path.len() < old.len() || (path.len() == old.len() && path < *old)
                        })
                    {
                        destination.incoming_path = Some(path.clone());
                        changed = true;
                    }

                    for feature in &dependency.features {
                        destination.features.insert(feature.clone());
                        if destination
                            .feature_paths
                            .get(feature)
                            .map_or(true, |old| {
                                path.len() < old.len() || (path.len() == old.len() && path < *old)
                            })
                        {
                            destination
                                .feature_paths
                                .insert(feature.clone(), path.clone());
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }

        for item in active.values_mut() {
            item.origins.sort_by(|a, b| {
                constraint_rank(a.constraint)
                    .cmp(&constraint_rank(b.constraint))
                    .then_with(|| a.path.cmp(&b.path))
            });
        }

        for (package_name, version) in selected {
            let Some(item) = active.get(package_name) else {
                continue;
            };
            let Some(index) = self.catalog_index.get(package_name).copied() else {
                continue;
            };
            let Some(release) = self.catalog[index]
                .releases
                .iter()
                .find(|release| release.version == *version)
            else {
                continue;
            };
            let declared = release
                .features
                .iter()
                .map(|feature| feature.name.as_str())
                .collect::<BTreeSet<_>>();
            if let Some(feature) = item
                .features
                .iter()
                .find(|feature| !declared.contains(feature.as_str()))
            {
                let path = item.incoming_path.clone().unwrap_or_default();
                return (
                    active,
                    Some(ResolveError::MissingFeature {
                        package: package_name.clone(),
                        selected: *version,
                        feature: feature.clone(),
                        path,
                    }),
                );
            }
        }
        (active, None)
    }

    fn node_error(
        &self,
        selected: &BTreeMap<String, Version>,
    ) -> Result<(BTreeMap<String, Active>, Option<String>), ResolveError> {
        let (active, feature_error) = self.closure(selected);

        if let Some(package) = active
            .keys()
            .find(|package| !self.catalog_index.contains_key(*package))
        {
            return Err(ResolveError::Missing {
                package: package.clone(),
                evidence: active[package]
                    .origins
                    .iter()
                    .map(|origin| Evidence {
                        constraint: origin.constraint,
                        path: origin.path.clone(),
                    })
                    .collect(),
            });
        }

        if let Some(error) = feature_error {
            return Err(error);
        }

        for (package, version) in selected {
            if let Some(item) = active.get(package) {
                let requirements = canonical_constraints(
                    &item.origins.iter().map(|origin| origin.constraint).collect::<Vec<_>>(),
                );
                if !constraints_match(&requirements, *version) {
                    return Err(ResolveError::Conflict {
                        package: package.clone(),
                        selected: *version,
                        requirements,
                        evidence: item
                            .origins
                            .iter()
                            .map(|origin| Evidence {
                                constraint: origin.constraint,
                                path: origin.path.clone(),
                            })
                            .collect(),
                    });
                }
            }
        }

        let mut branch = None;
        for (package, item) in &active {
            if selected.contains_key(package) {
                continue;
            }
            let requirements = canonical_constraints(
                &item.origins.iter().map(|origin| origin.constraint).collect::<Vec<_>>(),
            );
            let index = self.catalog_index[package];
            if !self.catalog[index].releases.iter().any(|release| {
                constraints_match(&requirements, release.version)
            }) {
                return Err(ResolveError::NoMatch {
                    package: package.clone(),
                    requirements,
                    evidence: item
                        .origins
                        .iter()
                        .map(|origin| Evidence {
                            constraint: origin.constraint,
                            path: origin.path.clone(),
                        })
                        .collect(),
                });
            }
            if branch.is_none() {
                branch = Some(package.clone());
            }
        }
        Ok((active, branch))
    }

    fn lock_cost(&self, selected: &BTreeMap<String, Version>) -> usize {
        self.locks
            .iter()
            .filter(|lock| selected.get(&lock.package).copied() != Some(lock.version))
            .count()
    }

    fn consider(
        &mut self,
        selected: &BTreeMap<String, Version>,
        active: BTreeMap<String, Active>,
    ) {
        let cost = self.lock_cost(selected);
        let mut names = self.catalog_index.keys().cloned().collect::<Vec<_>>();
        names.sort();
        let vector = names
            .iter()
            .map(|name| selected.get(name).copied())
            .collect::<Vec<_>>();
        let replace = self.best.as_ref().map_or(true, |(best_cost, best_vector, _)| {
            cost < *best_cost || (cost == *best_cost && vector > *best_vector)
        });
        if replace {
            self.best = Some((
                cost,
                vector,
                Candidate {
                    selected: selected.clone(),
                    active,
                },
            ));
        }
    }

    fn search(&mut self, selected: &mut BTreeMap<String, Version>) -> Result<(), ResolveError> {
        let (active, branch) = match self.node_error(selected) {
            Ok(result) => result,
            Err(error) => {
                if self.first_error.is_none() {
                    self.first_error = Some(error);
                }
                return Ok(());
            }
        };

        let Some(package) = branch else {
            self.consider(selected, active);
            return Ok(());
        };

        let item = &active[&package];
        let requirements = canonical_constraints(
            &item.origins.iter().map(|origin| origin.constraint).collect::<Vec<_>>(),
        );
        let index = self.catalog_index[&package];
        let mut candidates = self.catalog[index]
            .releases
            .iter()
            .filter(|release| constraints_match(&requirements, release.version))
            .map(|release| release.version)
            .collect::<Vec<_>>();
        candidates.sort_by(|a, b| b.cmp(a));

        if let Some(lock) = self.locks.iter().find(|lock| lock.package == package) {
            if let Some(position) = candidates.iter().position(|version| *version == lock.version) {
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
            let result = self.search(selected);
            selected.remove(&package);
            result?;
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
    validate(catalog, roots, locks)?;

    let catalog_index = catalog
        .iter()
        .enumerate()
        .map(|(i, package)| (package.name.clone(), i))
        .collect::<BTreeMap<_, _>>();

    let mut search = Search {
        catalog,
        roots,
        locks,
        target,
        max_attempts,
        attempts: 0,
        catalog_index,
        best: None,
        first_error: None,
    };
    search.search(&mut BTreeMap::new())?;

    let Some((_, _, solution)) = search.best else {
        return Err(search.first_error.unwrap_or(ResolveError::Missing {
            package: String::new(),
            evidence: Vec::new(),
        }));
    };

    let selected = solution
        .selected
        .iter()
        .map(|(package, version)| Selection {
            package: package.clone(),
            version: *version,
        })
        .collect::<Vec<_>>();

    let before = locks
        .iter()
        .map(|lock| (lock.package.clone(), lock.version))
        .collect::<BTreeMap<_, _>>();
    let changed_names = before
        .keys()
        .chain(solution.selected.keys())
        .cloned()
        .collect::<BTreeSet<_>>();

    let mut changes = Vec::new();
    for package in changed_names {
        let old = before.get(&package).copied();
        let new = solution.selected.get(&package).copied();
        if old == new {
            continue;
        }

        let reason = match (old, new) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old_version), Some(new_version)) => {
                let exists = search
                    .catalog_index
                    .get(&package)
                    .is_some_and(|index| {
                        catalog[*index]
                            .releases
                            .iter()
                            .any(|release| release.version == old_version)
                    });
                if !exists {
                    ChangeReason::Unavailable
                } else if solution.active.get(&package).is_some_and(|item| {
                    item.origins
                        .iter()
                        .any(|origin| !matches_constraint(origin.constraint, new_version))
                }) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn v(major: u32, minor: u32, patch: u32) -> Version {
        Version {
            major,
            minor,
            patch,
        }
    }

    fn x(n: u32) -> Version {
        v(n, 0, 0)
    }

    fn e(n: u32) -> Constraint {
        Constraint::Exact(x(n))
    }

    fn ge(n: u32) -> Constraint {
        Constraint::AtLeast(x(n))
    }

    fn q(package: &str, constraint: Constraint) -> Requirement {
        Requirement {
            package: package.into(),
            constraint,
            features: vec![],
        }
    }

    fn d(package: &str, constraint: Constraint) -> Dependency {
        Dependency {
            package: package.into(),
            constraint,
            features: vec![],
            gate: None,
            platform: Platform::Any,
        }
    }

    fn r(n: u32, deps: Vec<Dependency>) -> Release {
        Release {
            version: x(n),
            deps,
            features: vec![],
        }
    }

    fn feat(name: &str, enables: &[&str]) -> Feature {
        Feature {
            name: name.into(),
            enables: enables.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn ask(package: &str, constraint: Constraint, features: &[&str]) -> Requirement {
        let mut requirement = q(package, constraint);
        requirement.features = features.iter().map(|s| s.to_string()).collect();
        requirement
    }

    fn p(name: &str, releases: Vec<Release>) -> Package {
        Package {
            name: name.into(),
            releases,
        }
    }

    fn sel(package: &str, n: u32) -> Selection {
        Selection {
            package: package.into(),
            version: x(n),
        }
    }

    fn change(
        package: &str,
        before: Option<u32>,
        after: Option<u32>,
        reason: ChangeReason,
    ) -> Change {
        Change {
            package: package.into(),
            before: before.map(x),
            after: after.map(x),
            reason,
        }
    }

    #[test]
    fn transitive_selection() {
        let catalog = vec![
            p("engine", vec![r(1, vec![d("util", ge(1))])]),
            p("util", vec![r(1, vec![])]),
        ];
        let resolution =
            resolve(&catalog, &[q("engine", e(1))], &[], Target::Linux, 100).unwrap();
        assert_eq!(
            resolution.selected,
            vec![sel("engine", 1), sel("util", 1)]
        );
    }

    #[test]
    fn constraints_filter_versions() {
        let catalog = vec![p(
            "codec",
            vec![r(3, vec![]), r(1, vec![]), r(2, vec![])],
        )];
        let roots = [q("codec", ge(1)), q("codec", Constraint::Before(x(3)))];
        let resolution = resolve(&catalog, &roots, &[], Target::Linux, 100).unwrap();
        assert_eq!(resolution.selected, vec![sel("codec", 2)]);
    }

    #[test]
    fn invalid_name_format() {
        let error = ResolveError::InvalidName {
            site: "root[0]".into(),
            name: "Bad Name".into(),
        };
        assert_eq!(
            error_text(&error),
            "invalid-name|site=root%5B0%5D|name=Bad%20Name"
        );
    }

    #[test]
    fn feature_gate_activates_dependency() {
        let mut release = r(1, vec![]);
        release.features.push(feat("extras", &[]));
        let mut dependency = d("plugin", e(1));
        dependency.gate = Some("extras".into());
        release.deps.push(dependency);
        let catalog = vec![
            p("app", vec![release]),
            p("plugin", vec![r(1, vec![])]),
        ];
        let resolution = resolve(
            &catalog,
            &[ask("app", e(1), &["extras"])],
            &[],
            Target::Linux,
            10,
        )
        .unwrap();
        assert_eq!(
            resolution.selected,
            vec![sel("app", 1), sel("plugin", 1)]
        );
        assert_eq!(
            resolution.changes,
            vec![
                change("app", None, Some(1), ChangeReason::Added),
                change("plugin", None, Some(1), ChangeReason::Added),
            ]
        );
    }

    #[test]
    fn empty_roots_remove_locks_without_attempts() {
        let resolution = resolve(
            &[],
            &[],
            &[Lock {
                package: "unused".into(),
                version: x(4),
            }],
            Target::Linux,
            0,
        )
        .unwrap();
        assert_eq!(
            resolution.changes,
            vec![change("unused", Some(4), None, ChangeReason::Removed)]
        );
    }
}

fn main() {}