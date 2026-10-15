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
    !b.is_empty()
        && b.len() <= 24
        && b[0].is_ascii_lowercase()
        && b[b.len() - 1] != b'-'
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

fn enc(s: &str) -> String {
    let mut out = String::new();
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-') {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 15) as usize] as char);
        }
    }
    out
}

fn version_text(v: Version) -> String {
    format!("{}.{}.{}", v.major, v.minor, v.patch)
}

fn constraint_key(c: Constraint) -> (u8, Version) {
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

fn canonical_constraints(cs: &[Constraint]) -> Vec<Constraint> {
    let mut out = cs.to_vec();
    out.sort_by_key(|c| constraint_key(*c));
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

fn matches_all(cs: &[Constraint], v: Version) -> bool {
    cs.iter().all(|c| constraint_matches(*c, v))
}

fn evidence_cmp(a: &Evidence, b: &Evidence) -> std::cmp::Ordering {
    constraint_key(a.constraint)
        .cmp(&constraint_key(b.constraint))
        .then_with(|| a.path.cmp(&b.path))
}

fn format_evidence(evidence: &[Evidence]) -> String {
    let mut ev = evidence.to_vec();
    ev.sort_by(evidence_cmp);
    ev.iter()
        .map(|e| format!("\n  C:{} via {}", constraint_text(e.constraint), e.path.join(" -> ")))
        .collect()
}

pub fn error_text(error: &ResolveError) -> String {
    match error {
        ResolveError::InvalidName { site, name } => {
            format!("invalid-name|site={}|name={}", enc(site), enc(name))
        }
        ResolveError::DuplicatePackage { package } => {
            format!("duplicate-package|package={}", enc(package))
        }
        ResolveError::DuplicateVersion { package, version } => format!(
            "duplicate-version|package={}|version={}",
            enc(package),
            version_text(*version)
        ),
        ResolveError::DuplicateFeature {
            package,
            version,
            feature,
        } => format!(
            "duplicate-feature|package={}|version={}|feature={}",
            enc(package),
            version_text(*version),
            enc(feature)
        ),
        ResolveError::DuplicateLock { package } => {
            format!("duplicate-lock|package={}", enc(package))
        }
        ResolveError::Missing { package, evidence } => format!(
            "missing|package={}{}",
            enc(package),
            format_evidence(evidence)
        ),
        ResolveError::NoMatch {
            package,
            requirements,
            evidence,
        } => format!(
            "no-match|package={}|constraints={}{}",
            enc(package),
            constraints_text(requirements),
            format_evidence(evidence)
        ),
        ResolveError::Conflict {
            package,
            selected,
            requirements,
            evidence,
        } => format!(
            "conflict|package={}|selected={}|constraints={}{}",
            enc(package),
            version_text(*selected),
            constraints_text(requirements),
            format_evidence(evidence)
        ),
        ResolveError::MissingFeature {
            package,
            selected,
            feature,
            path,
        } => format!(
            "missing-feature|package={}|selected={}|feature={}\n  F:{} via {}",
            enc(package),
            version_text(*selected),
            enc(feature),
            enc(feature),
            path.join(" -> ")
        ),
        ResolveError::Limit { attempts } => format!("limit|attempts={}", attempts),
    }
}

fn check_name(site: String, name: &str) -> Result<(), ResolveError> {
    if valid_name(name) {
        Ok(())
    } else {
        Err(ResolveError::InvalidName {
            site,
            name: name.to_string(),
        })
    }
}

fn validate(
    catalog: &[Package],
    roots: &[Requirement],
    locks: &[Lock],
) -> Result<(), ResolveError> {
    for (i, root) in roots.iter().enumerate() {
        check_name(format!("root[{}]", i), &root.package)?;
        for (f, feature) in root.features.iter().enumerate() {
            check_name(format!("root[{}].feature[{}]", i, f), feature)?;
        }
    }

    for (i, package) in catalog.iter().enumerate() {
        check_name(format!("catalog[{}]", i), &package.name)?;
        for (j, release) in package.releases.iter().enumerate() {
            for (k, dep) in release.deps.iter().enumerate() {
                let site = format!("catalog[{}].release[{}].dep[{}]", i, j, k);
                check_name(site.clone(), &dep.package)?;
                if let Some(gate) = &dep.gate {
                    check_name(format!("{}.gate", site), gate)?;
                }
                for (f, feature) in dep.features.iter().enumerate() {
                    check_name(format!("{}.feature[{}]", site, f), feature)?;
                }
            }
            for (f, feature) in release.features.iter().enumerate() {
                let site = format!("catalog[{}].release[{}].feature[{}]", i, j, f);
                check_name(site.clone(), &feature.name)?;
                for (e, enabled) in feature.enables.iter().enumerate() {
                    check_name(format!("{}.enable[{}]", site, e), enabled)?;
                }
            }
        }
    }

    let mut packages = BTreeSet::new();
    for package in catalog {
        if !packages.insert(package.name.clone()) {
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
        check_name(format!("lock[{}]", i), &lock.package)?;
    }
    let mut lock_names = BTreeSet::new();
    for lock in locks {
        if !lock_names.insert(lock.package.clone()) {
            return Err(ResolveError::DuplicateLock {
                package: lock.package.clone(),
            });
        }
    }
    Ok(())
}

#[derive(Clone)]
struct Origin {
    constraint: Constraint,
    path: Vec<String>,
}

#[derive(Clone, Default)]
struct Active {
    constraints: Vec<Constraint>,
    origins: Vec<Origin>,
    requested: BTreeSet<String>,
    path: Option<Vec<String>>,
}

#[derive(Clone, Default)]
struct State {
    selected: BTreeMap<String, Version>,
}

struct Search<'a> {
    catalog: &'a [Package],
    roots: &'a [Requirement],
    locks: &'a [Lock],
    target: Target,
    max_attempts: usize,
    attempts: usize,
    best: Option<(usize, Vec<Option<Version>>, State, BTreeMap<String, Active>)>,
    first_error: Option<ResolveError>,
    limited: bool,
    catalog_names: Vec<String>,
}

impl<'a> Search<'a> {
    fn package(&self, name: &str) -> Option<&'a Package> {
        self.catalog.iter().find(|p| p.name == name)
    }

    fn release(&self, name: &str, version: Version) -> Option<&'a Release> {
        self.package(name)?
            .releases
            .iter()
            .find(|r| r.version == version)
    }

    fn target_matches(&self, platform: Platform) -> bool {
        matches!(
            (platform, self.target),
            (Platform::Any, _)
                | (Platform::Linux, Target::Linux)
                | (Platform::Mac, Target::Mac)
                | (Platform::Windows, Target::Windows)
        )
    }

    fn closure(&self, state: &State) -> BTreeMap<String, Active> {
        let mut active: BTreeMap<String, Active> = BTreeMap::new();
        for root in self.roots {
            let a = active.entry(root.package.clone()).or_default();
            a.constraints.push(root.constraint);
            a.requested.extend(root.features.iter().cloned());
            a.origins.push(Origin {
                constraint: root.constraint,
                path: vec![root_token(root)],
            });
        }

        let mut processed = BTreeSet::new();
        loop {
            let next = active.keys().find(|name| {
                state.selected.contains_key(*name) && !processed.contains(*name)
            }).cloned();

            let Some(name) = next else {
                break;
            };
            processed.insert(name.clone());

            let version = state.selected[&name];
            let Some(release) = self.release(&name, version) else {
                continue;
            };

            let mut activated = BTreeSet::new();
            let mut todo: Vec<String> = active[&name].requested.iter().cloned().collect();
            while let Some(feature) = todo.pop() {
                if !activated.insert(feature.clone()) {
                    continue;
                }
                if let Some(declaration) = release.features.iter().find(|f| f.name == feature) {
                    todo.extend(declaration.enables.iter().cloned());
                }
            }
            active.get_mut(&name).unwrap().requested.extend(activated.iter().cloned());

            for (j, dep) in release.deps.iter().enumerate() {
                if !self.target_matches(dep.platform)
                    || dep.gate.as_ref().is_some_and(|gate| !activated.contains(gate))
                {
                    continue;
                }

                let mut path = self
                    .canonical_path(&name, &active, state)
                    .unwrap_or_default();
                path.push(dep_token(&name, version, j, dep));

                let destination = active.entry(dep.package.clone()).or_default();
                destination.constraints.push(dep.constraint);
                destination.requested.extend(dep.features.iter().cloned());
                destination.origins.push(Origin {
                    constraint: dep.constraint,
                    path,
                });
            }
        }

        for a in active.values_mut() {
            a.constraints = canonical_constraints(&a.constraints);
        }

        let names: Vec<String> = active.keys().cloned().collect();
        for name in names {
            let path = self.canonical_path(&name, &active, state);
            active.get_mut(&name).unwrap().path = path;
        }
        active
    }

    fn canonical_path(
        &self,
        destination: &str,
        _active: &BTreeMap<String, Active>,
        state: &State,
    ) -> Option<Vec<String>> {
        let mut queue: Vec<(String, Vec<String>)> = Vec::new();
        for root in self.roots {
            if root.package == destination {
                queue.push((root.package.clone(), vec![root_token(root)]));
            }
        }

        let mut best: Option<Vec<String>> = None;
        let mut seen_depth: BTreeMap<String, usize> = BTreeMap::new();

        while !queue.is_empty() {
            queue.sort_by(|a, b| (a.1.len(), &a.1).cmp(&(b.1.len(), &b.1)));
            let (at, path) = queue.remove(0);

            if at == destination {
                if best
                    .as_ref()
                    .is_none_or(|current| (path.len(), &path) < (current.len(), current))
                {
                    best = Some(path);
                }
                continue;
            }

            if seen_depth.get(&at).is_some_and(|depth| *depth < path.len()) {
                continue;
            }
            seen_depth.insert(at.clone(), path.len());

            let Some(version) = state.selected.get(&at).copied() else {
                continue;
            };
            let Some(release) = self.release(&at, version) else {
                continue;
            };

            let mut activated: BTreeSet<String> = self
                .roots
                .iter()
                .filter(|r| r.package == at)
                .flat_map(|r| r.features.iter().cloned())
                .collect();
            let mut todo: Vec<String> = activated.iter().cloned().collect();
            while let Some(feature) = todo.pop() {
                if let Some(declaration) = release.features.iter().find(|f| f.name == feature) {
                    for enabled in &declaration.enables {
                        if activated.insert(enabled.clone()) {
                            todo.push(enabled.clone());
                        }
                    }
                }
            }

            for (j, dep) in release.deps.iter().enumerate() {
                if !self.target_matches(dep.platform)
                    || dep.gate.as_ref().is_some_and(|gate| !activated.contains(gate))
                {
                    continue;
                }
                let mut next_path = path.clone();
                next_path.push(dep_token(&at, version, j, dep));
                queue.push((dep.package.clone(), next_path));
            }
        }

        best
    }

    fn evidence(&self, active: &Active) -> Vec<Evidence> {
        let mut evidence: Vec<Evidence> = active
            .origins
            .iter()
            .map(|origin| Evidence {
                constraint: origin.constraint,
                path: origin.path.clone(),
            })
            .collect();
        evidence.sort_by(evidence_cmp);
        evidence
    }

    fn node_error(
        &self,
        state: &State,
        active: &BTreeMap<String, Active>,
    ) -> Option<ResolveError> {
        for name in active.keys() {
            if self.package(name).is_none() {
                return Some(ResolveError::Missing {
                    package: name.clone(),
                    evidence: self.evidence(&active[name]),
                });
            }
        }

        for (name, version) in &state.selected {
            if let Some(a) = active.get(name) {
                if !matches_all(&a.constraints, *version) {
                    return Some(ResolveError::Conflict {
                        package: name.clone(),
                        selected: *version,
                        requirements: a.constraints.clone(),
                        evidence: self.evidence(a),
                    });
                }
            }
        }

        for (name, a) in active {
            if state.selected.contains_key(name) {
                continue;
            }
            if self.candidates(name, &a.constraints).is_empty() {
                return Some(ResolveError::NoMatch {
                    package: name.clone(),
                    requirements: a.constraints.clone(),
                    evidence: self.evidence(a),
                });
            }
        }

        for (name, version) in &state.selected {
            let Some(a) = active.get(name) else {
                continue;
            };
            let Some(release) = self.release(name, *version) else {
                continue;
            };
            for feature in &a.requested {
                if !release.features.iter().any(|f| f.name == *feature) {
                    return Some(ResolveError::MissingFeature {
                        package: name.clone(),
                        selected: *version,
                        feature: feature.clone(),
                        path: a.path.clone().unwrap_or_default(),
                    });
                }
            }
        }
        None
    }

    fn candidates(&self, name: &str, constraints: &[Constraint]) -> Vec<Version> {
        let Some(package) = self.package(name) else {
            return Vec::new();
        };
        let mut candidates: Vec<Version> = package
            .releases
            .iter()
            .map(|r| r.version)
            .filter(|v| matches_all(constraints, *v))
            .collect();
        candidates.sort_by(|a, b| b.cmp(a));

        if let Some(lock) = self.locks.iter().find(|l| l.package == name) {
            if let Some(index) = candidates.iter().position(|v| *v == lock.version) {
                let version = candidates.remove(index);
                candidates.insert(0, version);
            }
        }
        candidates
    }

    fn solution_cost(&self, state: &State) -> usize {
        self.locks
            .iter()
            .filter(|lock| state.selected.get(&lock.package) != Some(&lock.version))
            .count()
    }

    fn vector(&self, state: &State) -> Vec<Option<Version>> {
        self.catalog_names
            .iter()
            .map(|name| state.selected.get(name).copied())
            .collect()
    }

    fn record_solution(
        &mut self,
        state: &State,
        active: BTreeMap<String, Active>,
    ) {
        let cost = self.solution_cost(state);
        let vector = self.vector(state);
        let replace = self.best.as_ref().is_none_or(|(best_cost, best_vector, _, _)| {
            cost < *best_cost || (cost == *best_cost && vector > *best_vector)
        });
        if replace {
            self.best = Some((cost, vector, state.clone(), active));
        }
    }

    fn search(&mut self, state: State) {
        if self.limited {
            return;
        }
        let active = self.closure(&state);
        if let Some(error) = self.node_error(&state, &active) {
            if self.first_error.is_none() {
                self.first_error = Some(error);
            }
            return;
        }

        let Some(name) = active
            .keys()
            .find(|name| !state.selected.contains_key(*name))
            .cloned()
        else {
            self.record_solution(&state, active);
            return;
        };

        let candidates = self.candidates(&name, &active[&name].constraints);
        for version in candidates {
            if self.attempts == self.max_attempts {
                self.limited = true;
                return;
            }
            self.attempts += 1;
            let mut child = state.clone();
            child.selected.insert(name.clone(), version);
            self.search(child);
            if self.limited {
                return;
            }
        }
    }
}

