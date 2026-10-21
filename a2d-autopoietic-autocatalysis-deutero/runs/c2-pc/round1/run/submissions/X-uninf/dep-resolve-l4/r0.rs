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
    InvalidName { site: String, name: String },
    DuplicatePackage { package: String },
    DuplicateVersion { package: String, version: Version },
    DuplicateFeature {
        package: String,
        version: Version,
        feature: String,
    },
    DuplicateLock { package: String },
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
    Limit { attempts: usize },
}

fn valid_name(name: &str) -> bool {
    let b = name.as_bytes();
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

fn encoded(s: &str) -> String {
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

fn sorted_evidence(e: &[Evidence]) -> Vec<Evidence> {
    let mut out = e.to_vec();
    out.sort_by(|a, b| {
        a.constraint
            .cmp(&b.constraint)
            .then_with(|| a.path.cmp(&b.path))
    });
    out
}

fn evidence_lines(e: &[Evidence]) -> String {
    let mut out = String::new();
    for item in sorted_evidence(e) {
        out.push_str("\n  C:");
        out.push_str(&constraint_text(item.constraint));
        out.push_str(" via ");
        out.push_str(&item.path.join(" -> "));
    }
    out
}

pub fn error_text(error: &ResolveError) -> String {
    match error {
        ResolveError::InvalidName { site, name } => {
            format!("invalid-name|site={}|name={}", encoded(site), encoded(name))
        }
        ResolveError::DuplicatePackage { package } => {
            format!("duplicate-package|package={}", encoded(package))
        }
        ResolveError::DuplicateVersion { package, version } => format!(
            "duplicate-version|package={}|version={}",
            encoded(package),
            version_text(*version)
        ),
        ResolveError::DuplicateFeature {
            package,
            version,
            feature,
        } => format!(
            "duplicate-feature|package={}|version={}|feature={}",
            encoded(package),
            version_text(*version),
            encoded(feature)
        ),
        ResolveError::DuplicateLock { package } => {
            format!("duplicate-lock|package={}", encoded(package))
        }
        ResolveError::Missing { package, evidence } => {
            format!("missing|package={}{}", encoded(package), evidence_lines(evidence))
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
            format!(
                "no-match|package={}|constraints={}{}",
                encoded(package),
                cs,
                evidence_lines(evidence)
            )
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
            format!(
                "conflict|package={}|selected={}|constraints={}{}",
                encoded(package),
                version_text(*selected),
                cs,
                evidence_lines(evidence)
            )
        }
        ResolveError::MissingFeature {
            package,
            selected,
            feature,
            path,
        } => format!(
            "missing-feature|package={}|selected={}|feature={}\n  F:{} via {}",
            encoded(package),
            version_text(*selected),
            encoded(feature),
            encoded(feature),
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

#[derive(Clone)]
struct Origin {
    constraint: Constraint,
    source: Option<(String, usize)>,
    root_index: Option<usize>,
}

#[derive(Clone, Default)]
struct Closure {
    origins: std::collections::BTreeMap<String, Vec<Origin>>,
    requests: std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    active_features: std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    paths: std::collections::BTreeMap<String, Vec<String>>,
    active_edges: std::collections::BTreeSet<(String, usize)>,
}

fn platform_name(p: Platform) -> &'static str {
    match p {
        Platform::Any => "any",
        Platform::Linux => "linux",
        Platform::Mac => "mac",
        Platform::Windows => "windows",
    }
}

fn platform_active(p: Platform, t: Target) -> bool {
    matches!(p, Platform::Any)
        || matches!(
            (p, t),
            (Platform::Linux, Target::Linux)
                | (Platform::Mac, Target::Mac)
                | (Platform::Windows, Target::Windows)
        )
}

fn release_index<'a>(
    catalog: &'a [Package],
    names: &std::collections::BTreeMap<String, usize>,
    selected: &std::collections::BTreeMap<String, Version>,
    package: &str,
) -> Option<&'a Release> {
    let pi = *names.get(package)?;
    let v = *selected.get(package)?;
    catalog[pi].releases.iter().find(|r| r.version == v)
}

fn root_token(i: usize, r: &Requirement) -> String {
    let mut fs = r.features.clone();
    fs.sort();
    fs.dedup();
    format!(
        "root[{}]:{}@{}+{{{}}}",
        i,
        r.package,
        constraint_text(r.constraint),
        fs.join(",")
    )
}

fn dep_token(source: &str, version: Version, j: usize, d: &Dependency) -> String {
    let mut fs = d.features.clone();
    fs.sort();
    fs.dedup();
    format!(
        "{}@{}/deps[{}]:{}@{}+{{{}}};gate={};platform={}",
        source,
        version_text(version),
        j,
        d.package,
        constraint_text(d.constraint),
        fs.join(","),
        d.gate.as_deref().unwrap_or("*"),
        platform_name(d.platform)
    )
}

fn compute_closure(
    catalog: &[Package],
    names: &std::collections::BTreeMap<String, usize>,
    roots: &[Requirement],
    selected: &std::collections::BTreeMap<String, Version>,
    target: Target,
) -> Closure {
    use std::collections::{BTreeMap, BTreeSet};

    let mut c = Closure::default();

    for (i, root) in roots.iter().enumerate() {
        c.origins
            .entry(root.package.clone())
            .or_default()
            .push(Origin {
                constraint: root.constraint,
                source: None,
                root_index: Some(i),
            });
        for feature in &root.features {
            c.requests
                .entry(root.package.clone())
                .or_default()
                .insert(feature.clone());
        }
    }

    loop {
        let mut changed = false;

        // Feature requests activate declarations and their enables entries.
        for package in selected.keys() {
            let Some(release) = release_index(catalog, names, selected, package) else {
                continue;
            };
            let requested = c.requests.get(package).cloned().unwrap_or_default();
            let active = c.active_features.entry(package.clone()).or_default();
            for feature in requested {
                if active.insert(feature.clone()) {
                    changed = true;
                }
            }

            let active_now = c.active_features.get(package).cloned().unwrap_or_default();
            for feature in active_now {
                if let Some(declaration) = release.features.iter().find(|f| f.name == feature) {
                    for enabled in &declaration.enables {
                        if c.requests
                            .entry(package.clone())
                            .or_default()
                            .insert(enabled.clone())
                        {
                            changed = true;
                        }
                    }
                }
            }
        }

        // Active dependencies add destination constraints and feature requests.
        for package in selected.keys() {
            let Some(release) = release_index(catalog, names, selected, package) else {
                continue;
            };
            let active_features = c.active_features.get(package);
            for (j, dep) in release.deps.iter().enumerate() {
                if !platform_active(dep.platform, target)
                    || dep
                        .gate
                        .as_ref()
                        .is_some_and(|g| !active_features.is_some_and(|fs| fs.contains(g)))
                {
                    continue;
                }
                if c.active_edges.insert((package.clone(), j)) {
                    c.origins
                        .entry(dep.package.clone())
                        .or_default()
                        .push(Origin {
                            constraint: dep.constraint,
                            source: Some((package.clone(), j)),
                            root_index: None,
                        });
                    changed = true;
                }
                for feature in &dep.features {
                    if c.requests
                        .entry(dep.package.clone())
                        .or_default()
                        .insert(feature.clone())
                    {
                        changed = true;
                    }
                }
            }
        }

        if !changed {
            break;
        }
    }

    // Resolve canonical provenance only after the complete active graph is known.
    // Paths are shortest first, then lexicographically smallest.
    let mut best: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut pending: Vec<(String, Vec<String>)> = Vec::new();

    for (i, root) in roots.iter().enumerate() {
        pending.push((root.package.clone(), vec![root_token(i, root)]));
    }
    pending.sort_by(|a, b| {
        (a.1.len(), &a.1, &a.0).cmp(&(b.1.len(), &b.1, &b.0))
    });

    while !pending.is_empty() {
        pending.sort_by(|a, b| {
            (a.1.len(), &a.1, &a.0).cmp(&(b.1.len(), &b.1, &b.0))
        });
        let (package, path) = pending.remove(0);
        let improve = match best.get(&package) {
            None => true,
            Some(old) => (path.len(), &path) < (old.len(), old),
        };
        if !improve {
            continue;
        }
        best.insert(package.clone(), path.clone());

        if let Some(release) = release_index(catalog, names, selected, &package) {
            let version = selected[&package];
            for (j, dep) in release.deps.iter().enumerate() {
                if !c.active_edges.contains(&(package.clone(), j)) {
                    continue;
                }
                let mut next = path.clone();
                next.push(dep_token(&package, version, j, dep));
                pending.push((dep.package.clone(), next));
            }
        }
    }

    c.paths = best;

    // Evidence is formed from each origin using the final canonical source path.
    for origins in c.origins.values_mut() {
        for origin in origins.iter_mut() {
            if let Some((source, edge_index)) = &origin.source {
                let Some(source_path) = c.paths.get(source) else {
                    continue;
                };
                let Some(release) = release_index(catalog, names, selected, source) else {
                    continue;
                };
                let Some(dep) = release.deps.get(*edge_index) else {
                    continue;
                };
                let mut path = source_path.clone();
                path.push(dep_token(source, selected[source], *edge_index, dep));
                origin.root_index = None;
                // Store the computed path via the side table below.
                let _ = path;
            }
        }
        origins.sort_by(|a, b| a.constraint.cmp(&b.constraint));
    }

    c
}

fn origin_evidence(
    origins: &[Origin],
    catalog: &[Package],
    names: &std::collections::BTreeMap<String, usize>,
    roots: &[Requirement],
    selected: &std::collections::BTreeMap<String, Version>,
    paths: &std::collections::BTreeMap<String, Vec<String>>,
) -> Vec<Evidence> {
    let mut out = Vec::new();
    for origin in origins {
        let path = if let Some(i) = origin.root_index {
            vec![root_token(i, &roots[i])]
        } else if let Some((source, edge_index)) = &origin.source {
            let mut path = paths.get(source).cloned().unwrap_or_default();
            if let Some(release) = release_index(catalog, names, selected, source) {
                if let Some(dep) = release.deps.get(*edge_index) {
                    path.push(dep_token(source, selected[source], *edge_index, dep));
                }
            }
            path
        } else {
            Vec::new()
        };
        out.push(Evidence {
            constraint: origin.constraint,
            path,
        });
    }
    out
}

struct Search<'a> {
    catalog: &'a [Package],
    roots: &'a [Requirement],
    target: Target,
    names: std::collections::BTreeMap<String, usize>,
    lock_map: std::collections::BTreeMap<String, Version>,
    attempts: usize,
    budget: usize,
    first_error: Option<ResolveError>,
    best: Option<(usize, std::collections::BTreeMap<String, Version>, Closure)>,
}

impl<'a> Search<'a> {
    fn inspect(
        &mut self,
        selected: &std::collections::BTreeMap<String, Version>,
    ) -> Result<Closure, ResolveError> {
        use std::collections::BTreeSet;

        let closure = compute_closure(
            self.catalog,
            &self.names,
            self.roots,
            selected,
            self.target,
        );

        let mut active: BTreeSet<String> = closure.origins.keys().cloned().collect();
        active.extend(closure.requests.keys().cloned());

        for package in &active {
            if !self.names.contains_key(package) {
                let evidence = origin_evidence(
                    closure.origins.get(package).map(Vec::as_slice).unwrap_or(&[]),
                    self.catalog,
                    &self.names,
                    self.roots,
                    selected,
                    &closure.paths,
                );
                return Err(ResolveError::Missing {
                    package: package.clone(),
                    evidence,
                });
            }
        }

        for (package, version) in selected {
            let features = closure
                .active_features
                .get(package)
                .cloned()
                .unwrap_or_default();
            if let Some(release) =
                release_index(self.catalog, &self.names, selected, package)
            {
                for feature in features {
                    if !release.features.iter().any(|f| f.name == feature) {
                        return Err(ResolveError::MissingFeature {
                            package: package.clone(),
                            selected: *version,
                            feature,
                            path: closure.paths.get(package).cloned().unwrap_or_default(),
                        });
                    }
                }
            }
        }

        for (package, version) in selected {
            let origins = closure.origins.get(package).map(Vec::as_slice).unwrap_or(&[]);
            let constraints = origins.iter().map(|o| o.constraint).collect::<Vec<_>>();
            if !constraints.iter().all(|c| matches(*c, *version)) {
                return Err(ResolveError::Conflict {
                    package: package.clone(),
                    selected: *version,
                    requirements: canonical_constraints(&constraints),
                    evidence: origin_evidence(
                        origins,
                        self.catalog,
                        &self.names,
                        self.roots,
                        selected,
                        &closure.paths,
                    ),
                });
            }
        }

        let mut unselected: Vec<String> = active
            .into_iter()
            .filter(|p| !selected.contains_key(p))
            .collect();
        unselected.sort();
        for package in &unselected {
            let origins = closure.origins.get(package).map(Vec::as_slice).unwrap_or(&[]);
            let constraints = origins.iter().map(|o| o.constraint).collect::<Vec<_>>();
            let pi = self.names[package];
            let has = self.catalog[pi]
                .releases
                .iter()
                .any(|r| constraints.iter().all(|c| matches(*c, r.version)));
            if !has {
                return Err(ResolveError::NoMatch {
                    package: package.clone(),
                    requirements: canonical_constraints(&constraints),
                    evidence: origin_evidence(
                        origins,
                        self.catalog,
                        &self.names,
                        self.roots,
                        selected,
                        &closure.paths,
                    ),
                });
            }
        }

        Ok(closure)
    }

    fn dfs(
        &mut self,
        selected: &mut std::collections::BTreeMap<String, Version>,
    ) -> Result<(), ResolveError> {
        let closure = match self.inspect(selected) {
            Ok(c) => c,
            Err(e) => {
                if matches!(e, ResolveError::Limit { .. }) {
                    return Err(e);
                }
                if self.first_error.is_none() {
                    self.first_error = Some(e);
                }
                return Ok(());
            }
        };

        let mut active: Vec<String> = closure.origins.keys().cloned().collect();
        active.extend(closure.requests.keys().cloned());
        active.sort();
        active.dedup();

        let Some(package) = active.into_iter().find(|p| !selected.contains_key(p)) else {
            let cost = self
                .lock_map
                .iter()
                .filter(|(p, v)| selected.get(*p) != Some(*v))
                .count();
            let replace = match &self.best {
                None => true,
                Some((best_cost, best_selection, _)) => {
                    cost < *best_cost
                        || (cost == *best_cost
                            && self.vector_cmp(selected, best_selection).is_gt())
                }
            };
            if replace {
                self.best = Some((cost, selected.clone(), closure));
            }
            return Ok(());
        };

        let constraints = closure
            .origins
            .get(&package)
            .map(|origins| origins.iter().map(|o| o.constraint).collect::<Vec<_>>())
            .unwrap_or_default();
        let pi = self.names[&package];
        let mut candidates: Vec<Version> = self.catalog[pi]
            .releases
            .iter()
            .filter(|r| constraints.iter().all(|c| matches(*c, r.version)))
            .map(|r| r.version)
            .collect();
        candidates.sort_by(|a, b| b.cmp(a));

        if let Some(lock) = self.lock_map.get(&package) {
            if let Some(pos) = candidates.iter().position(|v| v == lock) {
                let v = candidates.remove(pos);
                candidates.insert(0, v);
            }
        }

        for candidate in candidates {
            if self.attempts == self.budget {
                return Err(ResolveError::Limit {
                    attempts: self.budget,
                });
            }
            self.attempts += 1;
            selected.insert(package.clone(), candidate);
            let result = self.dfs(selected);
            selected.remove(&package);
            result?;
        }
        Ok(())
    }

    fn vector_cmp(
        &self,
        a: &std::collections::BTreeMap<String, Version>,
        b: &std::collections::BTreeMap<String, Version>,
    ) -> std::cmp::Ordering {
        for name in self.names.keys() {
            let ord = match (a.get(name), b.get(name)) {
                (None, None) => std::cmp::Ordering::Equal,
                (None, Some(_)) => std::cmp::Ordering::Less,
                (Some(_), None) => std::cmp::Ordering::Greater,
                (Some(x), Some(y)) => x.cmp(y),
            };
            if !ord.is_eq() {
                return ord;
            }
        }
        std::cmp::Ordering::Equal
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
                            site: format!(
                                "catalog[{}].release[{}].dep[{}].gate",
                                i, j, k
                            ),
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

    let mut seen = std::collections::BTreeSet::new();
    for package in catalog {
        if !seen.insert(package.name.clone()) {
            return Err(ResolveError::DuplicatePackage {
                package: package.name.clone(),
            });
        }
    }

    for package in catalog {
        let mut versions = std::collections::BTreeSet::new();
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
            let mut features = std::collections::BTreeSet::new();
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

    let mut lock_seen = std::collections::BTreeSet::new();
    for lock in locks {
        if !lock_seen.insert(lock.package.clone()) {
            return Err(ResolveError::DuplicateLock {
                package: lock.package.clone(),
            });
        }
    }

    let names: std::collections::BTreeMap<String, usize> = catalog
        .iter()
        .enumerate()
        .map(|(i, p)| (p.name.clone(), i))
        .collect();
    let lock_map = locks
        .iter()
        .map(|l| (l.package.clone(), l.version))
        .collect();

    let mut search = Search {
        catalog,
        roots,
        target,
        names,
        lock_map,
        attempts: 0,
        budget: max_attempts,
        first_error: None,
        best: None,
    };

    let mut selected = std::collections::BTreeMap::new();
    search.dfs(&mut selected)?;

    let Some((_, best, final_closure)) = search.best else {
        return Err(search.first_error.unwrap_or_else(|| ResolveError::NoMatch {
            package: String::new(),
            requirements: vec![],
            evidence: vec![],
        }));
    };

    let selected_vec = best
        .iter()
        .map(|(package, version)| Selection {
            package: package.clone(),
            version: *version,
        })
        .collect::<Vec<_>>();

    let mut changes = Vec::new();
    let mut all: std::collections::BTreeSet<String> =
        locks.iter().map(|l| l.package.clone()).collect();
    all.extend(best.keys().cloned());

    for package in all {
        let before = search.lock_map.get(&package).copied();
        let after = best.get(&package).copied();
        if before == after {
            continue;
        }
        let reason = match (before, after) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old), Some(_)) => {
                let exists = search.names.get(&package).is_some_and(|pi| {
                    catalog[*pi].releases.iter().any(|r| r.version == old)
                });
                if !exists {
                    ChangeReason::Unavailable
                } else {
                    let constraints = final_closure
                        .origins
                        .get(&package)
                        .map(|origins| origins.iter().map(|o| o.constraint).collect::<Vec<_>>())
                        .unwrap_or_default();
                    if !constraints.iter().all(|c| matches(*c, old)) {
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
        selected: selected_vec,
        changes,
    })
}