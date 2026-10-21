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
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
        && bytes[bytes.len() - 1] != b'-'
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

fn constraint_order(a: Constraint, b: Constraint) -> std::cmp::Ordering {
    fn tag(c: Constraint) -> u8 {
        match c {
            Constraint::Exact(_) => 0,
            Constraint::AtLeast(_) => 1,
            Constraint::Before(_) => 2,
            Constraint::Caret(_) => 3,
        }
    }

    tag(a).cmp(&tag(b)).then_with(|| match (a, b) {
        (Constraint::Exact(x), Constraint::Exact(y))
        | (Constraint::AtLeast(x), Constraint::AtLeast(y))
        | (Constraint::Before(x), Constraint::Before(y))
        | (Constraint::Caret(x), Constraint::Caret(y)) => x.cmp(&y),
        _ => std::cmp::Ordering::Equal,
    })
}

fn canonical_constraints(values: &[Constraint]) -> Vec<Constraint> {
    let mut out = values.to_vec();
    out.sort_by(|a, b| constraint_order(*a, *b));
    out.dedup();
    out
}

fn constraint_list_text(values: &[Constraint]) -> String {
    canonical_constraints(values)
        .iter()
        .map(|c| constraint_text(*c))
        .collect::<Vec<_>>()
        .join(",")
}

