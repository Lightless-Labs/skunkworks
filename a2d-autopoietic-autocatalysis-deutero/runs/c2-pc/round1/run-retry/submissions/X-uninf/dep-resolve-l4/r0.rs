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

fn constraint_key(c: Constraint) -> (u8, Version) {
    match c {
        Constraint::Exact(v) => (0, v),
        Constraint::AtLeast(v) => (1, v),
        Constraint::Before(v) => (2, v),
        Constraint::Caret(v) => (3, v),
    }
}

fn constraint_text(c: Constraint) -> String {
    let (prefix, v) = match c {
        Constraint::Exact(v) => ("=", v),
        Constraint::AtLeast(v) => (">=", v),
        Constraint::Before(v) => ("<", v),
        Constraint::Caret(v) => ("^", v),
    };
    format!("{prefix}{}", version_text(v))
}

fn canonical_constraints(mut constraints: Vec<Constraint>) -> Vec<Constraint> {
    constraints.sort_by_key(|c| constraint_key(*c));
    constraints.dedup();
    constraints
}

fn constraints_text(constraints: &[Constraint]) -> String {
    canonical_constraints(constraints.to_vec())
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
            out.push_str(&format!("%{b:02X}"));
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

fn evidence_lines(evidence: &[Evidence]) -> String {
    let mut out = String::new();
    for e in sorted_evidence(evidence) {
        out.push_str("\n  C:");
        out.push_str(&constraint_text(e.constraint));
        out.push_str(" via ");
        out.push_str(&e.path.join(" -> "));
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
        ResolveError::Limit { attempts } => format!("limit|attempts={attempts}"),
    }
}

fn valid_name(name: &str) -> bool {
    let b = name.as_bytes();
    (1..=24).contains(&b.len())
        && b[0].is_ascii_lowercase()
        && b.last().is_some_and(|c| *c != b'-')
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
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

fn target_matches(platform: Platform, target: Target) -> bool {
    match platform {
        Platform::Any => true,
        Platform::Linux => target == Target::Linux,
        Platform::Mac => target == Target::Mac,
        Platform::Windows => target == Target::Windows,
    }
}

fn target_text(platform: Platform) -> &'static str {
    match platform {
        Platform::Any => "any",
        Platform::Linux => "linux",
        Platform::Mac => "mac",
        Platform::Windows => "windows",
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
    origins: BTreeMap<String, Vec<Origin>>,
    requests: BTreeMap<String, BTreeSet<String>>,
    activated: BTreeMap<String, BTreeSet<String>>,
    paths: BTreeMap<String, Vec<String>>,
    active_deps: BTreeMap<String, BTreeSet<usize>>,
}

fn append_token(path: &[String], token: String) -> Vec<String> {
    let mut p = path.to_vec();
    p.push(token);
    p
}

fn canonical_paths_better(a: &[String], b: &[String]) -> bool {
    a.len() < b.len() || (a.len() == b.len() && a < b)
}

fn closure_for(
    roots: &[Requirement],
    catalog: &BTreeMap<String, &Package>,
    chosen: &BTreeMap<String, Version>,
    target: Target,
) -> Closure {
    let mut c = Closure::default();
    for (i, root) in roots.iter().enumerate() {
        let mut fs = root.features.clone();
        fs.sort();
        fs.dedup();
        let ftext = fs.join(",");
        let token = format!(
            "root[{i}]:{}@{}+{{{ftext}}}",
            root.package,
            constraint_text(root.constraint)
        );
        c.constraints
            .entry(root.package.clone())
            .or_default()
            .push(root.constraint);
        c.origins
            .entry(root.package.clone())
            .or_default()
            .push(Origin {
                constraint: root.constraint,
                path: vec![token.clone()],
            });
        c.requests
            .entry(root.package.clone())
            .or_default()
            .extend(fs);
        let p = vec![token];
        match c.paths.get(&root.package) {
            Some(old) if !canonical_paths_better(&p, old) => {}
            _ => {
                c.paths.insert(root.package.clone(), p);
            }
        }
    }

    loop {
        let mut changed = false;

        let selected_names: Vec<String> = chosen.keys().cloned().collect();
        for name in selected_names {
            let Some(package) = catalog.get(&name) else {
                continue;
            };
            let version = chosen[&name];
            let Some(release) = package.releases.iter().find(|r| r.version == version) else {
                continue;
            };

            let mut feature_map: BTreeMap<&str, &Feature> = BTreeMap::new();
            for feature in &release.features {
                feature_map.insert(&feature.name, feature);
            }

            let requests = c.requests.get(&name).cloned().unwrap_or_default();
            let activated = c.activated.entry(name.clone()).or_default();
            for f in requests {
                if activated.insert(f) {
                    changed = true;
                }
            }

            let active_names = c.activated.get(&name).cloned().unwrap_or_default();
            for feature_name in active_names {
                if let Some(feature) = feature_map.get(feature_name.as_str()) {
                    let set = c.requests.entry(name.clone()).or_default();
                    for enabled in &feature.enables {
                        if set.insert(enabled.clone()) {
                            changed = true;
                        }
                    }
                }
            }

            let path = c.paths.get(&name).cloned().unwrap_or_default();
            for (j, dep) in release.deps.iter().enumerate() {
                if !target_matches(dep.platform, target)
                    || dep
                        .gate
                        .as_ref()
                        .is_some_and(|g| !c.activated.get(&name).is_some_and(|s| s.contains(g)))
                {
                    continue;
                }
                if c.active_deps.entry(name.clone()).or_default().insert(j) {
                    changed = true;
                }
                let mut fs = dep.features.clone();
                fs.sort();
                fs.dedup();
                let token = format!(
                    "{}@{}/deps[{}]:{}@{}+{{{}}};gate={};platform={}",
                    name,
                    version_text(version),
                    j,
                    dep.package,
                    constraint_text(dep.constraint),
                    fs.join(","),
                    dep.gate.as_deref().unwrap_or("*"),
                    target_text(dep.platform)
                );
                let dep_path = append_token(&path, token);
                c.constraints
                    .entry(dep.package.clone())
                    .or_default()
                    .push(dep.constraint);
                let origins = c.origins.entry(dep.package.clone()).or_default();
                if !origins.iter().any(|o| {
                    o.constraint == dep.constraint && o.path == dep_path
                }) {
                    origins.push(Origin {
                        constraint: dep.constraint,
                        path: dep_path.clone(),
                    });
                    changed = true;
                }
                let reqs = c.requests.entry(dep.package.clone()).or_default();
                for f in fs {
                    if reqs.insert(f) {
                        changed = true;
                    }
                }
                match c.paths.get(&dep.package) {
                    Some(old) if !canonical_paths_better(&dep_path, old) => {}
                    _ => {
                        if c.paths.get(&dep.package) != Some(&dep_path) {
                            c.paths.insert(dep.package.clone(), dep_path);
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

    for constraints in c.constraints.values_mut() {
        *constraints = canonical_constraints(std::mem::take(constraints));
    }
    c
}

fn evidence_for(c: &Closure, package: &str) -> Vec<Evidence> {
    let mut out: Vec<Evidence> = c
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
        constraint_key(a.constraint)
            .cmp(&constraint_key(b.constraint))
            .then_with(|| a.path.cmp(&b.path))
    });
    out
}

struct Search<'a> {
    roots: &'a [Requirement],
    catalog: &'a BTreeMap<String, &'a Package>,
    target: Target,
    locks: &'a BTreeMap<String, Version>,
    attempts: usize,
    max_attempts: usize,
    best: Option<(usize, Vec<Version>, BTreeMap<String, Version>, Closure)>,
    catalog_names: Vec<String>,
    first_error: Option<ResolveError>,
}

impl Search<'_> {
    fn inspect(&mut self, chosen: &BTreeMap<String, Version>) -> Result<(), ResolveError> {
        let c = closure_for(self.roots, self.catalog, chosen, self.target);

        for package in c.constraints.keys() {
            if !self.catalog.contains_key(package) {
                return Err(ResolveError::Missing {
                    package: package.clone(),
                    evidence: evidence_for(&c, package),
                });
            }
        }

        for (package, version) in chosen {
            let Some(requirements) = c.constraints.get(package) else {
                continue;
            };
            if !requirements.iter().all(|r| matches(*version, *r)) {
                return Err(ResolveError::Conflict {
                    package: package.clone(),
                    selected: *version,
                    requirements: requirements.clone(),
                    evidence: evidence_for(&c, package),
                });
            }
        }

        for (package, version) in chosen {
            let Some(pkg) = self.catalog.get(package) else {
                continue;
            };
            let release = pkg.releases.iter().find(|r| r.version == *version).unwrap();
            let declared: BTreeSet<&str> =
                release.features.iter().map(|f| f.name.as_str()).collect();
            if let Some(feature) = c
                .activated
                .get(package)
                .into_iter()
                .flatten()
                .find(|f| !declared.contains(f.as_str()))
            {
                return Err(ResolveError::MissingFeature {
                    package: package.clone(),
                    selected: *version,
                    feature: feature.clone(),
                    path: c.paths.get(package).cloned().unwrap_or_default(),
                });
            }
        }

        for (package, requirements) in &c.constraints {
            if chosen.contains_key(package) {
                continue;
            }
            let pkg = self.catalog[package];
            if !pkg.releases.iter().any(|r| {
                requirements.iter().all(|constraint| matches(r.version, *constraint))
            }) {
                return Err(ResolveError::NoMatch {
                    package: package.clone(),
                    requirements: requirements.clone(),
                    evidence: evidence_for(&c, package),
                });
            }
        }

        let next = c
            .constraints
            .keys()
            .find(|package| !chosen.contains_key(*package))
            .cloned();
        let Some(package) = next else {
            let cost = self
                .locks
                .iter()
                .filter(|(name, version)| chosen.get(*name) != Some(*version))
                .count();
            let vector = self
                .catalog_names
                .iter()
                .map(|name| chosen.get(name).copied().unwrap_or(Version {
                    major: 0,
                    minor: 0,
                    patch: 0,
                }))
                .collect::<Vec<_>>();
            let replace = self.best.as_ref().is_none_or(|(bcost, bvec, ..)| {
                cost < *bcost || (cost == *bcost && vector > *bvec)
            });
            if replace {
                self.best = Some((cost, vector, chosen.clone(), c));
            }
            return Ok(());
        };

        let requirements = c.constraints[&package].clone();
        let pkg = self.catalog[&package];
        let mut candidates: Vec<Version> = pkg
            .releases
            .iter()
            .map(|r| r.version)
            .filter(|v| requirements.iter().all(|r| matches(*v, *r)))
            .collect();
        candidates.sort_by(|a, b| b.cmp(a));
        if let Some(lock) = self.locks.get(&package) {
            if let Some(i) = candidates.iter().position(|v| v == lock) {
                let v = candidates.remove(i);
                candidates.insert(0, v);
            }
        }

        let mut local_first_error = None;
        for candidate in candidates {
            if self.attempts == self.max_attempts {
                return Err(ResolveError::Limit {
                    attempts: self.max_attempts,
                });
            }
            self.attempts += 1;
            let mut child = chosen.clone();
            child.insert(package.clone(), candidate);
            match self.inspect(&child) {
                Ok(()) => {}
                Err(e @ ResolveError::Limit { .. }) => return Err(e),
                Err(e) => {
                    if local_first_error.is_none() {
                        local_first_error = Some(e);
                    }
                }
            }
        }
        if self.best.is_some() {
            Ok(())
        } else if let Some(e) = local_first_error {
            if self.first_error.is_none() {
                self.first_error = Some(e.clone());
            }
            Err(e)
        } else {
            Err(ResolveError::NoMatch {
                package,
                requirements,
                evidence: evidence_for(&c, &package),
            })
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
                site: format!("root[{i}]"),
                name: root.package.clone(),
            });
        }
        for (f, feature) in root.features.iter().enumerate() {
            if !valid_name(feature) {
                return Err(ResolveError::InvalidName {
                    site: format!("root[{i}].feature[{f}]"),
                    name: feature.clone(),
                });
            }
        }
    }
    for (i, package) in catalog.iter().enumerate() {
        if !valid_name(&package.name) {
            return Err(ResolveError::InvalidName {
                site: format!("catalog[{i}]"),
                name: package.name.clone(),
            });
        }
        for (j, release) in package.releases.iter().enumerate() {
            for (k, dep) in release.deps.iter().enumerate() {
                let base = format!("catalog[{i}].release[{j}].dep[{k}]");
                if !valid_name(&dep.package) {
                    return Err(ResolveError::InvalidName {
                        site: base,
                        name: dep.package.clone(),
                    });
                }
                if let Some(gate) = &dep.gate {
                    if !valid_name(gate) {
                        return Err(ResolveError::InvalidName {
                            site: format!("catalog[{i}].release[{j}].dep[{k}].gate"),
                            name: gate.clone(),
                        });
                    }
                }
                for (f, feature) in dep.features.iter().enumerate() {
                    if !valid_name(feature) {
                        return Err(ResolveError::InvalidName {
                            site: format!("catalog[{i}].release[{j}].dep[{k}].feature[{f}]"),
                            name: feature.clone(),
                        });
                    }
                }
            }
            for (f, feature) in release.features.iter().enumerate() {
                if !valid_name(&feature.name) {
                    return Err(ResolveError::InvalidName {
                        site: format!("catalog[{i}].release[{j}].feature[{f}]"),
                        name: feature.name.clone(),
                    });
                }
                for (e, enabled) in feature.enables.iter().enumerate() {
                    if !valid_name(enabled) {
                        return Err(ResolveError::InvalidName {
                            site: format!(
                                "catalog[{i}].release[{j}].feature[{f}].enable[{e}]"
                            ),
                            name: enabled.clone(),
                        });
                    }
                }
            }
        }
    }

    let mut names = BTreeSet::new();
    for package in catalog {
        if !names.insert(package.name.clone()) {
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
        if !valid_name(&lock.package) {
            return Err(ResolveError::InvalidName {
                site: format!("lock[{i}]"),
                name: lock.package.clone(),
            });
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

    let catalog_map: BTreeMap<String, &Package> = catalog
        .iter()
        .map(|p| (p.name.clone(), p))
        .collect();
    let lock_map: BTreeMap<String, Version> = locks
        .iter()
        .map(|l| (l.package.clone(), l.version))
        .collect();
    let mut catalog_names: Vec<String> = catalog.iter().map(|p| p.name.clone()).collect();
    catalog_names.sort();

    let mut search = Search {
        roots,
        catalog: &catalog_map,
        target,
        locks: &lock_map,
        attempts: 0,
        max_attempts,
        best: None,
        catalog_names,
        first_error: None,
    };

    match search.inspect(&BTreeMap::new()) {
        Err(ResolveError::Limit { attempts }) => return Err(ResolveError::Limit { attempts }),
        Err(error) => return Err(search.first_error.unwrap_or(error)),
        Ok(()) => {}
    }
    let Some((_, _, chosen, final_closure)) = search.best else {
        return Err(search.first_error.unwrap_or(ResolveError::NoMatch {
            package: String::new(),
            requirements: Vec::new(),
            evidence: Vec::new(),
        }));
    };

    let mut selected: Vec<Selection> = chosen
        .iter()
        .map(|(package, version)| Selection {
            package: package.clone(),
            version: *version,
        })
        .collect();
    selected.sort_by(|a, b| a.package.cmp(&b.package));

    let before: BTreeMap<String, Version> = lock_map;
    let after = chosen;
    let mut all: BTreeSet<String> = before.keys().cloned().collect();
    all.extend(after.keys().cloned());
    let mut changes = Vec::new();
    for package in all {
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
                } else if !final_closure
                    .constraints
                    .get(&package)
                    .into_iter()
                    .flatten()
                    .all(|c| matches(old, *c))
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