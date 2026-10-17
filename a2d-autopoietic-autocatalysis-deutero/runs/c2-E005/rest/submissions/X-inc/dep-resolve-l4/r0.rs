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

fn constraint_key(c: Constraint) -> (u8, Version) {
    match c {
        Constraint::Exact(v) => (0, v),
        Constraint::AtLeast(v) => (1, v),
        Constraint::Before(v) => (2, v),
        Constraint::Caret(v) => (3, v),
    }
}

fn canonical_constraints(cs: &[Constraint]) -> Vec<Constraint> {
    let mut out = cs.to_vec();
    out.sort_by_key(|c| constraint_key(*c));
    out.dedup();
    out
}

fn matches(v: Version, c: Constraint) -> bool {
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

fn percent_encode(s: &str) -> String {
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

fn evidence_sorted(evidence: &[Evidence]) -> Vec<Evidence> {
    let mut out = evidence.to_vec();
    out.sort_by(|a, b| {
        constraint_key(a.constraint)
            .cmp(&constraint_key(b.constraint))
            .then_with(|| a.path.cmp(&b.path))
    });
    out
}

fn error_text(error: &ResolveError) -> String {
    fn constraints_text(cs: &[Constraint]) -> String {
        canonical_constraints(cs)
            .iter()
            .map(|c| constraint_text(*c))
            .collect::<Vec<_>>()
            .join(",")
    }

    fn append_evidence(s: &mut String, evidence: &[Evidence]) {
        for e in evidence_sorted(evidence) {
            s.push_str("\n  C:");
            s.push_str(&constraint_text(e.constraint));
            s.push_str(" via ");
            s.push_str(&e.path.join(" -> "));
        }
    }

    match error {
        ResolveError::InvalidName { site, name } => format!(
            "invalid-name|site={}|name={}",
            percent_encode(site),
            percent_encode(name)
        ),
        ResolveError::DuplicatePackage { package } => {
            format!("duplicate-package|package={}", percent_encode(package))
        }
        ResolveError::DuplicateVersion { package, version } => format!(
            "duplicate-version|package={}|version={}",
            percent_encode(package),
            version_text(*version)
        ),
        ResolveError::DuplicateFeature {
            package,
            version,
            feature,
        } => format!(
            "duplicate-feature|package={}|version={}|feature={}",
            percent_encode(package),
            version_text(*version),
            percent_encode(feature)
        ),
        ResolveError::DuplicateLock { package } => {
            format!("duplicate-lock|package={}", percent_encode(package))
        }
        ResolveError::Missing { package, evidence } => {
            let mut s = format!("missing|package={}", percent_encode(package));
            append_evidence(&mut s, evidence);
            s
        }
        ResolveError::NoMatch {
            package,
            requirements,
            evidence,
        } => {
            let mut s = format!(
                "no-match|package={}|constraints={}",
                percent_encode(package),
                constraints_text(requirements)
            );
            append_evidence(&mut s, evidence);
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
                percent_encode(package),
                version_text(*selected),
                constraints_text(requirements)
            );
            append_evidence(&mut s, evidence);
            s
        }
        ResolveError::MissingFeature {
            package,
            selected,
            feature,
            path,
        } => format!(
            "missing-feature|package={}|selected={}|feature={}\n  F:{} via {}",
            percent_encode(package),
            version_text(*selected),
            percent_encode(feature),
            percent_encode(feature),
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
    origins: BTreeMap<String, Vec<Origin>>,
    requests: BTreeMap<String, BTreeSet<String>>,
    activated: BTreeMap<String, BTreeSet<String>>,
    paths: BTreeMap<String, Vec<String>>,
}

use std::collections::{BTreeMap, BTreeSet};

fn target_text(t: Target) -> &'static str {
    match t {
        Target::Linux => "linux",
        Target::Mac => "mac",
        Target::Windows => "windows",
    }
}

fn platform_matches(p: Platform, t: Target) -> bool {
    matches!(p, Platform::Any)
        || matches!(
            (p, t),
            (Platform::Linux, Target::Linux)
                | (Platform::Mac, Target::Mac)
                | (Platform::Windows, Target::Windows)
        )
}

fn path_better(a: &[String], b: &[String]) -> bool {
    a.len() < b.len() || (a.len() == b.len() && a < b)
}

fn set_path(paths: &mut BTreeMap<String, Vec<String>>, package: &str, path: Vec<String>) -> bool {
    match paths.get(package) {
        None => {
            paths.insert(package.to_string(), path);
            true
        }
        Some(old) if path_better(&path, old) => {
            paths.insert(package.to_string(), path);
            true
        }
        _ => false,
    }
}

fn release_for<'a>(catalog: &'a [Package], package: &str, version: Version) -> Option<&'a Release> {
    catalog
        .iter()
        .find(|p| p.name == package)
        .and_then(|p| p.releases.iter().find(|r| r.version == version))
}

