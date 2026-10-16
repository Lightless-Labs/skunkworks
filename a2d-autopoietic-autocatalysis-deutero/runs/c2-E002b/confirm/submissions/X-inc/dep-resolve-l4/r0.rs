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

fn canonical_constraints(items: &[Constraint]) -> Vec<Constraint> {
    let mut out = items.to_vec();
    out.sort_by_key(|c| constraint_rank(*c));
    out.dedup();
    out
}

fn constraints_text(items: &[Constraint]) -> String {
    canonical_constraints(items)
        .iter()
        .map(|c| constraint_text(*c))
        .collect::<Vec<_>>()
        .join(",")
}

fn encode(s: &str) -> String {
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

fn evidence_sorted(items: &[Evidence]) -> Vec<Evidence> {
    let mut out = items.to_vec();
    out.sort_by(|a, b| {
        constraint_rank(a.constraint)
            .cmp(&constraint_rank(b.constraint))
            .then_with(|| a.path.cmp(&b.path))
    });
    out
}

fn evidence_lines(items: &[Evidence]) -> String {
    evidence_sorted(items)
        .iter()
        .map(|e| {
            format!(
                "\n  C:{} via {}",
                constraint_text(e.constraint),
                e.path.join(" -> ")
            )
        })
        .collect::<String>()
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
            constraints_text(requirements),
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
            constraints_text(requirements),
            evidence_lines(evidence)
        ),
        ResolveError::MissingFeature {
            package,
            selected,
            feature,
            path,
        } => format!(
            "missing-feature|package={}|selected={}|feature={}\
\n  F:{} via {}",
            encode(package),
            version_text(*selected),
            encode(feature),
            encode(feature),
            path.join(" -> ")
        ),
        ResolveError::Limit { attempts } => format!("limit|attempts={}", attempts),
    }
}

fn valid_name(s: &str) -> bool {
    let b = s.as_bytes();
    if b.is_empty() || b.len() > 24 || !b[0].is_ascii_lowercase() || *b.last().unwrap() == b'-' {
        return false;
    }
    b.iter()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
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
    match platform {
        Platform::Any => true,
        Platform::Linux => target == Target::Linux,
        Platform::Mac => target == Target::Mac,
        Platform::Windows => target == Target::Windows,
    }
}

fn feature_names(req: &[String]) -> BTreeSet<String> {
    req.iter().cloned().collect()
}

fn feature_map(release: &Release) -> BTreeMap<String, Vec<String>> {
    release
        .features
        .iter()
        .map(|f| (f.name.clone(), f.enables.clone()))
        .collect()
}

#[derive(Clone)]
struct Origin {
    constraint: Constraint,
    path: Vec<String>,
}

#[derive(Clone, Default)]
struct Closure {
    constraints: BTreeMap<String, Vec<Constraint>>,
    origins: BTreeMap<String, Vec<Origin>>,
    requests: BTreeMap<String, BTreeSet<String>>,
    activated: BTreeMap<String, BTreeSet<String>>,
    active_deps: BTreeMap<String, BTreeSet<usize>>,
    paths: BTreeMap<String, Vec<String>>,
}

