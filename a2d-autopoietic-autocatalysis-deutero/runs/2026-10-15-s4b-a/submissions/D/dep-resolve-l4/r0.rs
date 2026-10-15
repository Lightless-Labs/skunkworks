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

fn constraints_text(cs: &[Constraint]) -> String {
    canonical_constraints(cs)
        .iter()
        .map(|c| constraint_text(*c))
        .collect::<Vec<_>>()
        .join(",")
}

fn satisfies(v: Version, c: Constraint) -> bool {
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

fn percent(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(*b, b'.' | b'_' | b'-') {
            out.push(*b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

fn sorted_evidence(evidence: &[Evidence]) -> Vec<Evidence> {
    let mut out = evidence.to_vec();
    out.sort_by(|a, b| {
        constraint_key(a.constraint)
            .cmp(&constraint_key(b.constraint))
            .then_with(|| a.path.cmp(&b.path))
    });
    out
}

fn append_evidence(line: &mut String, evidence: &[Evidence]) {
    for e in sorted_evidence(evidence) {
        line.push_str("\n  C:");
        line.push_str(&constraint_text(e.constraint));
        line.push_str(" via ");
        line.push_str(&e.path.join(" -> "));
    }
}

pub fn error_text(error: &ResolveError) -> String {
    match error {
        ResolveError::InvalidName { site, name } => {
            format!("invalid-name|site={}|name={}", percent(site), percent(name))
        }
        ResolveError::DuplicatePackage { package } => {
            format!("duplicate-package|package={}", percent(package))
        }
        ResolveError::DuplicateVersion { package, version } => format!(
            "duplicate-version|package={}|version={}",
            percent(package),
            version_text(*version)
        ),
        ResolveError::DuplicateFeature {
            package,
            version,
            feature,
        } => format!(
            "duplicate-feature|package={}|version={}|feature={}",
            percent(package),
            version_text(*version),
            percent(feature)
        ),
        ResolveError::DuplicateLock { package } => {
            format!("duplicate-lock|package={}", percent(package))
        }
        ResolveError::Missing { package, evidence } => {
            let mut s = format!("missing|package={}", percent(package));
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
                percent(package),
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
                percent(package),
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
            percent(package),
            version_text(*selected),
            percent(feature),
            percent(feature),
            path.join(" -> ")
        ),
        ResolveError::Limit { attempts } => format!("limit|attempts={}", attempts),
    }
}

#[derive(Clone)]
struct Active {
    constraints: Vec<Constraint>,
    evidence: Vec<Evidence>,
    requests: BTreeSet<String>,
}

#[derive(Clone)]
struct Closure {
    active: BTreeMap<String, Active>,
    features: BTreeMap<String, BTreeSet<String>>,
    paths: BTreeMap<String, Vec<String>>,
}

fn target_matches(platform: Platform, target: Target) -> bool {
    matches!(
        (platform, target),
        (Platform::Any, _)
            | (Platform::Linux, Target::Linux)
            | (Platform::Mac, Target::Mac)
            | (Platform::Windows, Target::Windows)
    )
}

fn root_token(r: &Requirement) -> String {
    let fs: BTreeSet<_> = r.features.iter().cloned().collect();
    format!(
        "root[?]:{}@{}+{{{}}}",
        r.package,
        constraint_text(r.constraint),
        fs.into_iter().collect::<Vec<_>>().join(",")
    )
}

fn root_token_at(index: usize, r: &Requirement) -> String {
    let fs: BTreeSet<_> = r.features.iter().cloned().collect();
    format!(
        "root[{}]:{}@{}+{{{}}}",
        index,
        r.package,
        constraint_text(r.constraint),
        fs.into_iter().collect::<Vec<_>>().join(",")
    )
}

fn dep_token(source: &str, version: Version, index: usize, d: &Dependency) -> String {
    let fs: BTreeSet<_> = d.features.iter().cloned().collect();
    let gate = d.gate.as_deref().unwrap_or("*");
    let platform = match d.platform {
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
        d.package,
        constraint_text(d.constraint),
        fs.into_iter().collect::<Vec<_>>().join(","),
        gate,
        platform
    )
}

fn path_cmp(a: &[String], b: &[String]) -> std::cmp::Ordering {
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

fn closure(
    catalog: &BTreeMap<String, &Package>,
    roots: &[Requirement],
    selected: &BTreeMap<String, Version>,
    target: Target,
) -> Closure {
    let mut active = BTreeMap::<String, Active>::new();
    for (i, r) in roots.iter().enumerate() {
        let a = active.entry(r.package.clone()).or_insert_with(|| Active {
            constraints: Vec::new(),
            evidence: Vec::new(),
            requests: BTreeSet::new(),
        });
        let path = vec![root_token_at(i, r)];
        a.constraints.push(r.constraint);
        a.evidence.push(Evidence {
            constraint: r.constraint,
            path,
        });
        a.requests.extend(r.features.iter().cloned());
    }

    let mut features = BTreeMap::<String, BTreeSet<String>>::new();
    let mut origins = BTreeMap::<String, BTreeSet<(String, usize)>>::new();
    loop {
        let mut changed = false;
        for (name, version) in selected {
            let Some(pkg) = catalog.get(name) else {
                continue;
            };
            let Some(release) = pkg.releases.iter().find(|r| r.version == *version) else {
                continue;
            };
            let requests = active
                .get(name)
                .map(|a| a.requests.clone())
                .unwrap_or_default();
            let fs = features.entry(name.clone()).or_default();
            for f in requests {
                changed |= fs.insert(f);
            }
            let current: Vec<String> = fs.iter().cloned().collect();
            for f in current {
                if let Some(decl) = release.features.iter().find(|d| d.name == f) {
                    for enabled in &decl.enables {
                        changed |= fs.insert(enabled.clone());
                    }
                }
            }
            let source_features = fs.clone();
            for (idx, dep) in release.deps.iter().enumerate() {
                if !target_matches(dep.platform, target)
                    || dep.gate.as_ref().is_some_and(|g| !source_features.contains(g))
                {
                    continue;
                }
                let origin = (name.clone(), idx);
                if !origins.entry(dep.package.clone()).or_default().insert(origin) {
                    continue;
                }
                changed = true;
                let a = active.entry(dep.package.clone()).or_insert_with(|| Active {
                    constraints: Vec::new(),
                    evidence: Vec::new(),
                    requests: BTreeSet::new(),
                });
                a.constraints.push(dep.constraint);
                a.requests.extend(dep.features.iter().cloned());
            }
        }
        if !changed {
            break;
        }
    }

    let mut paths = BTreeMap::<String, Vec<String>>::new();
    let mut root_paths = BTreeMap::<String, Vec<Vec<String>>>::new();
    for (i, r) in roots.iter().enumerate() {
        root_paths
            .entry(r.package.clone())
            .or_default()
            .push(vec![root_token_at(i, r)]);
    }
    let mut edges = Vec::<(String, String, String)>::new();
    for (source, version) in selected {
        let Some(pkg) = catalog.get(source) else {
            continue;
        };
        let Some(release) = pkg.releases.iter().find(|r| r.version == *version) else {
            continue;
        };
        let source_features = features.get(source).cloned().unwrap_or_default();
        for (idx, dep) in release.deps.iter().enumerate() {
            if target_matches(dep.platform, target)
                && dep.gate.as_ref().is_none_or(|g| source_features.contains(g))
                && origins
                    .get(&dep.package)
                    .is_some_and(|o| o.contains(&(source.clone(), idx)))
            {
                edges.push((
                    source.clone(),
                    dep.package.clone(),
                    dep_token(source, *version, idx, dep),
                ));
            }
        }
    }
    for (name, paths_for_root) in root_paths {
        for path in paths_for_root {
            if paths
                .get(&name)
                .is_none_or(|old| path_cmp(&path, old).is_lt())
            {
                paths.insert(name.clone(), path);
            }
        }
    }
    loop {
        let mut changed = false;
        for (source, dest, token) in &edges {
            if let Some(source_path) = paths.get(source).cloned() {
                let mut candidate = source_path;
                candidate.push(token.clone());
                if paths
                    .get(dest)
                    .is_none_or(|old| path_cmp(&candidate, old).is_lt())
                {
                    paths.insert(dest.clone(), candidate);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }

    for a in active.values_mut() {
        a.constraints = canonical_constraints(&a.constraints);
        a.evidence = sorted_evidence(&a.evidence);
    }
    for (dest, origins_for_dest) in &origins {
        if let Some(a) = active.get_mut(dest) {
            for (source, idx) in origins_for_dest {
                let Some(version) = selected.get(source) else {
                    continue;
                };
                let Some(pkg) = catalog.get(source) else {
                    continue;
                };
                let Some(release) = pkg.releases.iter().find(|r| r.version == *version) else {
                    continue;
                };
                let Some(dep) = release.deps.get(*idx) else {
                    continue;
                };
                let mut path = paths.get(source).cloned().unwrap_or_default();
                path.push(dep_token(source, *version, *idx, dep));
                a.evidence.push(Evidence {
                    constraint: dep.constraint,
                    path,
                });
            }
            a.evidence = sorted_evidence(&a.evidence);
        }
    }
    Closure {
        active,
        features,
        paths,
    }
}

fn dynamic_error(
    catalog: &BTreeMap<String, &Package>,
    roots: &[Requirement],
    selected: &BTreeMap<String, Version>,
    target: Target,
) -> Result<Closure, ResolveError> {
    let c = closure(catalog, roots, selected, target);

    for name in c.active.keys() {
        if !catalog.contains_key(name) {
            return Err(ResolveError::Missing {
                package: name.clone(),
                evidence: c.active[name].evidence.clone(),
            });
        }
    }

    for (name, version) in selected {
        let Some(a) = c.active.get(name) else {
            continue;
        };
        let pkg = catalog[name];
        let release = pkg
            .releases
            .iter()
            .find(|r| r.version == *version)
            .expect("selected versions come from the catalog");
        for feature in c.features.get(name).into_iter().flatten() {
            if !release.features.iter().any(|f| f.name == *feature) {
                return Err(ResolveError::MissingFeature {
                    package: name.clone(),
                    selected: *version,
                    feature: feature.clone(),
                    path: c.paths.get(name).cloned().unwrap_or_default(),
                });
            }
        }
        if a.constraints.iter().any(|x| !satisfies(*version, *x)) {
            return Err(ResolveError::Conflict {
                package: name.clone(),
                selected: *version,
                requirements: a.constraints.clone(),
                evidence: a.evidence.clone(),
            });
        }
    }

    for (name, a) in &c.active {
        if selected.contains_key(name) {
            continue;
        }
        let pkg = catalog[name];
        if !pkg
            .releases
            .iter()
            .any(|r| a.constraints.iter().all(|x| satisfies(r.version, *x)))
        {
            return Err(ResolveError::NoMatch {
                package: name.clone(),
                requirements: a.constraints.clone(),
                evidence: a.evidence.clone(),
            });
        }
    }
    Ok(c)
}

#[derive(Clone)]
struct SearchResult {
    selected: BTreeMap<String, Version>,
    closure: Closure,
}

struct SearchState<'a> {
    catalog: &'a BTreeMap<String, &'a Package>,
    roots: &'a [Requirement],
    locks: &'a [Lock],
    target: Target,
    budget: usize,
    attempts: usize,
    first_error: Option<ResolveError>,
    best: Option<(usize, Vec<Option<Version>>, SearchResult)>,
    catalog_names: Vec<String>,
}

fn lock_cost(selected: &BTreeMap<String, Version>, locks: &[Lock]) -> usize {
    locks
        .iter()
        .filter(|l| selected.get(&l.package) != Some(&l.version))
        .count()
}

fn vector_for(
    selected: &BTreeMap<String, Version>,
    names: &[String],
) -> Vec<Option<Version>> {
    names.iter().map(|n| selected.get(n).copied()).collect()
}

fn search(state: &mut SearchState<'_>, selected: &mut BTreeMap<String, Version>) -> bool {
    let c = match dynamic_error(state.catalog, state.roots, selected, state.target) {
        Ok(c) => c,
        Err(e) => {
            if let ResolveError::Limit { .. } = e {
                state.first_error = Some(e);
                return true;
            }
            if state.first_error.is_none() {
                state.first_error = Some(e);
            }
            return false;
        }
    };

    let next = c
        .active
        .keys()
        .find(|name| !selected.contains_key(*name))
        .cloned();
    let Some(name) = next else {
        let result = SearchResult {
            selected: selected.clone(),
            closure: c,
        };
        let cost = lock_cost(selected, state.locks);
        let vector = vector_for(selected, &state.catalog_names);
        let improve = state.best.as_ref().is_none_or(|(old_cost, old_vec, _)| {
            cost < *old_cost || (cost == *old_cost && vector > *old_vec)
        });
        if improve {
            state.best = Some((cost, vector, result));
        }
        return false;
    };

    let active = &c.active[&name];
    let pkg = state.catalog[&name];
    let mut candidates: Vec<_> = pkg
        .releases
        .iter()
        .filter(|r| {
            active
                .constraints
                .iter()
                .all(|constraint| satisfies(r.version, *constraint))
        })
        .map(|r| r.version)
        .collect();
    candidates.sort_by(|a, b| b.cmp(a));
    if let Some(lock) = state.locks.iter().find(|l| l.package == name) {
        if let Some(pos) = candidates.iter().position(|v| *v == lock.version) {
            let v = candidates.remove(pos);
            candidates.insert(0, v);
        }
    }

    for version in candidates {
        if state.attempts == state.budget {
            state.first_error = Some(ResolveError::Limit {
                attempts: state.budget,
            });
            return true;
        }
        state.attempts += 1;
        selected.insert(name.clone(), version);
        let abort = search(state, selected);
        selected.remove(&name);
        if abort {
            return true;
        }
    }
    false
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
        validate_name(format!("root[{}]", i), &root.package)?;
        for (f, feature) in root.features.iter().enumerate() {
            validate_name(format!("root[{}].feature[{}]", i, f), feature)?;
        }
    }
    for (i, package) in catalog.iter().enumerate() {
        validate_name(format!("catalog[{}]", i), &package.name)?;
        for (j, release) in package.releases.iter().enumerate() {
            for (k, dep) in release.deps.iter().enumerate() {
                validate_name(
                    format!("catalog[{}].release[{}].dep[{}]", i, j, k),
                    &dep.package,
                )?;
                if let Some(gate) = &dep.gate {
                    validate_name(
                        format!("catalog[{}].release[{}].dep[{}].gate", i, j, k),
                        gate,
                    )?;
                }
                for (f, feature) in dep.features.iter().enumerate() {
                    validate_name(
                        format!("catalog[{}].release[{}].dep[{}].feature[{}]", i, j, k, f),
                        feature,
                    )?;
                }
            }
            for (f, feature) in release.features.iter().enumerate() {
                validate_name(
                    format!("catalog[{}].release[{}].feature[{}]", i, j, f),
                    &feature.name,
                )?;
                for (e, enabled) in feature.enables.iter().enumerate() {
                    validate_name(
                        format!(
                            "catalog[{}].release[{}].feature[{}].enable[{}]",
                            i, j, f, e
                        ),
                        enabled,
                    )?;
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
        validate_name(format!("lock[{}]", i), &lock.package)?;
    }
    let mut seen_locks = BTreeSet::new();
    for lock in locks {
        if !seen_locks.insert(lock.package.clone()) {
            return Err(ResolveError::DuplicateLock {
                package: lock.package.clone(),
            });
        }
    }

    let catalog_map: BTreeMap<String, &Package> =
        catalog.iter().map(|p| (p.name.clone(), p)).collect();
    let mut names: Vec<String> = catalog.iter().map(|p| p.name.clone()).collect();
    names.sort();

    let mut state = SearchState {
        catalog: &catalog_map,
        roots,
        locks,
        target,
        budget: max_attempts,
        attempts: 0,
        first_error: None,
        best: None,
        catalog_names: names,
    };
    let mut selected = BTreeMap::new();
    let aborted = search(&mut state, &mut selected);
    if aborted {
        return Err(state.first_error.unwrap_or(ResolveError::Limit {
            attempts: state.budget,
        }));
    }
    let Some((_, _, result)) = state.best else {
        return Err(state.first_error.unwrap_or(ResolveError::NoMatch {
            package: String::new(),
            requirements: Vec::new(),
            evidence: Vec::new(),
        }));
    };

    let selected_out: Vec<Selection> = result
        .selected
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
    let after = &result.selected;
    let mut all_names: BTreeSet<String> = before.keys().cloned().collect();
    all_names.extend(after.keys().cloned());
    let mut changes = Vec::new();
    for package in all_names {
        let b = before.get(&package).copied();
        let a = after.get(&package).copied();
        if b == a {
            continue;
        }
        let reason = match (b, a) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old), Some(_)) => {
                let exists = catalog_map
                    .get(&package)
                    .is_some_and(|p| p.releases.iter().any(|r| r.version == old));
                if !exists {
                    ChangeReason::Unavailable
                } else if result
                    .closure
                    .active
                    .get(&package)
                    .is_some_and(|active| {
                        active.constraints.iter().any(|c| !satisfies(old, *c))
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
            before: b,
            after: a,
            reason,
        });
    }

    Ok(Resolution {
        selected: selected_out,
        changes,
    })
}