fn active_closure(
    catalog: &[Package],
    roots: &[Requirement],
    selected: &BTreeMap<String, Version>,
    target: Target,
) -> Active {
    let mut active = Active::default();

    for (i, root) in roots.iter().enumerate() {
        let fs: BTreeSet<String> = root.features.iter().cloned().collect();
        let ftext = fs.iter().cloned().collect::<Vec<_>>().join(",");
        let token = format!(
            "root[{}]:{}@{}+{{{}}}",
            i,
            root.package,
            constraint_text(root.constraint),
            ftext
        );
        active
            .origins
            .entry(root.package.clone())
            .or_default()
            .push(Origin {
                constraint: root.constraint,
                path: vec![token.clone()],
            });
        active
            .requests
            .entry(root.package.clone())
            .or_default()
            .extend(fs);
        set_path(&mut active.paths, &root.package, vec![token]);
    }

    loop {
        let mut changed = false;

        let packages: Vec<String> = selected.keys().cloned().collect();
        for package in packages {
            let Some(version) = selected.get(&package).copied() else {
                continue;
            };
            let Some(release) = release_for(catalog, &package, version) else {
                continue;
            };

            let reqs = active.requests.get(&package).cloned().unwrap_or_default();
            let acts = active.activated.entry(package.clone()).or_default();
            for name in reqs {
                if acts.insert(name) {
                    changed = true;
                }
            }

            let acts_now = active.activated.get(&package).cloned().unwrap_or_default();
            for feature_name in acts_now {
                if let Some(feature) = release.features.iter().find(|f| f.name == feature_name) {
                    for enabled in &feature.enables {
                        if active
                            .requests
                            .entry(package.clone())
                            .or_default()
                            .insert(enabled.clone())
                        {
                            changed = true;
                        }
                    }
                }
            }

            let source_path = active.paths.get(&package).cloned();
            for (j, dep) in release.deps.iter().enumerate() {
                if !platform_matches(dep.platform, target) {
                    continue;
                }
                if let Some(gate) = &dep.gate {
                    if !active
                        .activated
                        .get(&package)
                        .is_some_and(|set| set.contains(gate))
                    {
                        continue;
                    }
                }

                if let Some(path) = source_path.as_ref() {
                    let fs: BTreeSet<String> = dep.features.iter().cloned().collect();
                    let ftext = fs.iter().cloned().collect::<Vec<_>>().join(",");
                    let gate_text = dep.gate.as_deref().unwrap_or("*");
                    let token = format!(
                        "{}@{}/deps[{}]:{}@{}+{{{}}};gate={};platform={}",
                        package,
                        version_text(version),
                        j,
                        dep.package,
                        constraint_text(dep.constraint),
                        ftext,
                        gate_text,
                        match dep.platform {
                            Platform::Any => "any",
                            Platform::Linux => "linux",
                            Platform::Mac => "mac",
                            Platform::Windows => "windows",
                        }
                    );
                    let mut path_to_dep = path.clone();
                    path_to_dep.push(token);
                    if set_path(&mut active.paths, &dep.package, path_to_dep.clone()) {
                        changed = true;
                    }
                    let origins = active.origins.entry(dep.package.clone()).or_default();
                    if !origins.iter().any(|o| o.path == path_to_dep && o.constraint == dep.constraint)
                    {
                        origins.push(Origin {
                            constraint: dep.constraint,
                            path: path_to_dep,
                        });
                        changed = true;
                    }
                    let requests = active.requests.entry(dep.package.clone()).or_default();
                    for f in fs {
                        if requests.insert(f) {
                            changed = true;
                        }
                    }
                }
            }
        }

        if !changed {
            break;
        }
    }

    active
}

