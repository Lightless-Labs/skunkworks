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

fn name_error(site: String, name: &str) -> Option<ResolveError> {
    (!valid_name(name)).then(|| ResolveError::InvalidName {
        site,
        name: name.to_string(),
    })
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

fn canonical_constraints(values: &[Constraint]) -> Vec<Constraint> {
    let mut result = values.to_vec();
    result.sort_by_key(|c| constraint_rank(*c));
    result.dedup();
    result
}

fn constraints_text(values: &[Constraint]) -> String {
    canonical_constraints(values)
        .iter()
        .map(|c| constraint_text(*c))
        .collect::<Vec<_>>()
        .join(",")
}

fn permits(c: Constraint, v: Version) -> bool {
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

fn encode(value: &str) -> String {
    let mut out = String::new();
    for b in value.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(*b, b'.' | b'_' | b'-') {
            out.push(*b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

fn evidence_sorted(values: &[Evidence]) -> Vec<Evidence> {
    let mut result = values.to_vec();
    result.sort_by(|a, b| {
        constraint_rank(a.constraint)
            .cmp(&constraint_rank(b.constraint))
            .then_with(|| a.path.cmp(&b.path))
    });
    result
}

fn append_evidence(out: &mut String, evidence: &[Evidence]) {
    for e in evidence_sorted(evidence) {
        out.push_str("\n  ");
        out.push_str(&constraint_text(e.constraint));
        out.push_str(" via ");
        out.push_str(&e.path.join(" -> "));
    }
}

/// Formats a resolver error as a deterministic, machine-readable string.
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
            let mut out = format!(
                "no-match|package={}|constraints={}",
                encode(package),
                constraints_text(requirements)
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
            let mut out = format!(
                "conflict|package={}|selected={}|constraints={}",
                encode(package),
                version_text(*selected),
                constraints_text(requirements)
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

#[derive(Clone)]
struct Origin {
    constraint: Constraint,
    path: Vec<String>,
}

struct Closure {
    constraints: BTreeMap<String, Vec<Constraint>>,
    features: BTreeMap<String, BTreeSet<String>>,
    origins: BTreeMap<String, Vec<Origin>>,
    paths: BTreeMap<String, Vec<String>>,
    missing_features: Vec<(String, Version, String, Vec<String>)>,
}

fn platform_matches(platform: Platform, target: Target) -> bool {
    matches!(platform, Platform::Any)
        || matches!(
            (platform, target),
            (Platform::Linux, Target::Linux)
                | (Platform::Mac, Target::Mac)
                | (Platform::Windows, Target::Windows)
        )
}

fn sorted_features(features: &[String]) -> String {
    let values: BTreeSet<_> = features.iter().cloned().collect();
    values.into_iter().collect::<Vec<_>>().join(",")
}

fn root_token(index: usize, root: &Requirement) -> String {
    format!(
        "root[{}]:{}@{}+{{{}}}",
        index,
        root.package,
        constraint_text(root.constraint),
        sorted_features(&root.features)
    )
}

fn target_text(target: Platform) -> &'static str {
    match target {
        Platform::Any => "any",
        Platform::Linux => "linux",
        Platform::Mac => "mac",
        Platform::Windows => "windows",
    }
}

fn dep_token(source: &str, version: Version, index: usize, dep: &Dependency) -> String {
    format!(
        "{}@{}/deps[{}]:{}@{}+{{{}}};gate={};platform={}",
        source,
        version_text(version),
        index,
        dep.package,
        constraint_text(dep.constraint),
        sorted_features(&dep.features),
        dep.gate.as_deref().unwrap_or("*"),
        target_text(dep.platform)
    )
}

fn better_path(candidate: &[String], current: &[String]) -> bool {
    candidate.len() < current.len() || (candidate.len() == current.len() && candidate < current)
}

fn compute_closure(
    catalog: &BTreeMap<String, &Package>,
    roots: &[Requirement],
    selected: &BTreeMap<String, Version>,
    target: Target,
) -> Closure {
    let mut constraints: BTreeMap<String, Vec<Constraint>> = BTreeMap::new();
    let mut features: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut origins: BTreeMap<String, Vec<Origin>> = BTreeMap::new();
    let mut paths: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for (i, root) in roots.iter().enumerate() {
        constraints
            .entry(root.package.clone())
            .or_default()
            .push(root.constraint);
        features
            .entry(root.package.clone())
            .or_default()
            .extend(root.features.iter().cloned());
        let path = vec![root_token(i, root)];
        origins
            .entry(root.package.clone())
            .or_default()
            .push(Origin {
                constraint: root.constraint,
                path: path.clone(),
            });
        match paths.get(&root.package) {
            Some(old) if !better_path(&path, old) => {}
            _ => {
                paths.insert(root.package.clone(), path);
            }
        }
    }

    loop {
        let mut changed = false;
        for (package, version) in selected {
            let Some(pkg) = catalog.get(package) else {
                continue;
            };
            let Some(release) = pkg.releases.iter().find(|r| r.version == *version) else {
                continue;
            };

            let requested: Vec<String> = features
                .get(package)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect();

            for name in &requested {
                if let Some(feature) = release.features.iter().find(|f| f.name == *name) {
                    for enabled in &feature.enables {
                        if features
                            .entry(package.clone())
                            .or_default()
                            .insert(enabled.clone())
                        {
                            changed = true;
                        }
                    }
                }
            }

            let source_path = paths.get(package).cloned().unwrap_or_default();
            for (j, dep) in release.deps.iter().enumerate() {
                if !platform_matches(dep.platform, target)
                    || dep
                        .gate
                        .as_ref()
                        .is_some_and(|g| !features.get(package).is_some_and(|set| set.contains(g)))
                {
                    continue;
                }
                let token = dep_token(package, *version, j, dep);
                let mut path = source_path.clone();
                path.push(token);

                let list = origins.entry(dep.package.clone()).or_default();
                if !list.iter().any(|o| o.constraint == dep.constraint && o.path == path) {
                    list.push(Origin {
                        constraint: dep.constraint,
                        path: path.clone(),
                    });
                    changed = true;
                }
                let cs = constraints.entry(dep.package.clone()).or_default();
                if !cs.contains(&dep.constraint) {
                    cs.push(dep.constraint);
                    changed = true;
                }
                let fs = features.entry(dep.package.clone()).or_default();
                for f in &dep.features {
                    if fs.insert(f.clone()) {
                        changed = true;
                    }
                }
                match paths.get(&dep.package) {
                    Some(old) if !better_path(&path, old) => {}
                    _ => {
                        paths.insert(dep.package.clone(), path);
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }

    for values in constraints.values_mut() {
        *values = canonical_constraints(values);
    }

    let mut missing_features = Vec::new();
    for (package, version) in selected {
        let Some(pkg) = catalog.get(package) else {
            continue;
        };
        let Some(release) = pkg.releases.iter().find(|r| r.version == *version) else {
            continue;
        };
        let declared: BTreeSet<&str> = release.features.iter().map(|f| f.name.as_str()).collect();
        let path = paths.get(package).cloned().unwrap_or_default();
        for feature in features.get(package).into_iter().flatten() {
            if !declared.contains(feature.as_str()) {
                missing_features.push((
                    package.clone(),
                    *version,
                    feature.clone(),
                    path.clone(),
                ));
            }
        }
    }

    Closure {
        constraints,
        features,
        origins,
        paths,
        missing_features,
    }
}

fn evidence_for(closure: &Closure, package: &str) -> Vec<Evidence> {
    let mut out: Vec<Evidence> = closure
        .origins
        .get(package)
        .into_iter()
        .flatten()
        .map(|o| Evidence {
            constraint: o.constraint,
            path: o.path.clone(),
        })
        .collect();
    out.sort_by(|a, b| {
        constraint_rank(a.constraint)
            .cmp(&constraint_rank(b.constraint))
            .then_with(|| a.path.cmp(&b.path))
    });
    out
}

enum SearchResult {
    Complete,
    Failure(ResolveError),
    Limit,
}

struct Search<'a> {
    catalog: &'a BTreeMap<String, &'a Package>,
    roots: &'a [Requirement],
    locks: &'a BTreeMap<String, Version>,
    target: Target,
    max_attempts: usize,
    attempts: usize,
    best: Option<(usize, Vec<Option<Version>>, BTreeMap<String, Version>, Closure)>,
    first_error: Option<ResolveError>,
}

impl Search<'_> {
    fn inspect(&mut self, selected: &mut BTreeMap<String, Version>) -> SearchResult {
        let closure = compute_closure(self.catalog, self.roots, selected, self.target);

        if let Some(package) = closure
            .constraints
            .keys()
            .filter(|name| !self.catalog.contains_key(*name))
            .next()
            .cloned()
        {
            return SearchResult::Failure(ResolveError::Missing {
                evidence: evidence_for(&closure, &package),
                package,
            });
        }

        if let Some((package, version, feature, path)) =
            closure.missing_features.first().cloned()
        {
            return SearchResult::Failure(ResolveError::MissingFeature {
                package,
                selected: version,
                feature,
                path,
            });
        }

        for (package, version) in selected.iter() {
            let active = closure.constraints.get(package).cloned().unwrap_or_default();
            if active.iter().any(|c| !permits(*c, *version)) {
                return SearchResult::Failure(ResolveError::Conflict {
                    package: package.clone(),
                    selected: *version,
                    requirements: active,
                    evidence: evidence_for(&closure, package),
                });
            }
        }

        for package in closure.constraints.keys() {
            if selected.contains_key(package) {
                continue;
            }
            let constraints = closure.constraints.get(package).cloned().unwrap_or_default();
            let matching = self.catalog[package]
                .releases
                .iter()
                .filter(|r| constraints.iter().all(|c| permits(*c, r.version)))
                .count();
            if matching == 0 {
                return SearchResult::Failure(ResolveError::NoMatch {
                    package: package.clone(),
                    requirements: constraints,
                    evidence: evidence_for(&closure, package),
                });
            }
        }

        let next = closure
            .constraints
            .keys()
            .find(|p| !selected.contains_key(*p))
            .cloned();
        let Some(package) = next else {
            let cost = self
                .locks
                .iter()
                .filter(|(name, version)| selected.get(*name) != Some(*version))
                .count();
            let names: Vec<String> = self.catalog.keys().cloned().collect();
            let vector: Vec<Option<Version>> =
                names.iter().map(|name| selected.get(name).copied()).collect();

            let improve = match &self.best {
                None => true,
                Some((best_cost, best_vector, _, _)) => {
                    cost < *best_cost || (cost == *best_cost && vector > *best_vector)
                }
            };
            if improve {
                self.best = Some((cost, vector, selected.clone(), closure));
            }
            return SearchResult::Complete;
        };

        let constraints = closure.constraints.get(&package).cloned().unwrap_or_default();
        let mut candidates: Vec<Version> = self.catalog[&package]
            .releases
            .iter()
            .filter(|r| constraints.iter().all(|c| permits(*c, r.version)))
            .map(|r| r.version)
            .collect();
        candidates.sort_by(|a, b| b.cmp(a));
        if let Some(locked) = self.locks.get(&package) {
            if let Some(position) = candidates.iter().position(|v| v == locked) {
                let v = candidates.remove(position);
                candidates.insert(0, v);
            }
        }

        let mut first_failure = None;
        for candidate in candidates {
            if self.attempts == self.max_attempts {
                return SearchResult::Limit;
            }
            self.attempts += 1;
            selected.insert(package.clone(), candidate);
            match self.inspect(selected) {
                SearchResult::Complete => {}
                SearchResult::Failure(error) => {
                    if first_failure.is_none() {
                        first_failure = Some(error);
                    }
                }
                SearchResult::Limit => {
                    selected.remove(&package);
                    return SearchResult::Limit;
                }
            }
            selected.remove(&package);
        }

        let error = first_failure.unwrap_or_else(|| ResolveError::NoMatch {
            package: package.clone(),
            requirements: constraints.clone(),
            evidence: evidence_for(&closure, &package),
        });
        if self.first_error.is_none() {
            self.first_error = Some(error.clone());
        }
        SearchResult::Failure(error)
    }
}

fn validate(catalog: &[Package], roots: &[Requirement], locks: &[Lock]) -> Result<(), ResolveError> {
    for (i, root) in roots.iter().enumerate() {
        if let Some(e) = name_error(format!("root[{}]", i), &root.package) {
            return Err(e);
        }
        for (f, feature) in root.features.iter().enumerate() {
            if let Some(e) = name_error(format!("root[{}].feature[{}]", i, f), feature) {
                return Err(e);
            }
        }
    }
    for (i, package) in catalog.iter().enumerate() {
        if let Some(e) = name_error(format!("catalog[{}]", i), &package.name) {
            return Err(e);
        }
        for (j, release) in package.releases.iter().enumerate() {
            for (k, dep) in release.deps.iter().enumerate() {
                let site = format!("catalog[{}].release[{}].dep[{}]", i, j, k);
                if let Some(e) = name_error(site.clone(), &dep.package) {
                    return Err(e);
                }
                if let Some(gate) = &dep.gate {
                    if let Some(e) = name_error(format!("{}.gate", site), gate) {
                        return Err(e);
                    }
                }
                for (f, feature) in dep.features.iter().enumerate() {
                    if let Some(e) =
                        name_error(format!("{}.feature[{}]", site, f), feature)
                    {
                        return Err(e);
                    }
                }
            }
            for (f, feature) in release.features.iter().enumerate() {
                let site = format!("catalog[{}].release[{}].feature[{}]", i, j, f);
                if let Some(e) = name_error(site.clone(), &feature.name) {
                    return Err(e);
                }
                for (eidx, enabled) in feature.enables.iter().enumerate() {
                    if let Some(e) =
                        name_error(format!("{}.enable[{}]", site, eidx), enabled)
                    {
                        return Err(e);
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
            let mut feature_names = BTreeSet::new();
            for feature in &release.features {
                if !feature_names.insert(feature.name.clone()) {
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
        if let Some(e) = name_error(format!("lock[{}]", i), &lock.package) {
            return Err(e);
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
    Ok(())
}

fn make_changes(
    catalog: &BTreeMap<String, &Package>,
    locks: &[Lock],
    selected: &BTreeMap<String, Version>,
    closure: &Closure,
) -> Vec<Change> {
    let before: BTreeMap<String, Version> = locks
        .iter()
        .map(|l| (l.package.clone(), l.version))
        .collect();
    let mut names: BTreeSet<String> = before.keys().cloned().collect();
    names.extend(selected.keys().cloned());

    let mut changes = Vec::new();
    for name in names {
        let b = before.get(&name).copied();
        let a = selected.get(&name).copied();
        if b == a {
            continue;
        }
        let reason = match (b, a) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old), Some(_)) => {
                let exists = catalog
                    .get(&name)
                    .is_some_and(|p| p.releases.iter().any(|r| r.version == old));
                if !exists {
                    ChangeReason::Unavailable
                } else if closure
                    .constraints
                    .get(&name)
                    .is_some_and(|cs| cs.iter().any(|c| !permits(*c, old)))
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
            before: b,
            after: a,
            reason,
        });
    }
    changes
}

/// Resolves packages, features, platform-gated dependencies, and locks.
pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
    locks: &[Lock],
    target: Target,
    max_attempts: usize,
) -> Result<Resolution, ResolveError> {
    validate(catalog, roots, locks)?;

    let catalog_map: BTreeMap<String, &Package> =
        catalog.iter().map(|p| (p.name.clone(), p)).collect();
    let lock_map: BTreeMap<String, Version> = locks
        .iter()
        .map(|l| (l.package.clone(), l.version))
        .collect();
    let mut search = Search {
        catalog: &catalog_map,
        roots,
        locks: &lock_map,
        target,
        max_attempts,
        attempts: 0,
        best: None,
        first_error: None,
    };
    let mut selected = BTreeMap::new();

    if roots.is_empty() {
        let empty_closure = compute_closure(&catalog_map, roots, &selected, target);
        let changes = make_changes(&catalog_map, locks, &selected, &empty_closure);
        return Ok(Resolution {
            selected: Vec::new(),
            changes,
        });
    }

    match search.inspect(&mut selected) {
        SearchResult::Limit => Err(ResolveError::Limit {
            attempts: max_attempts,
        }),
        SearchResult::Failure(error) if search.best.is_none() => {
            Err(search.first_error.unwrap_or(error))
        }
        SearchResult::Failure(_) | SearchResult::Complete => {
            if let Some((_, _, best, closure)) = search.best {
                let selected_vec = best
                    .iter()
                    .map(|(package, version)| Selection {
                        package: package.clone(),
                        version: *version,
                    })
                    .collect();
                let changes = make_changes(&catalog_map, locks, &best, &closure);
                Ok(Resolution {
                    selected: selected_vec,
                    changes,
                })
            } else {
                Err(ResolveError::NoMatch {
                    package: String::new(),
                    requirements: Vec::new(),
                    evidence: Vec::new(),
                })
            }
        }
    }
}

fn v(major: u32, minor: u32, patch: u32) -> Version {
    Version { major, minor, patch }
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

fn lk(package: &str, n: u32) -> Lock {
    Lock {
        package: package.into(),
        version: x(n),
    }
}

fn run(catalog: &[Package], roots: &[Requirement]) -> Result<Vec<Selection>, ResolveError> {
    resolve(catalog, roots, &[], Target::Linux, 100000).map(|r| r.selected)
}

fn locked(
    catalog: &[Package],
    roots: &[Requirement],
    locks: &[Lock],
    budget: usize,
) -> Result<Resolution, ResolveError> {
    resolve(catalog, roots, locks, Target::Linux, budget)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_transitive_selection() {
        let catalog = vec![
            p("engine", vec![r(1, vec![d("util", ge(1))])]),
            p("util", vec![r(1, vec![])]),
        ];
        assert_eq!(
            run(&catalog, &[q("engine", e(1))]).unwrap(),
            vec![sel("engine", 1), sel("util", 1)]
        );
    }

    #[test]
    fn invalid_input_is_reported() {
        let err = run(&[p("Bad", vec![])], &[]);
        assert_eq!(
            err,
            Err(ResolveError::InvalidName {
                site: "catalog[0]".into(),
                name: "Bad".into()
            })
        );
    }

    #[test]
    fn end_to_end_feature_gate_and_changes() {
        let mut release = r(1, vec![]);
        release.features.push(feat("extras", &[]));
        let mut dependency = d("plugin", e(1));
        dependency.gate = Some("extras".into());
        release.deps.push(dependency);
        let catalog = vec![p("app", vec![release]), p("plugin", vec![r(1, vec![])])];

        let result = resolve(
            &catalog,
            &[ask("app", e(1), &["extras"])],
            &[],
            Target::Linux,
            10,
        )
        .unwrap();

        assert_eq!(result.selected, vec![sel("app", 1), sel("plugin", 1)]);
        assert_eq!(
            result.changes,
            vec![
                change("app", None, Some(1), ChangeReason::Added),
                change("plugin", None, Some(1), ChangeReason::Added)
            ]
        );
    }

    #[test]
    fn error_format_encodes_free_text() {
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
    fn unused_helpers_compile() {
        assert_eq!(locked(&[], &[], &[lk("orphan", 1)], 0).unwrap().changes[0].reason, ChangeReason::Removed);
        assert_eq!(Constraint::Exact(v(1, 2, 3)), Constraint::Exact(Version { major: 1, minor: 2, patch: 3 }));
    }
}

fn main() {}