fn closure(
    catalog: &BTreeMap<String, &Package>,
    roots: &[Requirement],
    selected: &BTreeMap<String, Version>,
    target: Target,
) -> Closure {
    let mut result = Closure::default();
    let mut requests_queue: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut expanded_features: BTreeSet<(String, String)> = BTreeSet::new();
    let mut seen_origins: BTreeSet<(String, Constraint, Vec<String>)> = BTreeSet::new();

    for (i, root) in roots.iter().enumerate() {
        let mut fs = feature_names(&root.features);
        let ftext = fs.iter().cloned().collect::<Vec<_>>().join(",");
        let token = format!(
            "root[{}]:{}@{}+{{{}}}",
            i,
            root.package,
            constraint_text(root.constraint),
            ftext
        );
        let path = vec![token];
        result
            .constraints
            .entry(root.package.clone())
            .or_default()
            .push(root.constraint);
        result
            .origins
            .entry(root.package.clone())
            .or_default()
            .push(Origin {
                constraint: root.constraint,
                path: path.clone(),
            });
        seen_origins.insert((root.package.clone(), root.constraint, path));
        requests_queue
            .entry(root.package.clone())
            .or_default()
            .append(&mut fs);
    }

    loop {
        let mut changed = false;

        let packages: Vec<String> = requests_queue.keys().cloned().collect();
        for package in packages {
            let Some(version) = selected.get(&package).copied() else {
                continue;
            };
            let Some(pkg) = catalog.get(&package).copied() else {
                continue;
            };
            let Some(release) = pkg.releases.iter().find(|r| r.version == version) else {
                continue;
            };

            let mut todo = requests_queue.get(&package).cloned().unwrap_or_default();
            let declarations = feature_map(release);
            loop {
                let next = todo.iter().find(|f| {
                    !expanded_features.contains(&(package.clone(), (*f).clone()))
                }).cloned();
                let Some(feature) = next else { break };
                expanded_features.insert((package.clone(), feature.clone()));
                result
                    .activated
                    .entry(package.clone())
                    .or_default()
                    .insert(feature.clone());
                if let Some(enables) = declarations.get(&feature) {
                    for enabled in enables {
                        if requests_queue
                            .entry(package.clone())
                            .or_default()
                            .insert(enabled.clone())
                        {
                            todo.insert(enabled.clone());
                            changed = true;
                        }
                    }
                }
            }
            for f in requests_queue.get(&package).into_iter().flatten() {
                result
                    .activated
                    .entry(package.clone())
                    .or_default()
                    .insert(f.clone());
            }
        }

        for (source, version) in selected {
            let Some(pkg) = catalog.get(source).copied() else {
                continue;
            };
            let Some(release) = pkg.releases.iter().find(|r| r.version == *version) else {
                continue;
            };
            let activated = result.activated.get(source).cloned().unwrap_or_default();
            for (j, dep) in release.deps.iter().enumerate() {
                if !platform_matches(dep.platform, target)
                    || dep.gate.as_ref().is_some_and(|g| !activated.contains(g))
                {
                    continue;
                }

                if result
                    .active_deps
                    .entry(source.clone())
                    .or_default()
                    .insert(j)
                {
                    changed = true;
                }

                result
                    .constraints
                    .entry(dep.package.clone())
                    .or_default()
                    .push(dep.constraint);

                let src_path = result
                    .paths
                    .get(source)
                    .cloned()
                    .unwrap_or_default();
                let mut fs = feature_names(&dep.features);
                let ftext = fs.iter().cloned().collect::<Vec<_>>().join(",");
                let gate = dep.gate.as_deref().unwrap_or("*");
                let platform = match dep.platform {
                    Platform::Any => "any",
                    Platform::Linux => "linux",
                    Platform::Mac => "mac",
                    Platform::Windows => "windows",
                };
                let token = format!(
                    "{}@{}/deps[{}]:{}@{}+{{{}}};gate={};platform={}",
                    source,
                    version_text(*version),
                    j,
                    dep.package,
                    constraint_text(dep.constraint),
                    ftext,
                    gate,
                    platform
                );
                let mut path = src_path;
                path.push(token);
                let key = (dep.package.clone(), dep.constraint, path.clone());
                if seen_origins.insert(key) {
                    result
                        .origins
                        .entry(dep.package.clone())
                        .or_default()
                        .push(Origin {
                            constraint: dep.constraint,
                            path: path.clone(),
                        });
                }
                if !path.is_empty() {
                    let replace = result
                        .paths
                        .get(&dep.package)
                        .map(|old| (path.len(), &path) < (old.len(), old))
                        .unwrap_or(true);
                    if replace {
                        result.paths.insert(dep.package.clone(), path);
                    }
                }
                for f in fs.iter() {
                    if requests_queue
                        .entry(dep.package.clone())
                        .or_default()
                        .insert(f.clone())
                    {
                        changed = true;
                    }
                }
            }
        }

        let all_selected_with_roots: BTreeSet<String> =
            result.constraints.keys().cloned().collect();
        for package in all_selected_with_roots {
            if !result.paths.contains_key(&package) {
                if let Some(origin) = result
                    .origins
                    .get(&package)
                    .and_then(|v| v.iter().min_by(|a, b| {
                        (a.path.len(), &a.path).cmp(&(b.path.len(), &b.path))
                    }))
                {
                    result.paths.insert(package, origin.path.clone());
                }
            }
        }

        if !changed {
            break;
        }
    }

    for cs in result.constraints.values_mut() {
        *cs = canonical_constraints(cs);
    }
    result
}