fn canonical_origins(origins: &[Origin]) -> Vec<Evidence> {
    let mut out: Vec<Evidence> = origins
        .iter()
        .map(|o| Evidence {
            constraint: o.constraint,
            path: o.path.clone(),
        })
        .collect();
    out.sort_by(|a, b| {
        constraint_key(a.constraint)
            .cmp(&constraint_key(b.constraint))
            .then_with(|| a.path.cmp(&b.path))
    });
    out
}

fn active_constraints(active: &Active, package: &str) -> Vec<Constraint> {
    let cs: Vec<Constraint> = active
        .origins
        .get(package)
        .into_iter()
        .flatten()
        .map(|o| o.constraint)
        .collect();
    canonical_constraints(&cs)
}

struct Search<'a> {
    catalog: &'a [Package],
    roots: &'a [Requirement],
    locks: &'a [Lock],
    target: Target,
    attempts: usize,
    max_attempts: usize,
    first_error: Option<ResolveError>,
    best: Option<BTreeMap<String, Version>>,
    best_cost: usize,
    catalog_names: Vec<String>,
}

impl<'a> Search<'a> {
    fn visit(&mut self, selected: &mut BTreeMap<String, Version>) -> Result<(), ResolveError> {
        let active = active_closure(self.catalog, self.roots, selected, self.target);

        let mut missing: Vec<String> = active
            .origins
            .keys()
            .filter(|name| !self.catalog.iter().any(|p| &p.name == *name))
            .cloned()
            .collect();
        missing.sort();
        if let Some(package) = missing.first() {
            let evidence = canonical_origins(active.origins.get(package).unwrap());
            self.record_error(ResolveError::Missing {
                package: package.clone(),
                evidence,
            });
            return Ok(());
        }

        for (package, version) in selected.iter() {
            let requirements = active_constraints(&active, package);
            if requirements.iter().any(|c| !matches(*version, *c)) {
                let evidence = canonical_origins(active.origins.get(package).unwrap_or(&Vec::new()));
                self.record_error(ResolveError::Conflict {
                    package: package.clone(),
                    selected: *version,
                    requirements,
                    evidence,
                });
                return Ok(());
            }
        }

        for package in selected.keys() {
            let Some(release) = release_for(self.catalog, package, selected[package]) else {
                continue;
            };
            if let Some(features) = active.activated.get(package) {
                for feature in features {
                    if !release.features.iter().any(|f| &f.name == feature) {
                        self.record_error(ResolveError::MissingFeature {
                            package: package.clone(),
                            selected: selected[package],
                            feature: feature.clone(),
                            path: active.paths.get(package).cloned().unwrap_or_default(),
                        });
                        return Ok(());
                    }
                }
            }
        }

        let mut unselected: Vec<String> = active
            .origins
            .keys()
            .filter(|p| !selected.contains_key(*p))
            .cloned()
            .collect();
        unselected.sort();

        let mut candidates_by_package = BTreeMap::new();
        for package in &unselected {
            let requirements = active_constraints(&active, package);
            let mut candidates: Vec<Version> = self
                .catalog
                .iter()
                .find(|p| &p.name == package)
                .unwrap()
                .releases
                .iter()
                .map(|r| r.version)
                .filter(|v| requirements.iter().all(|c| matches(*v, *c)))
                .collect();
            candidates.sort_by(|a, b| b.cmp(a));
            candidates.dedup();
            if candidates.is_empty() {
                let evidence = canonical_origins(active.origins.get(package).unwrap());
                self.record_error(ResolveError::NoMatch {
                    package: package.clone(),
                    requirements,
                    evidence,
                });
                return Ok(());
            }
            if let Some(lock) = self.locks.iter().find(|l| l.package == *package) {
                if let Some(pos) = candidates.iter().position(|v| *v == lock.version) {
                    let v = candidates.remove(pos);
                    candidates.insert(0, v);
                }
            }
            candidates_by_package.insert(package.clone(), candidates);
        }

        if unselected.is_empty() {
            self.record_solution(selected);
            return Ok(());
        }

        let package = &unselected[0];
        let candidates = candidates_by_package.get(package).unwrap().clone();
        for version in candidates {
            if self.attempts == self.max_attempts {
                return Err(ResolveError::Limit {
                    attempts: self.max_attempts,
                });
            }
            self.attempts += 1;
            selected.insert(package.clone(), version);
            let result = self.visit(selected);
            selected.remove(package);
            result?;
        }
        Ok(())
    }