fn accepts(c: Constraint, v: Version) -> bool {
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

fn encode(s: &str) -> String {
    let mut out = String::new();
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

fn sorted_evidence(evidence: &[Evidence]) -> Vec<Evidence> {
    let mut out = evidence.to_vec();
    out.sort_by(|a, b| {
        constraint_order(a.constraint, b.constraint).then_with(|| a.path.cmp(&b.path))
    });
    out
}

fn evidence_lines(evidence: &[Evidence]) -> String {
    let mut out = String::new();
    for ev in sorted_evidence(evidence) {
        out.push_str("\n  C:");
        out.push_str(&constraint_text(ev.constraint));
        out.push_str(" via ");
        out.push_str(&ev.path.join(" -> "));
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
        ResolveError::Missing { package, evidence } => format!(
            "missing|package={}{}",
            encode(package),
            evidence_lines(evidence)
        ),
        ResolveError::NoMatch {
            package,
            requirements,
            evidence,
        } => format!(
            "no-match|package={}|constraints={}{}",
            encode(package),
            constraint_list_text(requirements),
            evidence_lines(evidence)
        ),
        ResolveError::Conflict {
            package,
            selected,
            requirements,
            evidence,
        } => format!(
            "conflict|package={}|selected={}|constraints={}{}",
            encode(package),
            version_text(*selected),
            constraint_list_text(requirements),
            evidence_lines(evidence)
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

#[derive(Clone)]
struct Origin {
    constraint: Constraint,
    path: Vec<String>,
}

#[derive(Clone, Default)]
struct Active {
    origins: Vec<Origin>,
    features: BTreeSet<String>,
}

struct Solver<'a> {
    catalog: &'a [Package],
    package_indices: BTreeMap<String, usize>,
    roots: &'a [Requirement],
    locks: BTreeMap<String, Version>,
    target: Target,
    budget: usize,
    attempts: usize,
    best: Option<(usize, BTreeMap<String, Version>)>,
    first_error: Option<ResolveError>,
}

impl<'a> Solver<'a> {
    fn release(&self, package: &str, version: Version) -> Option<&'a Release> {
        let idx = *self.package_indices.get(package)?;
        self.catalog[idx]
            .releases
            .iter()
            .find(|r| r.version == version)
    }

    fn target_matches(&self, platform: Platform) -> bool {
        matches!(platform, Platform::Any)
            || matches!(
                (platform, self.target),
                (Platform::Linux, Target::Linux)
                    | (Platform::Mac, Target::Mac)
                    | (Platform::Windows, Target::Windows)
            )
    }

    fn build_active(
        &self,
        selected: &BTreeMap<String, Version>,
    ) -> (BTreeMap<String, Active>, BTreeMap<String, Vec<String>>) {
        let mut active: BTreeMap<String, Active> = BTreeMap::new();
        for (i, root) in self.roots.iter().enumerate() {
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
            let entry = active.entry(root.package.clone()).or_default();
            entry.origins.push(Origin {
                constraint: root.constraint,
                path: vec![token],
            });
            entry.features.extend(root.features.iter().cloned());
        }

        // Reachability and feature closure are monotone for a fixed assignment.
        // Track each dependency edge once, then derive provenance from final paths.
        let mut known_edges = BTreeSet::<(String, Version, usize)>::new();
        loop {
            let mut changed = false;
            let names: Vec<String> = active.keys().cloned().collect();

            for package in names {
                let Some(&version) = selected.get(&package) else {
                    continue;
                };
                let Some(release) = self.release(&package, version) else {
                    continue;
                };

                let requested: Vec<String> = active[&package].features.iter().cloned().collect();
                for feature_name in requested {
                    if let Some(feature) = release.features.iter().find(|f| f.name == feature_name) {
                        for enabled in &feature.enables {
                            if active
                                .get_mut(&package)
                                .unwrap()
                                .features
                                .insert(enabled.clone())
                            {
                                changed = true;
                            }
                        }
                    }
                }

                let activated = active[&package].features.clone();
                for (j, dep) in release.deps.iter().enumerate() {
                    if !self.target_matches(dep.platform)
                        || dep.gate.as_ref().is_some_and(|g| !activated.contains(g))
                    {
                        continue;
                    }
                    if !known_edges.insert((package.clone(), version, j)) {
                        continue;
                    }

                    let mut features = dep.features.clone();
                    features.sort();
                    features.dedup();
                    let token = format!(
                        "{}@{}/deps[{}]:{}@{}+{{{}}};gate={};platform={}",
                        package,
                        version_text(version),
                        j,
                        dep.package,
                        constraint_text(dep.constraint),
                        features.join(","),
                        dep.gate.as_deref().unwrap_or("*"),
                        match dep.platform {
                            Platform::Any => "any",
                            Platform::Linux => "linux",
                            Platform::Mac => "mac",
                            Platform::Windows => "windows",
                        }
                    );

                    let entry = active.entry(dep.package.clone()).or_default();
                    entry.origins.push(Origin {
                        constraint: dep.constraint,
                        path: vec![token],
                    });
                    let before = entry.features.len();
                    entry.features.extend(dep.features.iter().cloned());
                    changed = true;
                    if entry.features.len() != before {
                        changed = true;
                    }
                }
            }

            if !changed {
                break;
            }
        }

        // Compute canonical incoming paths over the completed active dependency graph.
        let mut paths: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (name, state) in &active {
            for origin in &state.origins {
                if origin.path.len() == 1 && origin.path[0].starts_with("root[") {
                    let candidate = origin.path.clone();
                    let replace = paths.get(name).is_none_or(|old| {
                        candidate.len() < old.len()
                            || (candidate.len() == old.len() && candidate < *old)
                    });
                    if replace {
                        paths.insert(name.clone(), candidate);
                    }
                }
            }
        }

        loop {
            let mut changed = false;
            for (source, version) in selected {
                if !active.contains_key(source) {
                    continue;
                }
                let Some(source_path) = paths.get(source).cloned() else {
                    continue;
                };
                let Some(release) = self.release(source, *version) else {
                    continue;
                };
                let activated = &active[source].features;
                for (j, dep) in release.deps.iter().enumerate() {
                    if !self.target_matches(dep.platform)
                        || dep.gate.as_ref().is_some_and(|g| !activated.contains(g))
                    {
                        continue;
                    }
                    if !active.contains_key(&dep.package) {
                        continue;
                    }
                    let mut features = dep.features.clone();
                    features.sort();
                    features.dedup();
                    let token = format!(
                        "{}@{}/deps[{}]:{}@{}+{{{}}};gate={};platform={}",
                        source,
                        version_text(*version),
                        j,
                        dep.package,
                        constraint_text(dep.constraint),
                        features.join(","),
                        dep.gate.as_deref().unwrap_or("*"),
                        match dep.platform {
                            Platform::Any => "any",
                            Platform::Linux => "linux",
                            Platform::Mac => "mac",
                            Platform::Windows => "windows",
                        }
                    );
                    let mut candidate = source_path.clone();
                    candidate.push(token);
                    let replace = paths.get(&dep.package).is_none_or(|old| {
                        candidate.len() < old.len()
                            || (candidate.len() == old.len() && candidate < *old)
                    });
                    if replace {
                        paths.insert(dep.package.clone(), candidate);
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }

        // Replace dependency origin paths with canonical source paths.
        for (source, version) in selected {
            if !active.contains_key(source) {
                continue;
            }
            let Some(source_path) = paths.get(source).cloned() else {
                continue;
            };
            let Some(release) = self.release(source, *version) else {
                continue;
            };
            let activated = &active[source].features;
            for (j, dep) in release.deps.iter().enumerate() {
                if !self.target_matches(dep.platform)
                    || dep.gate.as_ref().is_some_and(|g| !activated.contains(g))
                {
                    continue;
                }
                let mut features = dep.features.clone();
                features.sort();
                features.dedup();
                let token = format!(
                    "{}@{}/deps[{}]:{}@{}+{{{}}};gate={};platform={}",
                    source,
                    version_text(*version),
                    j,
                    dep.package,
                    constraint_text(dep.constraint),
                    features.join(","),
                    dep.gate.as_deref().unwrap_or("*"),
                    match dep.platform {
                        Platform::Any => "any",
                        Platform::Linux => "linux",
                        Platform::Mac => "mac",
                        Platform::Windows => "windows",
                    }
                );
                if let Some(state) = active.get_mut(&dep.package) {
                    if let Some(origin) = state.origins.iter_mut().find(|o| {
                        o.constraint == dep.constraint && o.path == vec![token.clone()]
                    }) {
                        let mut path = source_path.clone();
                        path.push(token);
                        origin.path = path;
                    }
                }
            }
        }

        (active, paths)
    }

    fn canonical_origins(active: &Active) -> Vec<Constraint> {
        canonical_constraints(
            &active
                .origins
                .iter()
                .map(|o| o.constraint)
                .collect::<Vec<_>>(),
        )
    }

    fn evidence(active: &Active) -> Vec<Evidence> {
        let mut result = active
            .origins
            .iter()
            .map(|o| Evidence {
                constraint: o.constraint,
                path: o.path.clone(),
            })
            .collect::<Vec<_>>();
        result.sort_by(|a, b| {
            constraint_order(a.constraint, b.constraint).then_with(|| a.path.cmp(&b.path))
        });
        result
    }

    fn candidates(&self, name: &str, state: &Active) -> Vec<Version> {
        let idx = self.package_indices[name];
        let mut candidates: Vec<Version> = self.catalog[idx]
            .releases
            .iter()
            .map(|r| r.version)
            .filter(|v| state.origins.iter().all(|o| accepts(o.constraint, *v)))
            .collect();
        candidates.sort_by(|a, b| b.cmp(a));
        candidates.dedup();
        if let Some(lock_version) = self.locks.get(name) {
            if let Some(pos) = candidates.iter().position(|v| v == lock_version) {
                let locked = candidates.remove(pos);
                candidates.insert(0, locked);
            }
        }
        candidates
    }

    fn inspect(
        &mut self,
        selected: &BTreeMap<String, Version>,
    ) -> Result<Option<(String, Vec<Version>)>, ResolveError> {
        let (active, paths) = self.build_active(selected);

        if let Some(name) = active
            .keys()
            .find(|name| !self.package_indices.contains_key(*name))
        {
            return Err(ResolveError::Missing {
                package: name.clone(),
                evidence: Self::evidence(&active[name]),
            });
        }

        // MissingFeature precedes conflicts and candidate availability checks.
        for (name, version) in selected {
            if let Some(state) = active.get(name) {
                let Some(release) = self.release(name, *version) else {
                    continue;
                };
                for feature in &state.features {
                    if !release.features.iter().any(|f| &f.name == feature) {
                        return Err(ResolveError::MissingFeature {
                            package: name.clone(),
                            selected: *version,
                            feature: feature.clone(),
                            path: paths.get(name).cloned().unwrap_or_default(),
                        });
                    }
                }
            }
        }

        for (name, version) in selected {
            if let Some(state) = active.get(name) {
                if !state
                    .origins
                    .iter()
                    .all(|o| accepts(o.constraint, *version))
                {
                    return Err(ResolveError::Conflict {
                        package: name.clone(),
                        selected: *version,
                        requirements: Self::canonical_origins(state),
                        evidence: Self::evidence(state),
                    });
                }
            }
        }

        for (name, state) in &active {
            if selected.contains_key(name) {
                continue;
            }
            if self.candidates(name, state).is_empty() {
                return Err(ResolveError::NoMatch {
                    package: name.clone(),
                    requirements: Self::canonical_origins(state),
                    evidence: Self::evidence(state),
                });
            }
        }

        let Some(name) = active.keys().find(|name| !selected.contains_key(*name)) else {
            return Ok(None);
        };
        Ok(Some((name.clone(), self.candidates(name, &active[name]))))
    }

    fn lock_cost(&self, selected: &BTreeMap<String, Version>) -> usize {
        self.locks
            .iter()
            .filter(|(package, version)| selected.get(*package) != Some(*version))
            .count()
    }

    fn better(
        &self,
        cost: usize,
        candidate: &BTreeMap<String, Version>,
        current: &(usize, BTreeMap<String, Version>),
    ) -> bool {
        if cost != current.0 {
            return cost < current.0;
        }
        for name in self.package_indices.keys() {
            match (candidate.get(name), current.1.get(name)) {
                (Some(x), Some(y)) if x != y => return x > y,
                (Some(_), None) => return true,
                (None, Some(_)) => return false,
                _ => {}
            }
        }
        false
    }

    fn search(&mut self, selected: &mut BTreeMap<String, Version>) -> Result<(), ResolveError> {
        match self.inspect(selected) {
            Err(error) => {
                if matches!(error, ResolveError::Limit { .. }) {
                    return Err(error);
                }
                if self.first_error.is_none() {
                    self.first_error = Some(error);
                }
                Ok(())
            }
            Ok(None) => {
                let cost = self.lock_cost(selected);
                if self
                    .best
                    .as_ref()
                    .is_none_or(|current| self.better(cost, selected, current))
                {
                    self.best = Some((cost, selected.clone()));
                }
                Ok(())
            }
            Ok(Some((package, candidates))) => {
                for version in candidates {
                    if self.attempts == self.budget {
                        return Err(ResolveError::Limit {
                            attempts: self.budget,
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
                let base = format!("catalog[{}].release[{}].dep[{}]", i, j, k);
                if !valid_name(&dep.package) {
                    return Err(ResolveError::InvalidName {
                        site: base.clone(),
                        name: dep.package.clone(),
                    });
                }
                if let Some(gate) = &dep.gate {
                    if !valid_name(gate) {
                        return Err(ResolveError::InvalidName {
                            site: format!("{}.gate", base),
                            name: gate.clone(),
                        });
                    }
                }
                for (f, feature) in dep.features.iter().enumerate() {
                    if !valid_name(feature) {
                        return Err(ResolveError::InvalidName {
                            site: format!("{}.feature[{}]", base, f),
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
            let mut seen = BTreeSet::new();
            for feature in &release.features {
                if !seen.insert(feature.name.clone()) {
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

    let mut seen_locks = BTreeSet::new();
    let mut lock_map = BTreeMap::new();
    for lock in locks {
        if !seen_locks.insert(lock.package.clone()) {
            return Err(ResolveError::DuplicateLock {
                package: lock.package.clone(),
            });
        }
        lock_map.insert(lock.package.clone(), lock.version);
    }

    let package_indices = catalog
        .iter()
        .enumerate()
        .map(|(i, p)| (p.name.clone(), i))
        .collect::<BTreeMap<_, _>>();

    let mut solver = Solver {
        catalog,
        package_indices,
        roots,
        locks: lock_map.clone(),
        target,
        budget: max_attempts,
        attempts: 0,
        best: None,
        first_error: None,
    };
    let mut selected = BTreeMap::new();
    solver.search(&mut selected)?;

    let chosen = match solver.best.take() {
        Some((_, chosen)) => chosen,
        None => {
            return Err(solver.first_error.take().unwrap_or_else(|| ResolveError::Missing {
                package: String::new(),
                evidence: Vec::new(),
            }));
        }
    };

    let selected_out = chosen
        .iter()
        .map(|(package, version)| Selection {
            package: package.clone(),
            version: *version,
        })
        .collect::<Vec<_>>();

    let (final_active, _) = solver.build_active(&chosen);
    let mut names = BTreeSet::new();
    names.extend(lock_map.keys().cloned());
    names.extend(chosen.keys().cloned());
    let mut changes = Vec::new();

    for package in names {
        let before = lock_map.get(&package).copied();
        let after = chosen.get(&package).copied();
        if before == after {
            continue;
        }
        let reason = match (before, after) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old), Some(_)) => {
                let available = solver
                    .package_indices
                    .get(&package)
                    .is_some_and(|idx| {
                        catalog[*idx]
                            .releases
                            .iter()
                            .any(|r| r.version == old)
                    });
                if !available {
                    ChangeReason::Unavailable
                } else {
                    let rejected = final_active.get(&package).is_some_and(|active| {
                        !active
                            .origins
                            .iter()
                            .all(|origin| accepts(origin.constraint, old))
                    });
                    if rejected {
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

    Ok(Resolution {
        selected: selected_out,
        changes,
    })
}