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

fn satisfies(v: Version, cs: &[Constraint]) -> bool {
    cs.iter().all(|c| matches(v, *c))
}

fn enc(s: &str) -> String {
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
    let mut v = e.to_vec();
    v.sort_by(|a, b| {
        a.constraint
            .cmp(&b.constraint)
            .then_with(|| a.path.cmp(&b.path))
    });
    v
}

fn evidence_lines(e: &[Evidence]) -> String {
    let mut out = String::new();
    for ev in sorted_evidence(e) {
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
        ResolveError::Missing { package, evidence } => {
            format!("missing|package={}{}", enc(package), evidence_lines(evidence))
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
                enc(package),
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
                enc(package),
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
            enc(package),
            version_text(*selected),
            enc(feature),
            enc(feature),
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
struct Closure {
    origins: BTreeMap<String, Vec<Origin>>,
    requests: BTreeMap<String, BTreeSet<String>>,
    activated: BTreeMap<String, BTreeSet<String>>,
    paths: BTreeMap<String, Vec<String>>,
}

fn sorted_features(fs: &[String]) -> String {
    let set: BTreeSet<_> = fs.iter().cloned().collect();
    set.into_iter().collect::<Vec<_>>().join(",")
}

fn platform_text(p: Platform) -> &'static str {
    match p {
        Platform::Any => "any",
        Platform::Linux => "linux",
        Platform::Mac => "mac",
        Platform::Windows => "windows",
    }
}

fn target_matches(p: Platform, t: Target) -> bool {
    matches!(p, Platform::Any)
        || matches!(
            (p, t),
            (Platform::Linux, Target::Linux)
                | (Platform::Mac, Target::Mac)
                | (Platform::Windows, Target::Windows)
        )
}

fn release<'a>(
    catalog: &'a BTreeMap<String, &'a Package>,
    name: &str,
    v: Version,
) -> Option<&'a Release> {
    catalog
        .get(name)?
        .releases
        .iter()
        .find(|r| r.version == v)
}