    fn record_error(&mut self, error: ResolveError) {
        if self.first_error.is_none() {
            self.first_error = Some(error);
        }
    }

    fn record_solution(&mut self, selected: &BTreeMap<String, Version>) {
        let cost = self
            .locks
            .iter()
            .filter(|lock| selected.get(&lock.package) != Some(&lock.version))
            .count();
        let better = match &self.best {
            None => true,
            Some(best) => {
                if cost != self.best_cost {
                    cost < self.best_cost
                } else {
                    self.catalog_names.iter().find_map(|name| {
                        let a = selected.get(name);
                        let b = best.get(name);
                        if a != b {
                            Some(match (a, b) {
                                (None, Some(_)) => false,
                                (Some(_), None) => true,
                                (Some(x), Some(y)) => x > y,
                                (None, None) => false,
                            })
                        } else {
                            None
                        }
                    }).unwrap_or(false)
                }
            }
        };
        if better {
            self.best = Some(selected.clone());
            self.best_cost = cost;
        }
    }
}

fn validate(catalog: &[Package], roots: &[Requirement], locks: &[Lock]) -> Result<(), ResolveError> {
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
                let base = format!("catalog[{}].release[{}].feature[{}]", i, j, f);
                if !valid_name(&feature.name) {
                    return Err(ResolveError::InvalidName {
                        site: base.clone(),
                        name: feature.name.clone(),
                    });
                }
                for (e, enabled) in feature.enables.iter().enumerate() {
                    if !valid_name(enabled) {
                        return Err(ResolveError::InvalidName {
                            site: format!("{}.enable[{}]", base, e),
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

    for (i, lock) in locks.iter().enumerate() {
        if !valid_name(&lock.package) {
            return Err(ResolveError::InvalidName {
                site: format!("lock[{}]", i),
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

    Ok(())
}

pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
    locks: &[Lock],
    target: Target,
    max_attempts: usize,
) -> Result<Resolution, ResolveError> {
    validate(catalog, roots, locks)?;

    let mut names: Vec<String> = catalog.iter().map(|p| p.name.clone()).collect();
    names.sort();
    names.dedup();

    let mut search = Search {
        catalog,
        roots,
        locks,
        target,
        attempts: 0,
        max_attempts,
        first_error: None,
        best: None,
        best_cost: usize::MAX,
        catalog_names: names,
    };

    search.visit(&mut BTreeMap::new())?;
    let Some(best) = search.best else {
        return Err(search.first_error.unwrap_or(ResolveError::Missing {
            package: String::new(),
            evidence: Vec::new(),
        }));
    };

    let selected: Vec<Selection> = best
        .iter()
        .map(|(package, version)| Selection {
            package: package.clone(),
            version: *version,
        })
        .collect();

    let before: BTreeMap<String, Version> = locks
        .iter()
        .map(|l| (l.package.clone(), l.version))
        .collect();
    let mut all_names: BTreeSet<String> = before.keys().cloned().collect();
    all_names.extend(best.keys().cloned());

    let final_active = active_closure(catalog, roots, &best, target);
    let mut changes = Vec::new();
    for package in all_names {
        let b = before.get(&package).copied();
        let a = best.get(&package).copied();
        if b == a {
            continue;
        }
        let reason = match (b, a) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old), Some(_)) => {
                let exists = catalog
                    .iter()
                    .find(|p| p.name == package)
                    .is_some_and(|p| p.releases.iter().any(|r| r.version == old));
                if !exists {
                    ChangeReason::Unavailable
                } else if active_constraints(&final_active, &package)
                    .iter()
                    .any(|c| !matches(old, *c))
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
            before: b,
            after: a,
            reason,
        });
    }

    Ok(Resolution { selected, changes })
}