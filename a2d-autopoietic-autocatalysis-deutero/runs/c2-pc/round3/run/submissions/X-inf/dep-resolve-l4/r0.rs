use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version { pub major: u32, pub minor: u32, pub patch: u32 }

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Constraint { Exact(Version), AtLeast(Version), Before(Version), Caret(Version) }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection { pub package: String, pub version: Version }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package { pub name: String, pub releases: Vec<Release> }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target { Linux, Mac, Windows }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform { Any, Linux, Mac, Windows }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Requirement { pub package: String, pub constraint: Constraint, pub features: Vec<String> }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dependency {
    pub package: String,
    pub constraint: Constraint,
    pub features: Vec<String>,
    pub gate: Option<String>,
    pub platform: Platform,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Feature { pub name: String, pub enables: Vec<String> }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release { pub version: Version, pub deps: Vec<Dependency>, pub features: Vec<Feature> }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Evidence { pub constraint: Constraint, pub path: Vec<String> }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lock { pub package: String, pub version: Version }

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangeReason { Added, Removed, Unavailable, Rejected, Rebalanced }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub package: String,
    pub before: Option<Version>,
    pub after: Option<Version>,
    pub reason: ChangeReason,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolution { pub selected: Vec<Selection>, pub changes: Vec<Change> }

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolveError {
    InvalidName { site: String, name: String },
    DuplicatePackage { package: String },
    DuplicateVersion { package: String, version: Version },
    DuplicateFeature { package: String, version: Version, feature: String },
    DuplicateLock { package: String },
    Missing { package: String, evidence: Vec<Evidence> },
    NoMatch { package: String, requirements: Vec<Constraint>, evidence: Vec<Evidence> },
    Conflict { package: String, selected: Version, requirements: Vec<Constraint>, evidence: Vec<Evidence> },
    MissingFeature { package: String, selected: Version, feature: String, path: Vec<String> },
    Limit { attempts: usize },
}

fn valid_name(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b.len() <= 24
        && b[0].is_ascii_lowercase()
        && b[b.len() - 1] != b'-'
        && b.iter().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

fn constraint_key(c: &Constraint) -> (u8, Version) {
    match *c {
        Constraint::Exact(v) => (0, v),
        Constraint::AtLeast(v) => (1, v),
        Constraint::Before(v) => (2, v),
        Constraint::Caret(v) => (3, v),
    }
}

fn canonical(mut cs: Vec<Constraint>) -> Vec<Constraint> {
    cs.sort_by_key(|c| constraint_key(c));
    cs.dedup();
    cs
}

fn constraint_text(c: Constraint) -> String {
    let (s, v) = match c {
        Constraint::Exact(v) => ("=", v),
        Constraint::AtLeast(v) => (">=", v),
        Constraint::Before(v) => ("<", v),
        Constraint::Caret(v) => ("^", v),
    };
    format!("{s}{}.{}.{}", v.major, v.minor, v.patch)
}

fn version_text(v: Version) -> String {
    format!("{}.{}.{}", v.major, v.minor, v.patch)
}

fn matches(v: Version, c: Constraint) -> bool {
    match c {
        Constraint::Exact(x) => v == x,
        Constraint::AtLeast(x) => v >= x,
        Constraint::Before(x) => v < x,
        Constraint::Caret(x) => {
            v >= x && if x.major > 0 {
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
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn sorted_evidence(evidence: &[Evidence]) -> Vec<Evidence> {
    let mut es = evidence.to_vec();
    es.sort_by(|a, b| {
        constraint_key(&a.constraint)
            .cmp(&constraint_key(&b.constraint))
            .then_with(|| a.path.cmp(&b.path))
    });
    es
}

fn evidence_text(evidence: &[Evidence]) -> Vec<String> {
    sorted_evidence(evidence).into_iter().map(|e| {
        format!("  C:{} via {}", constraint_text(e.constraint), e.path.join(" -> "))
    }).collect()
}

pub fn error_text(error: &ResolveError) -> String {
    let (first, evidence) = match error {
        ResolveError::InvalidName { site, name } =>
            (format!("invalid-name|site={}|name={}", encode(site), encode(name)), None),
        ResolveError::DuplicatePackage { package } =>
            (format!("duplicate-package|package={}", encode(package)), None),
        ResolveError::DuplicateVersion { package, version } =>
            (format!("duplicate-version|package={}|version={}", encode(package), version_text(*version)), None),
        ResolveError::DuplicateFeature { package, version, feature } =>
            (format!("duplicate-feature|package={}|version={}|feature={}", encode(package), version_text(*version), encode(feature)), None),
        ResolveError::DuplicateLock { package } =>
            (format!("duplicate-lock|package={}", encode(package)), None),
        ResolveError::Missing { package, evidence } =>
            (format!("missing|package={}", encode(package)), Some(evidence.as_slice())),
        ResolveError::NoMatch { package, requirements, evidence } =>
            (format!("no-match|package={}|constraints={}", encode(package), canonical(requirements.clone()).iter().map(|c| constraint_text(*c)).collect::<Vec<_>>().join(",")), Some(evidence.as_slice())),
        ResolveError::Conflict { package, selected, requirements, evidence } =>
            (format!("conflict|package={}|selected={}|constraints={}", encode(package), version_text(*selected), canonical(requirements.clone()).iter().map(|c| constraint_text(*c)).collect::<Vec<_>>().join(",")), Some(evidence.as_slice())),
        ResolveError::MissingFeature { package, selected, feature, path } =>
            (format!("missing-feature|package={}|selected={}|feature={}", encode(package), version_text(*selected), encode(feature)), Some(path.as_slice())),
        ResolveError::Limit { attempts } => (format!("limit|attempts={attempts}"), None),
    };
    match evidence {
        Some(es) => {
            let mut s = first;
            for line in evidence_text(es) { s.push('\n'); s.push_str(&line); }
            s
        }
        None => first,
    }
}

#[derive(Clone)]
struct Origin {
    constraint: Constraint,
    path: Vec<String>,
}

#[derive(Clone, Default)]
struct Closure {
    constraints: BTreeMap<String, Vec<Constraint>>,
    features: BTreeMap<String, BTreeSet<String>>,
    origins: BTreeMap<String, Vec<Origin>>,
    paths: BTreeMap<String, Vec<String>>,
}

struct Solver<'a> {
    packages: BTreeMap<String, &'a Package>,
    roots: &'a [Requirement],
    locks: BTreeMap<String, Version>,
    target: Target,
    max_attempts: usize,
    attempts: usize,
    best: Option<(usize, Vec<Selection>, Closure)>,
    first_error: Option<ResolveError>,
    catalog_names: Vec<String>,
}

impl<'a> Solver<'a> {
    fn platform_matches(&self, p: Platform) -> bool {
        p == Platform::Any || matches!((p, self.target),
            (Platform::Linux, Target::Linux) |
            (Platform::Mac, Target::Mac) |
            (Platform::Windows, Target::Windows))
    }

    fn release(&self, name: &str, version: Version) -> Option<&'a Release> {
        self.packages.get(name)?.releases.iter().find(|r| r.version == version)
    }

    fn dependency_token(pkg: &str, version: Version, index: usize, dep: &Dependency) -> String {
        let mut fs = dep.features.clone();
        fs.sort();
        fs.dedup();
        let gate = dep.gate.as_deref().unwrap_or("*");
        let plat = match dep.platform {
            Platform::Any => "any",
            Platform::Linux => "linux",
            Platform::Mac => "mac",
            Platform::Windows => "windows",
        };
        format!("{}@{}/deps[{}]:{}@{}+{{{}}};gate={};platform={}",
            pkg, version_text(version), index, dep.package, constraint_text(dep.constraint),
            fs.join(","), gate, plat)
    }

    fn closure(&self, assigned: &BTreeMap<String, Version>) -> Closure {
        let mut c = Closure::default();
        let mut queue: Vec<(String, String)> = Vec::new();
        let mut root_origins: BTreeMap<String, Vec<Origin>> = BTreeMap::new();

        for (i, root) in self.roots.iter().enumerate() {
            let mut fs = root.features.clone();
            fs.sort();
            fs.dedup();
            let token = format!("root[{i}]:{}@{}+{{{}}}", root.package, constraint_text(root.constraint), fs.join(","));
            c.constraints.entry(root.package.clone()).or_default().push(root.constraint);
            root_origins.entry(root.package.clone()).or_default().push(Origin {
                constraint: root.constraint,
                path: vec![token.clone()],
            });
            queue.extend(fs.into_iter().map(|f| (root.package.clone(), f)));
        }

        let mut processed_features: BTreeSet<(String, Version, String)> = BTreeSet::new();
        let mut processed_deps: BTreeSet<(String, Version, usize)> = BTreeSet::new();

        loop {
            while let Some((pkg, feature)) = queue.pop() {
                c.features.entry(pkg.clone()).or_default().insert(feature.clone());
                let Some(&version) = assigned.get(&pkg) else { continue };
                if !processed_features.insert((pkg.clone(), version, feature.clone())) { continue };
                let Some(release) = self.release(&pkg, version) else { continue };
                if let Some(decl) = release.features.iter().find(|f| f.name == feature) {
                    queue.extend(decl.enables.iter().cloned().map(|f| (pkg.clone(), f)));
                }
            }

            let mut changed = false;
            let selected: Vec<(String, Version)> = assigned.iter().map(|(n, v)| (n.clone(), *v)).collect();
            for (pkg, version) in selected {
                let Some(release) = self.release(&pkg, version) else { continue };
                for (j, dep) in release.deps.iter().enumerate() {
                    if !self.platform_matches(dep.platform) { continue; }
                    if dep.gate.as_ref().is_some_and(|g| !c.features.get(&pkg).is_some_and(|fs| fs.contains(g))) { continue; }
                    if !processed_deps.insert((pkg.clone(), version, j)) { continue; }
                    changed = true;
                    c.constraints.entry(dep.package.clone()).or_default().push(dep.constraint);
                    queue.extend(dep.features.iter().cloned().map(|f| (dep.package.clone(), f)));
                }
            }
            if queue.is_empty() && !changed { break; }
        }

        for cs in c.constraints.values_mut() { *cs = canonical(std::mem::take(cs)); }

        // Compute shortest canonical paths only from actual roots. Repeated relaxation
        // propagates improvements through all active edges, including cycles.
        c.paths.clear();
        for origins in root_origins.values() {
            for origin in origins {
                let package = origin.path[0].split_once(':').map(|(s, _)| s).unwrap_or("");
                let _ = package;
            }
        }
        for (i, root) in self.roots.iter().enumerate() {
            let mut fs = root.features.clone();
            fs.sort();
            fs.dedup();
            let token = format!("root[{i}]:{}@{}+{{{}}}", root.package, constraint_text(root.constraint), fs.join(","));
            let path = vec![token];
            c.paths.entry(root.package.clone()).and_modify(|old| {
                if path.len() < old.len() || (path.len() == old.len() && path < *old) { *old = path.clone(); }
            }).or_insert(path);
        }

        loop {
            let mut updates: Vec<(String, Vec<String>)> = Vec::new();
            for (pkg, version) in assigned {
                let Some(source_path) = c.paths.get(pkg).cloned() else { continue };
                let Some(release) = self.release(pkg, *version) else { continue };
                let active_features = c.features.get(pkg);
                for (j, dep) in release.deps.iter().enumerate() {
                    if !self.platform_matches(dep.platform) { continue; }
                    if dep.gate.as_ref().is_some_and(|g| !active_features.is_some_and(|fs| fs.contains(g))) { continue; }
                    let mut path = source_path.clone();
                    path.push(Self::dependency_token(pkg, *version, j, dep));
                    updates.push((dep.package.clone(), path));
                }
            }
            let mut improved = false;
            for (name, path) in updates {
                let better = c.paths.get(&name).is_none_or(|old| {
                    path.len() < old.len() || (path.len() == old.len() && path < *old)
                });
                if better {
                    c.paths.insert(name, path);
                    improved = true;
                }
            }
            if !improved { break; }
        }

        c.origins = self.active_origins(assigned, &c);
        c
    }

    fn active_origins(&self, assigned: &BTreeMap<String, Version>, c: &Closure) -> BTreeMap<String, Vec<Origin>> {
        let mut out: BTreeMap<String, Vec<Origin>> = BTreeMap::new();
        for (i, root) in self.roots.iter().enumerate() {
            let mut fs = root.features.clone(); fs.sort(); fs.dedup();
            let token = format!("root[{i}]:{}@{}+{{{}}}", root.package, constraint_text(root.constraint), fs.join(","));
            out.entry(root.package.clone()).or_default().push(Origin { constraint: root.constraint, path: vec![token] });
        }
        for (pkg, version) in assigned {
            let Some(release) = self.release(pkg, *version) else { continue };
            for (j, dep) in release.deps.iter().enumerate() {
                if !self.platform_matches(dep.platform) { continue; }
                if dep.gate.as_ref().is_some_and(|g| !c.features.get(pkg).is_some_and(|fs| fs.contains(g))) { continue; }
                let mut path = c.paths.get(pkg).cloned().unwrap_or_default();
                path.push(Self::dependency_token(pkg, *version, j, dep));
                out.entry(dep.package.clone()).or_default().push(Origin { constraint: dep.constraint, path });
            }
        }
        out
    }

    fn dynamic_error(&self, assigned: &BTreeMap<String, Version>, c: &Closure) -> Option<ResolveError> {
        for name in c.constraints.keys() {
            if !self.packages.contains_key(name) {
                return Some(ResolveError::Missing { package: name.clone(), evidence: self.evidence(name, c) });
            }
        }

        for (pkg, version) in assigned {
            let Some(features) = c.features.get(pkg) else { continue };
            for feature in features {
                let declared = self.release(pkg, *version).is_some_and(|r| r.features.iter().any(|f| f.name == *feature));
                if !declared {
                    return Some(ResolveError::MissingFeature {
                        package: pkg.clone(), selected: *version, feature: feature.clone(),
                        path: c.paths.get(pkg).cloned().unwrap_or_default(),
                    });
                }
            }
        }

        for (pkg, version) in assigned {
            let reqs = c.constraints.get(pkg).cloned().unwrap_or_default();
            if reqs.iter().any(|r| !matches(*version, *r)) {
                return Some(ResolveError::Conflict {
                    package: pkg.clone(), selected: *version, requirements: reqs,
                    evidence: self.evidence(pkg, c),
                });
            }
        }

        for (pkg, reqs) in &c.constraints {
            if assigned.contains_key(pkg) { continue; }
            let has = self.packages[pkg].releases.iter().any(|r| reqs.iter().all(|c| matches(r.version, *c)));
            if !has {
                return Some(ResolveError::NoMatch {
                    package: pkg.clone(), requirements: reqs.clone(),
                    evidence: self.evidence(pkg, c),
                });
            }
        }
        None
    }

    fn evidence(&self, pkg: &str, c: &Closure) -> Vec<Evidence> {
        let mut evidence: Vec<Evidence> = c.origins.get(pkg).cloned().unwrap_or_default().into_iter()
            .map(|o| Evidence { constraint: o.constraint, path: o.path }).collect();
        evidence.sort_by(|a, b| {
            constraint_key(&a.constraint).cmp(&constraint_key(&b.constraint))
                .then_with(|| a.path.cmp(&b.path))
        });
        evidence
    }

    fn score(&self, assigned: &BTreeMap<String, Version>) -> usize {
        self.locks.iter().filter(|(p, v)| assigned.get(*p) != Some(*v)).count()
    }

    fn better(&self, score: usize, assigned: &BTreeMap<String, Version>) -> bool {
        let Some((best_score, best, _)) = &self.best else { return true };
        if score != *best_score { return score < *best_score; }
        for name in &self.catalog_names {
            let a = assigned.get(name);
            let b = best.iter().find(|s| &s.package == name).map(|s| &s.version);
            match (a, b) {
                (None, None) => {}
                (None, Some(_)) => return false,
                (Some(_), None) => return true,
                (Some(x), Some(y)) if x != y => return x > y,
                _ => {}
            }
        }
        false
    }

    fn search(&mut self, assigned: &mut BTreeMap<String, Version>) -> Result<(), ResolveError> {
        let closure = self.closure(assigned);
        if let Some(err) = self.dynamic_error(assigned, &closure) {
            if self.first_error.is_none() { self.first_error = Some(err); }
            return Ok(());
        }

        let next = closure.constraints.keys().find(|p| !assigned.contains_key(*p)).cloned();
        let Some(pkg) = next else {
            let score = self.score(assigned);
            if self.better(score, assigned) {
                let selected = assigned.iter().map(|(package, version)| Selection { package: package.clone(), version: *version }).collect();
                self.best = Some((score, selected, closure));
            }
            return Ok(());
        };

        let mut candidates: Vec<Version> = self.packages[&pkg].releases.iter()
            .filter(|r| closure.constraints[&pkg].iter().all(|c| matches(r.version, *c)))
            .map(|r| r.version).collect();
        candidates.sort_by(|a, b| b.cmp(a));
        if let Some(lock) = self.locks.get(&pkg) {
            if let Some(i) = candidates.iter().position(|v| v == lock) {
                let v = candidates.remove(i);
                candidates.insert(0, v);
            }
        }

        for candidate in candidates {
            if self.attempts == self.max_attempts {
                return Err(ResolveError::Limit { attempts: self.max_attempts });
            }
            self.attempts += 1;
            assigned.insert(pkg.clone(), candidate);
            let result = self.search(assigned);
            assigned.remove(&pkg);
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
    for (i, r) in roots.iter().enumerate() {
        if !valid_name(&r.package) {
            return Err(ResolveError::InvalidName { site: format!("root[{i}]"), name: r.package.clone() });
        }
        for (f, name) in r.features.iter().enumerate() {
            if !valid_name(name) {
                return Err(ResolveError::InvalidName { site: format!("root[{i}].feature[{f}]"), name: name.clone() });
            }
        }
    }
    for (i, p) in catalog.iter().enumerate() {
        if !valid_name(&p.name) {
            return Err(ResolveError::InvalidName { site: format!("catalog[{i}]"), name: p.name.clone() });
        }
        for (j, r) in p.releases.iter().enumerate() {
            for (k, d) in r.deps.iter().enumerate() {
                if !valid_name(&d.package) {
                    return Err(ResolveError::InvalidName { site: format!("catalog[{i}].release[{j}].dep[{k}]"), name: d.package.clone() });
                }
                if let Some(g) = &d.gate {
                    if !valid_name(g) {
                        return Err(ResolveError::InvalidName { site: format!("catalog[{i}].release[{j}].dep[{k}].gate"), name: g.clone() });
                    }
                }
                for (f, name) in d.features.iter().enumerate() {
                    if !valid_name(name) {
                        return Err(ResolveError::InvalidName { site: format!("catalog[{i}].release[{j}].dep[{k}].feature[{f}]"), name: name.clone() });
                    }
                }
            }
            for (f, feature) in r.features.iter().enumerate() {
                if !valid_name(&feature.name) {
                    return Err(ResolveError::InvalidName { site: format!("catalog[{i}].release[{j}].feature[{f}]"), name: feature.name.clone() });
                }
                for (e, name) in feature.enables.iter().enumerate() {
                    if !valid_name(name) {
                        return Err(ResolveError::InvalidName { site: format!("catalog[{i}].release[{j}].feature[{f}].enable[{e}]"), name: name.clone() });
                    }
                }
            }
        }
    }

    let mut seen = BTreeSet::new();
    for p in catalog {
        if !seen.insert(p.name.clone()) { return Err(ResolveError::DuplicatePackage { package: p.name.clone() }); }
    }
    for p in catalog {
        let mut versions = BTreeSet::new();
        for r in &p.releases {
            if !versions.insert(r.version) {
                return Err(ResolveError::DuplicateVersion { package: p.name.clone(), version: r.version });
            }
        }
    }
    for p in catalog {
        for r in &p.releases {
            let mut features = BTreeSet::new();
            for f in &r.features {
                if !features.insert(f.name.clone()) {
                    return Err(ResolveError::DuplicateFeature { package: p.name.clone(), version: r.version, feature: f.name.clone() });
                }
            }
        }
    }
    for (i, l) in locks.iter().enumerate() {
        if !valid_name(&l.package) {
            return Err(ResolveError::InvalidName { site: format!("lock[{i}]"), name: l.package.clone() });
        }
    }
    let mut lock_seen = BTreeSet::new();
    for l in locks {
        if !lock_seen.insert(l.package.clone()) {
            return Err(ResolveError::DuplicateLock { package: l.package.clone() });
        }
    }

    let package_map: BTreeMap<String, &Package> = catalog.iter().map(|p| (p.name.clone(), p)).collect();
    let locks_map: BTreeMap<String, Version> = locks.iter().map(|l| (l.package.clone(), l.version)).collect();
    let mut solver = Solver {
        packages: package_map,
        roots,
        locks: locks_map,
        target,
        max_attempts,
        attempts: 0,
        best: None,
        first_error: None,
        catalog_names: catalog.iter().map(|p| p.name.clone()).collect(),
    };
    solver.catalog_names.sort();
    let mut assigned = BTreeMap::new();
    solver.search(&mut assigned)?;

    let Some((_, selected, closure)) = solver.best else {
        return Err(solver.first_error.unwrap_or(ResolveError::Missing { package: String::new(), evidence: vec![] }));
    };

    let after: BTreeMap<String, Version> = selected.iter().map(|s| (s.package.clone(), s.version)).collect();
    let mut names: BTreeSet<String> = solver.locks.keys().cloned().collect();
    names.extend(after.keys().cloned());
    let mut changes = Vec::new();
    for name in names {
        let before = solver.locks.get(&name).copied();
        let now = after.get(&name).copied();
        if before == now { continue; }
        let reason = match (before, now) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old), Some(_)) => {
                let exists = solver.packages.get(&name).is_some_and(|p| p.releases.iter().any(|r| r.version == old));
                if !exists { ChangeReason::Unavailable }
                else if closure.constraints.get(&name).is_some_and(|cs| cs.iter().any(|c| !matches(old, *c))) {
                    ChangeReason::Rejected
                } else { ChangeReason::Rebalanced }
            }
            (None, None) => unreachable!(),
        };
        changes.push(Change { package: name, before, after: now, reason });
    }
    Ok(Resolution { selected, changes })
}