fn active_evidence(c: &Closure, package: &str) -> Vec<Evidence> {
    let mut items = c
        .origins
        .get(package)
        .into_iter()
        .flatten()
        .map(|o| Evidence {
            constraint: o.constraint,
            path: o.path.clone(),
        })
        .collect::<Vec<_>>();
    items.sort_by(|a, b| {
        constraint_rank(a.constraint)
            .cmp(&constraint_rank(b.constraint))
            .then_with(|| a.path.cmp(&b.path))
    });
    items
}

struct Search<'a> {
    catalog: &'a BTreeMap<String, &'a Package>,
    roots: &'a [Requirement],
    locks: &'a BTreeMap<String, Version>,
    target: Target,
    max_attempts: usize,
    attempts: usize,
    best: Option<(usize, BTreeMap<String, Version>)>,
    first_error: Option<ResolveError>,
}

impl<'a> Search<'a> {
    fn inspect(&mut self, selected: &mut BTreeMap<String, Version>) -> Result<(), ResolveError> {
        let state = closure(self.catalog, self.roots, selected, self.target);

        if let Some(package) = state
            .constraints
            .keys()
            .filter(|p| !self.catalog.contains_key(*p))
            .next()
            .cloned()
        {
            return Err(ResolveError::Missing {
                evidence: active_evidence(&state, &package),
                package,
            });
        }

        for (package, version) in selected.iter() {
            if let Some(cs) = state.constraints.get(package) {
                if !cs.iter().all(|c| matches(*c, *version)) {
                    return Err(ResolveError::Conflict {
                        package: package.clone(),
                        selected: *version,
                        requirements: cs.clone(),
                        evidence: active_evidence(&state, package),
                    });
                }
            }
        }

        for (package, version) in selected.iter() {
            let Some(pkg) = self.catalog.get(package).copied() else {
                continue;
            };
            let Some(release) = pkg.releases.iter().find(|r| r.version == *version) else {
                continue;
            };
            let activated = state.activated.get(package).cloned().unwrap_or_default();
            let declarations: BTreeSet<String> =
                release.features.iter().map(|f| f.name.clone()).collect();
            for feature in activated {
                if !declarations.contains(&feature) {
                    return Err(ResolveError::MissingFeature {
                        package: package.clone(),
                        selected: *version,
                        feature,
                        path: state.paths.get(package).cloned().unwrap_or_default(),
                    });
                }
            }
        }

        for (package, constraints) in &state.constraints {
            if selected.contains_key(package) {
                continue;
            }
            let pkg = self.catalog.get(package).copied().unwrap();
            let mut releases: Vec<Version> = pkg
                .releases
                .iter()
                .map(|r| r.version)
                .filter(|v| constraints.iter().all(|c| matches(*c, *v)))
                .collect();
            if releases.is_empty() {
                return Err(ResolveError::NoMatch {
                    package: package.clone(),
                    requirements: constraints.clone(),
                    evidence: active_evidence(&state, package),
                });
            }
        }

        let next = state
            .constraints
            .keys()
            .find(|p| !selected.contains_key(*p))
            .cloned();

        let Some(package) = next else {
            let cost = self
                .locks
                .iter()
                .filter(|(p, v)| selected.get(*p) != Some(*v))
                .count();
            let should_replace = match &self.best {
                None => true,
                Some((best_cost, best_solution)) if cost < *best_cost => true,
                Some((best_cost, best_solution)) if cost == *best_cost => {
                    let names: BTreeSet<String> = self.catalog.keys().cloned().collect();
                    let mut better = false;
                    for name in names {
                        match (selected.get(&name), best_solution.get(&name)) {
                            (Some(a), Some(b)) if a != b => {
                                better = a > b;
                                break;
                            }
                            (Some(_), None) => {
                                better = true;
                                break;
                            }
                            (None, Some(_)) => break,
                            _ => {}
                        }
                    }
                    better
                }
                _ => false,
            };
            if should_replace {
                self.best = Some((cost, selected.clone()));
            }
            return Ok(());
        };

        let pkg = self.catalog.get(&package).copied().unwrap();
        let constraints = state.constraints.get(&package).cloned().unwrap_or_default();
        let mut candidates: Vec<Version> = pkg
            .releases
            .iter()
            .map(|r| r.version)
            .filter(|v| constraints.iter().all(|c| matches(*c, *v)))
            .collect();
        candidates.sort_by(|a, b| b.cmp(a));
        if let Some(lock) = self.locks.get(&package) {
            if let Some(pos) = candidates.iter().position(|v| v == lock) {
                let v = candidates.remove(pos);
                candidates.insert(0, v);
            }
        }

        let mut first_local_error = None;
        for version in candidates {
            if self.attempts == self.max_attempts {
                return Err(ResolveError::Limit {
                    attempts: self.max_attempts,
                });
            }
            self.attempts += 1;
            selected.insert(package.clone(), version);
            match self.inspect(selected) {
                Ok(()) => {}
                Err(ResolveError::Limit { attempts }) => {
                    selected.remove(&package);
                    return Err(ResolveError::Limit { attempts });
                }
                Err(error) => {
                    if first_local_error.is_none() {
                        first_local_error = Some(error);
                    }
                }
            }
            selected.remove(&package);
        }
        if let Some(error) = first_local_error {
            if self.first_error.is_none() {
                self.first_error = Some(error);
            }
        }
        Ok(())
    }
}

