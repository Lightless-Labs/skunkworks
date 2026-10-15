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

fn evidence_sorted(evidence: &[Evidence]) -> Vec<Evidence> {
    let mut e = evidence.to_vec();
    e.sort_by(|a, b| {
        a.constraint
            .cmp(&b.constraint)
            .then_with(|| a.path.cmp(&b.path))
    });
    e
}

fn append_evidence(line: &mut String, evidence: &[Evidence]) {
    for e in evidence_sorted(evidence) {
        line.push_str("\n  ");
        line.push_str(&constraint_text(e.constraint));
        line.push_str(" via ");
        line.push_str(&e.path.join(" -> "));
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
            let mut s = format!("missing|package={}", encode(package));
            append_evidence(&mut s, evidence);
            s
        }
        ResolveError::NoMatch {
            package,
            requirements,
            evidence,
        } => {
            let cs = canonical_constraints(requirements)
                .iter()
                .map(|c| constraint_text(*c))
                .collect::<Vec<_>>()
                .join(",");
            let mut s = format!("no-match|package={}|constraints={}", encode(package), cs);
            append_evidence(&mut s, evidence);
            s
        }
        ResolveError::Conflict {
            package,
            selected,
            requirements,
            evidence,
        } => {
            let cs = canonical_constraints(requirements)
                .iter()
                .map(|c| constraint_text(*c))
                .collect::<Vec<_>>()
                .join(",");
            let mut s = format!(
                "conflict|package={}|selected={}|constraints={}",
                encode(package),
                version_text(*selected),
                cs
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

type Origin = (Constraint, Vec<String>);

#[derive(Clone, Default)]
struct Active {
    origins: BTreeMap<String, Vec<Origin>>,
    requests: BTreeMap<String, BTreeSet<String>>,
    paths: BTreeMap<String, Vec<String>>,
    activated: BTreeMap<String, BTreeSet<String>>,
}

fn feature_names(release: &Release) -> BTreeMap<String, Vec<String>> {
    release
        .features
        .iter()
        .map(|f| (f.name.clone(), f.enables.clone()))
        .collect()
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

fn root_token(i: usize, r: &Requirement) -> String {
    let fs: BTreeSet<_> = r.features.iter().cloned().collect();
    format!(
        "root[{}]:{}@{}+{{{}}}",
        i,
        r.package,
        constraint_text(r.constraint),
        fs.into_iter().collect::<Vec<_>>().join(",")
    )
}

fn dep_token(source: &str, version: Version, index: usize, dep: &Dependency) -> String {
    let fs: BTreeSet<_> = dep.features.iter().cloned().collect();
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
        fs.into_iter().collect::<Vec<_>>().join(","),
        gate,
        platform
    )
}

fn insert_path(paths: &mut BTreeMap<String, Vec<String>>, package: &str, candidate: Vec<String>) {
    match paths.get(package) {
        None => {
            paths.insert(package.to_string(), candidate);
        }
        Some(old) if (candidate.len(), &candidate) < (old.len(), old) => {
            paths.insert(package.to_string(), candidate);
        }
        _ => {}
    }
}

fn closure(
    catalog: &BTreeMap<String, &Package>,
    roots: &[Requirement],
    selected: &BTreeMap<String, Version>,
    target: Target,
) -> Active {
    let mut active = Active::default();
    for (i, root) in roots.iter().enumerate() {
        let path = vec![root_token(i, root)];
        active
            .origins
            .entry(root.package.clone())
            .or_default()
            .push((root.constraint, path.clone()));
        active.requests
            .entry(root.package.clone())
            .or_default()
            .extend(root.features.iter().cloned());
        insert_path(&mut active.paths, &root.package, path);
    }

    loop {
        let mut changed = false;
        let package_names: Vec<String> = selected.keys().cloned().collect();
        for package in package_names {
            let Some(path) = active.paths.get(&package).cloned() else {
                continue;
            };
            let version = selected[&package];
            let Some(pkg) = catalog.get(&package) else {
                continue;
            };
            let Some(release) = pkg.releases.iter().find(|r| r.version == version) else {
                continue;
            };
            let declarations = feature_names(release);
            let requests = active.requests.get(&package).cloned().unwrap_or_default();
            let activated = active.activated.entry(package.clone()).or_default();
            for feature in requests {
                if activated.insert(feature) {
                    changed = true;
                }
            }
            loop {
                let current: Vec<String> = active.activated[&package].iter().cloned().collect();
                let mut feature_changed = false;
                for f in current {
                    if let Some(enables) = declarations.get(&f) {
                        for enabled in enables {
                            if active
                                .activated
                                .entry(package.clone())
                                .or_default()
                                .insert(enabled.clone())
                            {
                                feature_changed = true;
                                changed = true;
                            }
                        }
                    }
                }
                if !feature_changed {
                    break;
                }
            }
            let enabled = active.activated.get(&package).cloned().unwrap_or_default();
            for (j, dep) in release.deps.iter().enumerate() {
                if !platform_matches(dep.platform, target)
                    || dep.gate.as_ref().is_some_and(|g| !enabled.contains(g))
                {
                    continue;
                }
                let token = dep_token(&package, version, j, dep);
                let mut dep_path = path.clone();
                dep_path.push(token.clone());
                active
                    .origins
                    .entry(dep.package.clone())
                    .or_default()
                    .push((dep.constraint, dep_path.clone()));
                let reqs = active.requests.entry(dep.package.clone()).or_default();
                for f in &dep.features {
                    if reqs.insert(f.clone()) {
                        changed = true;
                    }
                }
                let before_path = active.paths.get(&dep.package).cloned();
                insert_path(&mut active.paths, &dep.package, dep_path);
                if active.paths.get(&dep.package) != before_path.as_ref() {
                    changed = true;
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
    let mut result: Vec<Evidence> = origins
        .iter()
        .map(|(constraint, path)| Evidence {
            constraint: *constraint,
            path: path.clone(),
        })
        .collect();
    result.sort_by(|a, b| {
        a.constraint
            .cmp(&b.constraint)
            .then_with(|| a.path.cmp(&b.path))
    });
    result
}

fn active_requirements(active: &Active, package: &str) -> Vec<Constraint> {
    canonical_constraints(
        &active
            .origins
            .get(package)
            .into_iter()
            .flatten()
            .map(|(c, _)| *c)
            .collect::<Vec<_>>(),
    )
}

fn inspect(
    catalog: &BTreeMap<String, &Package>,
    roots: &[Requirement],
    selected: &BTreeMap<String, Version>,
    target: Target,
) -> Result<(Active, Option<String>), ResolveError> {
    let active = closure(catalog, roots, selected, target);
    for package in active.origins.keys() {
        if !catalog.contains_key(package) {
            return Err(ResolveError::Missing {
                package: package.clone(),
                evidence: canonical_origins(&active.origins[package]),
            });
        }
    }
    for (package, version) in selected {
        if let Some(origins) = active.origins.get(package) {
            if !origins.iter().all(|(c, _)| matches(*c, *version)) {
                return Err(ResolveError::Conflict {
                    package: package.clone(),
                    selected: *version,
                    requirements: active_requirements(&active, package),
                    evidence: canonical_origins(origins),
                });
            }
        }
    }
    for package in selected.keys() {
        let Some(path) = active.paths.get(package) else {
            continue;
        };
        let version = selected[package];
        let Some(pkg) = catalog.get(package) else {
            continue;
        };
        let Some(release) = pkg.releases.iter().find(|r| r.version == version) else {
            continue;
        };
        let declarations = feature_names(release);
        for feature in active
            .activated
            .get(package)
            .into_iter()
            .flatten()
        {
            if !declarations.contains_key(feature) {
                return Err(ResolveError::MissingFeature {
                    package: package.clone(),
                    selected: version,
                    feature: feature.clone(),
                    path: path.clone(),
                });
            }
        }
    }
    let active_unselected: Vec<String> = active
        .origins
        .keys()
        .filter(|p| !selected.contains_key(*p))
        .cloned()
        .collect();
    for package in &active_unselected {
        let pkg = catalog[package];
        let requirements = active_requirements(&active, package);
        if !pkg.releases.iter().any(|r| {
            requirements.iter().all(|c| matches(*c, r.version))
        }) {
            return Err(ResolveError::NoMatch {
                package: package.clone(),
                requirements,
                evidence: canonical_origins(&active.origins[package]),
            });
        }
    }
    Ok((active, active_unselected.into_iter().next()))
}

struct Search {
    attempts: usize,
    budget: usize,
    best: Option<(usize, Vec<Selection>, Active)>,
    first_error: Option<ResolveError>,
    limit: bool,
}

fn solution_cost(selected: &BTreeMap<String, Version>, locks: &[Lock]) -> usize {
    locks
        .iter()
        .filter(|lock| selected.get(&lock.package) != Some(&lock.version))
        .count()
}

fn better_solution(
    selected: &BTreeMap<String, Version>,
    cost: usize,
    best: &Option<(usize, Vec<Selection>, Active)>,
    catalog_names: &[String],
) -> bool {
    let Some((best_cost, best_selected, _)) = best else {
        return true;
    };
    if cost != *best_cost {
        return cost < *best_cost;
    }
    let best_map: BTreeMap<&str, Version> = best_selected
        .iter()
        .map(|s| (s.package.as_str(), s.version))
        .collect();
    for name in catalog_names {
        match (selected.get(name), best_map.get(name.as_str())) {
            (Some(a), Some(b)) if a != b => return a > b,
            (Some(_), None) => return true,
            (None, Some(_)) => return false,
            _ => {}
        }
    }
    false
}

fn search(
    catalog: &BTreeMap<String, &Package>,
    roots: &[Requirement],
    locks: &[Lock],
    target: Target,
    selected: &mut BTreeMap<String, Version>,
    state: &mut Search,
    catalog_names: &[String],
) {
    if state.limit {
        return;
    }
    let inspected = inspect(catalog, roots, selected, target);
    let (active, branch) = match inspected {
        Err(ResolveError::Limit { .. }) => unreachable!(),
        Err(e) => {
            if state.first_error.is_none() {
                state.first_error = Some(e);
            }
            return;
        }
        Ok(v) => v,
    };
    let Some(package) = branch else {
        let cost = solution_cost(selected, locks);
        if better_solution(selected, cost, &state.best, catalog_names) {
            let mut sels: Vec<Selection> = selected
                .iter()
                .map(|(package, version)| Selection {
                    package: package.clone(),
                    version: *version,
                })
                .collect();
            sels.sort_by(|a, b| a.package.cmp(&b.package));
            state.best = Some((cost, sels, active));
        }
        return;
    };
    let pkg = catalog[&package];
    let requirements = active_requirements(&active, &package);
    let mut candidates: Vec<Version> = pkg
        .releases
        .iter()
        .filter(|r| requirements.iter().all(|c| matches(*c, r.version)))
        .map(|r| r.version)
        .collect();
    candidates.sort_by(|a, b| b.cmp(a));
    if let Some(lock) = locks.iter().find(|l| l.package == package) {
        if let Some(i) = candidates.iter().position(|v| *v == lock.version) {
            let v = candidates.remove(i);
            candidates.insert(0, v);
        }
    }
    for version in candidates {
        if state.attempts == state.budget {
            state.limit = true;
            return;
        }
        state.attempts += 1;
        selected.insert(package.clone(), version);
        search(
            catalog,
            roots,
            locks,
            target,
            selected,
            state,
            catalog_names,
        );
        selected.remove(&package);
        if state.limit {
            return;
        }
    }
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
        for (f, name) in root.features.iter().enumerate() {
            if !valid_name(name) {
                return Err(ResolveError::InvalidName {
                    site: format!("root[{}].feature[{}]", i, f),
                    name: name.clone(),
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
                for (f, name) in dep.features.iter().enumerate() {
                    if !valid_name(name) {
                        return Err(ResolveError::InvalidName {
                            site: format!(
                                "catalog[{}].release[{}].dep[{}].feature[{}]",
                                i, j, k, f
                            ),
                            name: name.clone(),
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
                for (e, name) in feature.enables.iter().enumerate() {
                    if !valid_name(name) {
                        return Err(ResolveError::InvalidName {
                            site: format!(
                                "catalog[{}].release[{}].feature[{}].enable[{}]",
                                i, j, f, e
                            ),
                            name: name.clone(),
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
    validate_inputs(catalog, roots, locks)?;
    let by_name: BTreeMap<String, &Package> = catalog
        .iter()
        .map(|p| (p.name.clone(), p))
        .collect();
    let catalog_names: Vec<String> = by_name.keys().cloned().collect();
    let mut selected = BTreeMap::new();
    let mut state = Search {
        attempts: 0,
        budget: max_attempts,
        best: None,
        first_error: None,
        limit: false,
    };
    search(
        &by_name,
        roots,
        locks,
        target,
        &mut selected,
        &mut state,
        &catalog_names,
    );
    if state.limit {
        return Err(ResolveError::Limit {
            attempts: max_attempts,
        });
    }
    let Some((_, selected, active)) = state.best else {
        return Err(state.first_error.unwrap_or_else(|| ResolveError::Missing {
            package: String::new(),
            evidence: Vec::new(),
        }));
    };
    let selected_map: BTreeMap<String, Version> = selected
        .iter()
        .map(|s| (s.package.clone(), s.version))
        .collect();
    let locks_map: BTreeMap<String, Version> = locks
        .iter()
        .map(|l| (l.package.clone(), l.version))
        .collect();
    let all_names: BTreeSet<String> = locks_map
        .keys()
        .chain(selected_map.keys())
        .cloned()
        .collect();
    let mut changes = Vec::new();
    for package in all_names {
        let before = locks_map.get(&package).copied();
        let after = selected_map.get(&package).copied();
        if before == after {
            continue;
        }
        let reason = match (before, after) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old), Some(_)) => {
                let exists = by_name
                    .get(&package)
                    .is_some_and(|p| p.releases.iter().any(|r| r.version == old));
                if !exists {
                    ChangeReason::Unavailable
                } else if active_requirements(&active, &package)
                    .iter()
                    .any(|c| !matches(*c, old))
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
    Ok(Resolution { selected, changes })
}