fn sorted_features(features: &[String]) -> Vec<String> {
    let mut out = features.to_vec();
    out.sort();
    out.dedup();
    out
}

fn root_token(root: &Requirement) -> String {
    format!(
        "root:{}@{}+{{{}}}",
        root.package,
        constraint_text(root.constraint),
        sorted_features(&root.features).join(",")
    )
}

fn dep_token(source: &str, version: Version, index: usize, dep: &Dependency) -> String {
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

fn build_changes(
    locks: &[Lock],
    selected: &BTreeMap<String, Version>,
    active: &BTreeMap<String, Active>,
    catalog: &[Package],
) -> Vec<Change> {
    let old: BTreeMap<String, Version> = locks
        .iter()
        .map(|lock| (lock.package.clone(), lock.version))
        .collect();
    let mut names = BTreeSet::new();
    names.extend(old.keys().cloned());
    names.extend(selected.keys().cloned());

    let mut changes = Vec::new();
    for name in names {
        let before = old.get(&name).copied();
        let after = selected.get(&name).copied();
        if before == after {
            continue;
        }

        let reason = match (before, after) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(locked), Some(_)) => {
                let exists = catalog
                    .iter()
                    .find(|p| p.name == name)
                    .is_some_and(|p| p.releases.iter().any(|r| r.version == locked));
                if !exists {
                    ChangeReason::Unavailable
                } else if active
                    .get(&name)
                    .is_some_and(|a| !matches_all(&a.constraints, locked))
                {
                    ChangeReason::Rejected
                } else {
                    ChangeReason::Rebalanced
                }
            }
            (None, None) => continue,
        };

        changes.push(Change {
            package: name,
            before,
            after,
            reason,
        });
    }
    changes
}

pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
    locks: &[Lock],
    target: Target,
    max_attempts: usize,
) -> Result<Resolution, ResolveError> {
    validate(catalog, roots, locks)?;

    let mut catalog_names: Vec<String> = catalog.iter().map(|p| p.name.clone()).collect();
    catalog_names.sort();
    catalog_names.dedup();

    let mut search = Search {
        catalog,
        roots,
        locks,
        target,
        max_attempts,
        attempts: 0,
        best: None,
        first_error: None,
        limited: false,
        catalog_names,
    };
    search.search(State::default());

    if search.limited {
        return Err(ResolveError::Limit {
            attempts: max_attempts,
        });
    }

    if let Some((_, _, state, active)) = search.best {
        let selected: Vec<Selection> = state
            .selected
            .iter()
            .map(|(package, version)| Selection {
                package: package.clone(),
                version: *version,
            })
            .collect();
        let changes = build_changes(locks, &state.selected, &active, catalog);
        Ok(Resolution { selected, changes })
    } else {
        Err(search
            .first_error
            .unwrap_or(ResolveError::Limit { attempts: max_attempts }))
    }
}

fn main() {}