fn validate(catalog: &[Package], roots: &[Requirement], locks: &[Lock]) -> Result<(), ResolveError> {
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
                check_name(
                    format!("catalog[{}].release[{}].dep[{}]", i, j, k),
                    &dep.package,
                )?;
                if let Some(gate) = &dep.gate {
                    check_name(
                        format!("catalog[{}].release[{}].dep[{}].gate", i, j, k),
                        gate,
                    )?;
                }
                for (f, feature) in dep.features.iter().enumerate() {
                    check_name(
                        format!(
                            "catalog[{}].release[{}].dep[{}].feature[{}]",
                            i, j, k, f
                        ),
                        feature,
                    )?;
                }
            }
            for (f, feature) in release.features.iter().enumerate() {
                check_name(
                    format!("catalog[{}].release[{}].feature[{}]", i, j, f),
                    &feature.name,
                )?;
                for (e, enabled) in feature.enables.iter().enumerate() {
                    check_name(
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
    let mut selected = BTreeMap::new();
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

    match search.inspect(&mut selected) {
        Err(error) => return Err(error),
        Ok(()) => {}
    }

    let Some((_, final_selected)) = search.best else {
        return Err(search.first_error.unwrap_or_else(|| {
            let state = closure(&catalog_map, roots, &BTreeMap::new(), target);
            if let Some(package) = state.constraints.keys().next() {
                ResolveError::NoMatch {
                    package: package.clone(),
                    requirements: state
                        .constraints
                        .get(package)
                        .cloned()
                        .unwrap_or_default(),
                    evidence: active_evidence(&state, package),
                }
            } else {
                ResolveError::Limit {
                    attempts: search.attempts,
                }
            }
        }));
    };

    let final_state = closure(&catalog_map, roots, &final_selected, target);
    let selected_out = final_selected
        .iter()
        .map(|(package, version)| Selection {
            package: package.clone(),
            version: *version,
        })
        .collect::<Vec<_>>();

    let mut change_names = BTreeSet::new();
    change_names.extend(lock_map.keys().cloned());
    change_names.extend(final_selected.keys().cloned());
    let mut changes = Vec::new();
    for package in change_names {
        let before = lock_map.get(&package).copied();
        let after = final_selected.get(&package).copied();
        if before == after {
            continue;
        }
        let reason = match (before, after) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old), Some(_)) => {
                let exists = catalog_map
                    .get(&package)
                    .is_some_and(|p| p.releases.iter().any(|r| r.version == old));
                if !exists {
                    ChangeReason::Unavailable
                } else if !final_state
                    .constraints
                    .get(&package)
                    .map(|cs| cs.iter().all(|c| matches(*c, old)))
                    .unwrap_or(true)
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

    Ok(Resolution {
        selected: selected_out,
        changes,
    })
}