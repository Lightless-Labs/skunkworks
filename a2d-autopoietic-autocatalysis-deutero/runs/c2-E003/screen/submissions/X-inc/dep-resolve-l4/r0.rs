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

fn valid_name(s: &str) -> bool {
    let b = s.as_bytes();
    (1..=24).contains(&b.len())
        && b[0].is_ascii_lowercase()
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
        && b[b.len() - 1] != b'-'
}

fn constraint_rank(c: Constraint) -> (u8, Version) {
    match c {
        Constraint::Exact(v) => (0, v),
        Constraint::AtLeast(v) => (1, v),
        Constraint::Before(v) => (2, v),
        Constraint::Caret(v) => (3, v),
    }
}

fn canonical_constraints(cs: &[Constraint]) -> Vec<Constraint> {
    let mut out = cs.to_vec();
    out.sort_by_key(|c| constraint_rank(*c));
    out.dedup();
    out
}

fn constraint_matches(c: Constraint, v: Version) -> bool {
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

fn evidence_text(evidence: &[Evidence]) -> Vec<Evidence> {
    let mut out = evidence.to_vec();
    out.sort_by(|a, b| {
        constraint_rank(a.constraint)
            .cmp(&constraint_rank(b.constraint))
            .then_with(|| a.path.cmp(&b.path))
    });
    out
}

fn path_text(path: &[String]) -> String {
    path.join(" -> ")
}

fn append_evidence(text: &mut String, evidence: &[Evidence]) {
    for e in evidence_text(evidence) {
        text.push_str(&format!(
            "\n  C:{} via {}",
            constraint_text(e.constraint),
            path_text(&e.path)
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
            let mut text = format!("missing|package={}", encode(package));
            append_evidence(&mut text, evidence);
            text
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
            let mut text = format!(
                "no-match|package={}|constraints={}",
                encode(package),
                constraints
            );
            append_evidence(&mut text, evidence);
            text
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
            let mut text = format!(
                "conflict|package={}|selected={}|constraints={}",
                encode(package),
                version_text(*selected),
                constraints
            );
            append_evidence(&mut text, evidence);
            text
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
            path_text(path)
        ),
        ResolveError::Limit { attempts } => format!("limit|attempts={}", attempts),
    }
}

#[derive(Clone)]
struct Origin {
    constraint: Constraint,
    path: Vec<String>,
}

#[derive(Clone)]
struct Closure {
    constraints: BTreeMap<String, Vec<Origin>>,
    activated: BTreeMap<String, BTreeSet<String>>,
    paths: BTreeMap<String, Vec<String>>,
}

struct Solver<'a> {
    catalog: &'a [Package],
    roots: &'a [Requirement],
    locks: &'a [Lock],
    target: Target,
    attempts: usize,
    max_attempts: usize,
    first_error: Option<ResolveError>,
    best: Option<(usize, Vec<Option<Version>>, BTreeMap<String, Version>, Closure)>,
    catalog_names: Vec<String>,
}

impl<'a> Solver<'a> {
    fn package(&self, name: &str) -> Option<&'a Package> {
        self.catalog.iter().find(|p| p.name == name)
    }

    fn release(&self, package: &str, version: Version) -> Option<&'a Release> {
        self.package(package)?
            .releases
            .iter()
            .find(|r| r.version == version)
    }

    fn platform_matches(&self, platform: Platform) -> bool {
        matches!(platform, Platform::Any)
            || matches!(
                (platform, self.target),
                (Platform::Linux, Target::Linux)
                    | (Platform::Mac, Target::Mac)
                    | (Platform::Windows, Target::Windows)
            )
    }

    fn sorted_features(features: &[String]) -> Vec<String> {
        let mut out = features.to_vec();
        out.sort();
        out.dedup();
        out
    }

    fn token_root(&self, i: usize, root: &Requirement) -> String {
        format!(
            "root[{}]:{}@{}+{{{}}}",
            i,
            root.package,
            constraint_text(root.constraint),
            Self::sorted_features(&root.features).join(",")
        )
    }

    fn token_dep(
        &self,
        source: &str,
        version: Version,
        index: usize,
        dep: &Dependency,
    ) -> String {
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
            Self::sorted_features(&dep.features).join(","),
            dep.gate.as_deref().unwrap_or("*"),
            platform
        )
    }

    fn closure(&self, selected: &BTreeMap<String, Version>) -> Closure {
        let mut constraints: BTreeMap<String, Vec<Origin>> = BTreeMap::new();
        let mut requested: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut paths: BTreeMap<String, Vec<String>> = BTreeMap::new();

        for (i, root) in self.roots.iter().enumerate() {
            let token = self.token_root(i, root);
            let path = vec![token];
            constraints
                .entry(root.package.clone())
                .or_default()
                .push(Origin {
                    constraint: root.constraint,
                    path: path.clone(),
                });
            requested
                .entry(root.package.clone())
                .or_default()
                .extend(root.features.iter().cloned());
            let slot = paths.entry(root.package.clone()).or_insert_with(|| path.clone());
            if (path.len(), &path) < (slot.len(), slot) {
                *slot = path;
            }
        }

        let mut activated: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        loop {
            let mut changed = false;

            for (name, version) in selected {
                let Some(release) = self.release(name, *version) else {
                    continue;
                };

                let requests = requested.get(name).cloned().unwrap_or_default();
                let active = activated.entry(name.clone()).or_default();
                let mut queue: Vec<String> = requests.into_iter().collect();

                while let Some(feature) = queue.pop() {
                    if !active.insert(feature.clone()) {
                        continue;
                    }
                    changed = true;
                    if let Some(declaration) =
                        release.features.iter().find(|f| f.name == feature)
                    {
                        for enabled in &declaration.enables {
                            if !active.contains(enabled) {
                                queue.push(enabled.clone());
                            }
                        }
                    }
                }

                for (index, dep) in release.deps.iter().enumerate() {
                    if !self.platform_matches(dep.platform)
                        || dep.gate.as_ref().is_some_and(|gate| !active.contains(gate))
                    {
                        continue;
                    }

                    let Some(source_path) = paths.get(name).cloned() else {
                        continue;
                    };
                    let mut dest_path = source_path;
                    dest_path.push(self.token_dep(name, *version, index, dep));

                    let slot = paths
                        .entry(dep.package.clone())
                        .or_insert_with(|| dest_path.clone());
                    if (dest_path.len(), &dest_path) < (slot.len(), slot) {
                        *slot = dest_path.clone();
                        changed = true;
                    }

                    let origins = constraints.entry(dep.package.clone()).or_default();
                    if !origins
                        .iter()
                        .any(|origin| origin.constraint == dep.constraint && origin.path == dest_path)
                    {
                        origins.push(Origin {
                            constraint: dep.constraint,
                            path: dest_path,
                        });
                        changed = true;
                    }

                    let destination = requested.entry(dep.package.clone()).or_default();
                    for feature in &dep.features {
                        if destination.insert(feature.clone()) {
                            changed = true;
                        }
                    }
                }
            }

            if !changed {
                break;
            }
        }

        Closure {
            constraints,
            activated,
            paths,
        }
    }

    fn active_constraints(closure: &Closure, name: &str) -> Vec<Constraint> {
        canonical_constraints(
            &closure
                .constraints
                .get(name)
                .into_iter()
                .flatten()
                .map(|origin| origin.constraint)
                .collect::<Vec<_>>(),
        )
    }

    fn evidence(closure: &Closure, name: &str) -> Vec<Evidence> {
        let mut evidence = closure
            .constraints
            .get(name)
            .into_iter()
            .flatten()
            .map(|origin| Evidence {
                constraint: origin.constraint,
                path: origin.path.clone(),
            })
            .collect::<Vec<_>>();
        evidence.sort_by(|a, b| {
            constraint_rank(a.constraint)
                .cmp(&constraint_rank(b.constraint))
                .then_with(|| a.path.cmp(&b.path))
        });
        evidence
    }

    fn node_error(
        &self,
        selected: &BTreeMap<String, Version>,
        closure: &Closure,
    ) -> Option<ResolveError> {
        for name in closure.constraints.keys() {
            if self.package(name).is_none() {
                return Some(ResolveError::Missing {
                    package: name.clone(),
                    evidence: Self::evidence(closure, name),
                });
            }
        }

        for (name, version) in selected {
            let features = closure.activated.get(name).cloned().unwrap_or_default();
            if let Some(release) = self.release(name, *version) {
                for feature in features {
                    if !release.features.iter().any(|f| f.name == feature) {
                        return Some(ResolveError::MissingFeature {
                            package: name.clone(),
                            selected: *version,
                            feature,
                            path: closure.paths.get(name).cloned().unwrap_or_default(),
                        });
                    }
                }
            }
        }

        for (name, version) in selected {
            let requirements = Self::active_constraints(closure, name);
            if requirements
                .iter()
                .any(|constraint| !constraint_matches(*constraint, *version))
            {
                return Some(ResolveError::Conflict {
                    package: name.clone(),
                    selected: *version,
                    requirements,
                    evidence: Self::evidence(closure, name),
                });
            }
        }

        for name in closure.constraints.keys() {
            if selected.contains_key(name) {
                continue;
            }
            let requirements = Self::active_constraints(closure, name);
            let matches = self.package(name).is_some_and(|package| {
                package.releases.iter().any(|release| {
                    requirements
                        .iter()
                        .all(|constraint| constraint_matches(*constraint, release.version))
                })
            });
            if !matches {
                return Some(ResolveError::NoMatch {
                    package: name.clone(),
                    requirements,
                    evidence: Self::evidence(closure, name),
                });
            }
        }

        None
    }

    fn candidate_versions(&self, package: &str, requirements: &[Constraint]) -> Vec<Version> {
        let mut versions = self
            .package(package)
            .into_iter()
            .flat_map(|p| p.releases.iter())
            .filter(|release| {
                requirements
                    .iter()
                    .all(|constraint| constraint_matches(*constraint, release.version))
            })
            .map(|release| release.version)
            .collect::<Vec<_>>();
        versions.sort_by(|a, b| b.cmp(a));

        if let Some(lock) = self.locks.iter().find(|lock| lock.package == package) {
            if let Some(index) = versions.iter().position(|version| *version == lock.version) {
                let version = versions.remove(index);
                versions.insert(0, version);
            }
        }
        versions
    }

    fn record_solution(
        &mut self,
        selected: &BTreeMap<String, Version>,
        closure: Closure,
    ) {
        let cost = self
            .locks
            .iter()
            .filter(|lock| selected.get(&lock.package) != Some(&lock.version))
            .count();
        let vector = self
            .catalog_names
            .iter()
            .map(|name| selected.get(name).copied())
            .collect::<Vec<_>>();
        let replace = match &self.best {
            None => true,
            Some((best_cost, best_vector, _, _)) => {
                cost < *best_cost || (cost == *best_cost && vector > *best_vector)
            }
        };
        if replace {
            self.best = Some((cost, vector, selected.clone(), closure));
        }
    }

    fn search(&mut self, selected: &mut BTreeMap<String, Version>) -> Result<(), ResolveError> {
        let closure = self.closure(selected);
        if let Some(error) = self.node_error(selected, &closure) {
            if self.first_error.is_none() {
                self.first_error = Some(error);
            }
            return Ok(());
        }

        let next = closure
            .constraints
            .keys()
            .find(|name| !selected.contains_key(*name))
            .cloned();

        let Some(package) = next else {
            self.record_solution(selected, closure);
            return Ok(());
        };

        let requirements = Self::active_constraints(&closure, &package);
        for version in self.candidate_versions(&package, &requirements) {
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
                for (e, enable) in feature.enables.iter().enumerate() {
                    if !valid_name(enable) {
                        return Err(ResolveError::InvalidName {
                            site: format!(
                                "catalog[{}].release[{}].feature[{}].enable[{}]",
                                i, j, f, e
                            ),
                            name: enable.clone(),
                        });
                    }
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
        if !valid_name(&lock.package) {
            return Err(ResolveError::InvalidName {
                site: format!("lock[{}]", i),
                name: lock.package.clone(),
            });
        }
    }
    let mut lock_names = BTreeSet::new();
    for lock in locks {
        if !lock_names.insert(lock.package.clone()) {
            return Err(ResolveError::DuplicateLock {
                package: lock.package.clone(),
            });
        }
    }

    let mut catalog_names = catalog.iter().map(|p| p.name.clone()).collect::<Vec<_>>();
    catalog_names.sort();

    let mut solver = Solver {
        catalog,
        roots,
        locks,
        target,
        attempts: 0,
        max_attempts,
        first_error: None,
        best: None,
        catalog_names,
    };
    solver.search(&mut BTreeMap::new())?;

    let Some((_, _, chosen, closure)) = solver.best.take() else {
        return Err(solver.first_error.unwrap_or_else(|| ResolveError::NoMatch {
            package: roots
                .first()
                .map(|root| root.package.clone())
                .unwrap_or_default(),
            requirements: Vec::new(),
            evidence: Vec::new(),
        }));
    };

    let selected = chosen
        .iter()
        .map(|(package, version)| Selection {
            package: package.clone(),
            version: *version,
        })
        .collect::<Vec<_>>();

    let mut all_names = BTreeSet::new();
    for lock in locks {
        all_names.insert(lock.package.clone());
    }
    for selection in &selected {
        all_names.insert(selection.package.clone());
    }

    let mut changes = Vec::new();
    for package in all_names {
        let before = locks
            .iter()
            .find(|lock| lock.package == package)
            .map(|lock| lock.version);
        let after = chosen.get(&package).copied();
        if before == after {
            continue;
        }

        let reason = match (before, after) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old), Some(_)) => {
                let exists = solver
                    .package(&package)
                    .is_some_and(|p| p.releases.iter().any(|release| release.version == old));
                if !exists {
                    ChangeReason::Unavailable
                } else {
                    let requirements = Solver::active_constraints(&closure, &package);
                    if requirements
                        .iter()
                        .any(|constraint| !constraint_matches(*constraint, old))
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