fn compute_closure(
    catalog: &BTreeMap<String, &Package>,
    roots: &[Requirement],
    selected: &BTreeMap<String, Version>,
    target: Target,
) -> Closure {
    let mut c = Closure::default();

    for root in roots {
        c.requests
            .entry(root.package.clone())
            .or_default()
            .extend(root.features.iter().cloned());
    }

    // Reach a fixed point over feature requests and active dependencies. Paths
    // and evidence are computed afterward from the final active graph.
    loop {
        let mut changed = false;

        for (name, version) in selected {
            let Some(rel) = release(catalog, name, *version) else {
                continue;
            };

            let requests = c.requests.get(name).cloned().unwrap_or_default();
            let activated = c.activated.entry(name.clone()).or_default();
            for feature in requests {
                if activated.insert(feature) {
                    changed = true;
                }
            }

            loop {
                let active = c.activated.get(name).cloned().unwrap_or_default();
                let mut added = false;
                for feature in active {
                    if let Some(decl) = rel.features.iter().find(|f| f.name == feature) {
                        for enabled in &decl.enables {
                            if c.activated
                                .entry(name.clone())
                                .or_default()
                                .insert(enabled.clone())
                            {
                                added = true;
                                changed = true;
                            }
                        }
                    }
                }
                if !added {
                    break;
                }
            }

            let active_features = c.activated.get(name).cloned().unwrap_or_default();
            for dep in &rel.deps {
                if !target_matches(dep.platform, target)
                    || dep
                        .gate
                        .as_ref()
                        .is_some_and(|gate| !active_features.contains(gate))
                {
                    continue;
                }
                let before = c.requests.get(&dep.package).cloned().unwrap_or_default();
                c.requests
                    .entry(dep.package.clone())
                    .or_default()
                    .extend(dep.features.iter().cloned());
                if c.requests.get(&dep.package) != Some(&before) {
                    changed = true;
                }
            }
        }

        if !changed {
            break;
        }
    }

    // Construct the final active dependency graph. One edge is kept for each
    // active dependency, including edges that happen to have equal contents.
    let mut edges: BTreeMap<String, Vec<(String, Constraint, String)>> = BTreeMap::new();
    for (name, version) in selected {
        let Some(rel) = release(catalog, name, *version) else {
            continue;
        };
        let active_features = c.activated.get(name).cloned().unwrap_or_default();
        for (j, dep) in rel.deps.iter().enumerate() {
            if !target_matches(dep.platform, target)
                || dep
                    .gate
                    .as_ref()
                    .is_some_and(|gate| !active_features.contains(gate))
            {
                continue;
            }
            let token = format!(
                "{}@{}/deps[{}]:{}@{}+{{{}}};gate={};platform={}",
                name,
                version_text(*version),
                j,
                dep.package,
                constraint_text(dep.constraint),
                sorted_features(&dep.features),
                dep.gate.as_deref().unwrap_or("*"),
                platform_text(dep.platform)
            );
            edges
                .entry(name.clone())
                .or_default()
                .push((dep.package.clone(), dep.constraint, token));
        }
    }

    // Initialize provenance with every root token.
    for (i, root) in roots.iter().enumerate() {
        let path = vec![format!(
            "root[{}]:{}@{}+{{{}}}",
            i,
            root.package,
            constraint_text(root.constraint),
            sorted_features(&root.features)
        )];
        c.paths
            .entry(root.package.clone())
            .and_modify(|p| {
                if path.len() < p.len() || (path.len() == p.len() && path < *p) {
                    *p = path.clone();
                }
            })
            .or_insert(path);
    }

    // Repeated relaxation finds shortest, then lexicographically smallest,
    // paths in the final graph; cycles cannot improve a shortest path.
    loop {
        let mut changed = false;
        let sources: Vec<(String, Vec<String>)> = c
            .paths
            .iter()
            .map(|(name, path)| (name.clone(), path.clone()))
            .collect();
        for (source, source_path) in sources {
            for (destination, _, token) in edges.get(&source).into_iter().flatten() {
                let mut path = source_path.clone();
                path.push(token.clone());
                let improves = c.paths.get(destination).is_none_or(|old| {
                    path.len() < old.len() || (path.len() == old.len() && path < *old)
                });
                if improves {
                    c.paths.insert(destination.clone(), path);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }

    // Each root and each active dependency contributes exactly one origin,
    // using final canonical paths for dependency sources.
    for (i, root) in roots.iter().enumerate() {
        c.origins
            .entry(root.package.clone())
            .or_default()
            .push(Origin {
                constraint: root.constraint,
                path: vec![format!(
                    "root[{}]:{}@{}+{{{}}}",
                    i,
                    root.package,
                    constraint_text(root.constraint),
                    sorted_features(&root.features)
                )],
            });
    }
    for (source, deps) in &edges {
        let Some(source_path) = c.paths.get(source) else {
            continue;
        };
        for (destination, constraint, token) in deps {
            let mut path = source_path.clone();
            path.push(token.clone());
            c.origins
                .entry(destination.clone())
                .or_default()
                .push(Origin {
                    constraint: *constraint,
                    path,
                });
        }
    }

    c
}

fn canonical_for(c: &Closure, package: &str) -> (Vec<Constraint>, Vec<Evidence>) {
    let origins = c.origins.get(package).cloned().unwrap_or_default();
    let constraints = canonical_constraints(
        &origins
            .iter()
            .map(|o| o.constraint)
            .collect::<Vec<_>>(),
    );
    let mut evidence: Vec<Evidence> = origins
        .into_iter()
        .map(|o| Evidence {
            constraint: o.constraint,
            path: o.path,
        })
        .collect();
    evidence.sort_by(|a, b| a.constraint.cmp(&b.constraint).then(a.path.cmp(&b.path)));
    (constraints, evidence)
}

fn dynamic_error(
    catalog: &BTreeMap<String, &Package>,
    selected: &BTreeMap<String, Version>,
    closure: &Closure,
) -> Option<ResolveError> {
    for name in closure.origins.keys() {
        if !catalog.contains_key(name) {
            let (_, evidence) = canonical_for(closure, name);
            return Some(ResolveError::Missing {
                package: name.clone(),
                evidence,
            });
        }
    }

    // MissingFeature precedes conflicts and candidate exhaustion.
    for (name, version) in selected {
        let Some(rel) = release(catalog, name, *version) else {
            continue;
        };
        let active = closure.activated.get(name).cloned().unwrap_or_default();
        for feature in active {
            if !rel.features.iter().any(|f| f.name == feature) {
                return Some(ResolveError::MissingFeature {
                    package: name.clone(),
                    selected: *version,
                    feature,
                    path: closure.paths.get(name).cloned().unwrap_or_default(),
                });
            }
        }
    }

    for (name, version) in selected {
        let (constraints, _) = canonical_for(closure, name);
        if !satisfies(*version, &constraints) {
            let (_, evidence) = canonical_for(closure, name);
            return Some(ResolveError::Conflict {
                package: name.clone(),
                selected: *version,
                requirements: constraints,
                evidence,
            });
        }
    }

    for name in closure.origins.keys() {
        if selected.contains_key(name) {
            continue;
        }
        let (constraints, evidence) = canonical_for(closure, name);
        let has_match = catalog[name]
            .releases
            .iter()
            .any(|r| satisfies(r.version, &constraints));
        if !has_match {
            return Some(ResolveError::NoMatch {
                package: name.clone(),
                requirements: constraints,
                evidence,
            });
        }
    }

    None
}

struct Search<'a> {
    catalog: &'a BTreeMap<String, &'a Package>,
    roots: &'a [Requirement],
    locks: &'a BTreeMap<String, Version>,
    target: Target,
    budget: usize,
    attempts: usize,
    first_error: Option<ResolveError>,
    best: Option<(usize, BTreeMap<String, Version>)>,
    catalog_names: Vec<String>,
}

impl Search<'_> {
    fn visit(&mut self, selected: &mut BTreeMap<String, Version>) -> Result<(), ResolveError> {
        let closure = compute_closure(self.catalog, self.roots, selected, self.target);
        if let Some(error) = dynamic_error(self.catalog, selected, &closure) {
            if self.first_error.is_none() {
                self.first_error = Some(error);
            }
            return Ok(());
        }

        let next = closure
            .origins
            .keys()
            .find(|name| !selected.contains_key(*name))
            .cloned();

        let Some(name) = next else {
            let cost = self
                .locks
                .iter()
                .filter(|(package, version)| selected.get(*package) != Some(*version))
                .count();
            let replace = match &self.best {
                None => true,
                Some((best_cost, best_sel)) if cost < *best_cost => true,
                Some((best_cost, best_sel)) if cost == *best_cost => {
                    self.vector_cmp(selected, best_sel).is_gt()
                }
                _ => false,
            };
            if replace {
                self.best = Some((cost, selected.clone()));
            }
            return Ok(());
        };

        let (constraints, _) = canonical_for(&closure, &name);
        let mut candidates: Vec<Version> = self.catalog[&name]
            .releases
            .iter()
            .filter(|r| satisfies(r.version, &constraints))
            .map(|r| r.version)
            .collect();
        candidates.sort_by(|a, b| b.cmp(a));
        candidates.dedup();

        if let Some(lock) = self.locks.get(&name) {
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
            selected.insert(name.clone(), candidate);
            let result = self.visit(selected);
            selected.remove(&name);
            result?;
        }
        Ok(())
    }

    fn vector_cmp(
        &self,
        a: &BTreeMap<String, Version>,
        b: &BTreeMap<String, Version>,
    ) -> std::cmp::Ordering {
        for name in &self.catalog_names {
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
        for (j, rel) in package.releases.iter().enumerate() {
            for (k, dep) in rel.deps.iter().enumerate() {
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
            for (f, feature) in rel.features.iter().enumerate() {
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
        for rel in &package.releases {
            if !seen_versions.insert(rel.version) {
                return Err(ResolveError::DuplicateVersion {
                    package: package.name.clone(),
                    version: rel.version,
                });
            }
        }
    }

    for package in catalog {
        for rel in &package.releases {
            let mut seen_features = BTreeSet::new();
            for feature in &rel.features {
                if !seen_features.insert(feature.name.clone()) {
                    return Err(ResolveError::DuplicateFeature {
                        package: package.name.clone(),
                        version: rel.version,
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

    let mut cat = BTreeMap::new();
    for package in catalog {
        cat.insert(package.name.clone(), package);
    }

    let mut lock_map = BTreeMap::new();
    for lock in locks {
        lock_map.insert(lock.package.clone(), lock.version);
    }

    let catalog_names = cat.keys().cloned().collect();
    let mut search = Search {
        catalog: &cat,
        roots,
        locks: &lock_map,
        target,
        budget: max_attempts,
        attempts: 0,
        first_error: None,
        best: None,
        catalog_names,
    };
    search.visit(&mut BTreeMap::new())?;

    let Some((_, solution)) = search.best else {
        return Err(search.first_error.unwrap_or_else(|| ResolveError::NoMatch {
            package: String::new(),
            requirements: Vec::new(),
            evidence: Vec::new(),
        }));
    };

    let final_closure = compute_closure(&cat, roots, &solution, target);
    let selected: Vec<Selection> = solution
        .iter()
        .map(|(package, version)| Selection {
            package: package.clone(),
            version: *version,
        })
        .collect();

    let mut all_names: BTreeSet<String> = lock_map.keys().cloned().collect();
    all_names.extend(solution.keys().cloned());
    let mut changes = Vec::new();

    for package in all_names {
        let before = lock_map.get(&package).copied();
        let after = solution.get(&package).copied();
        if before == after {
            continue;
        }

        let reason = match (before, after) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old), Some(_)) => {
                let exists = cat
                    .get(&package)
                    .is_some_and(|p| p.releases.iter().any(|r| r.version == old));
                if !exists {
                    ChangeReason::Unavailable
                } else {
                    let (constraints, _) = canonical_for(&final_closure, &package);
                    if !satisfies(old, &constraints) {
                        ChangeReason::Rejected
                    } else {
                        ChangeReason::Rebalanced
                    }
                }
            }
            (None, None) => unreachable!(),
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