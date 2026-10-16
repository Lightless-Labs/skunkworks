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
        .into_iter()
        .map(constraint_text)
        .collect::<Vec<_>>()
        .join(",")
}

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(*b, b'.' | b'_' | b'-') {
            out.push(*b as char);
        } else {
            out.push('%');
            out.push_str(&format!("{:02X}", b));
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

fn append_evidence(out: &mut String, evidence: &[Evidence]) {
    for e in evidence_sorted(evidence) {
        out.push_str("\n  C:");
        out.push_str(&constraint_text(e.constraint));
        out.push_str(" via ");
        out.push_str(&e.path.join(" -> "));
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

fn valid_name(name: &str) -> bool {
    let b = name.as_bytes();
    if b.is_empty() || b.len() > 24 || !b[0].is_ascii_lowercase() || b[b.len() - 1] == b'-' {
        return false;
    }
    b.iter()
        .all(|x| x.is_ascii_lowercase() || x.is_ascii_digit() || *x == b'-')
}

fn check_name(name: &str, site: String) -> Result<(), ResolveError> {
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

#[derive(Clone, Debug)]
struct Origin {
    constraint: Constraint,
    path: Vec<String>,
}

#[derive(Clone, Debug)]
struct Closure {
    constraints: BTreeMap<String, Vec<Constraint>>,
    origins: BTreeMap<String, Vec<Origin>>,
    requested: BTreeMap<String, BTreeSet<String>>,
    activated: BTreeMap<String, BTreeSet<String>>,
    paths: BTreeMap<String, Vec<String>>,
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

fn dep_token(
    source: &str,
    version: Version,
    j: usize,
    dep: &Dependency,
) -> String {
    let mut fs = dep.features.clone();
    fs.sort();
    fs.dedup();
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
        j,
        dep.package,
        constraint_text(dep.constraint),
        fs.join(","),
        gate,
        platform
    )
}

fn platform_active(platform: Platform, target: Target) -> bool {
    match platform {
        Platform::Any => true,
        Platform::Linux => target == Target::Linux,
        Platform::Mac => target == Target::Mac,
        Platform::Windows => target == Target::Windows,
    }
}

fn path_better(a: &[String], b: &[String]) -> bool {
    a.len() < b.len() || (a.len() == b.len() && a < b)
}

fn compute_closure(
    packages: &BTreeMap<String, &Package>,
    roots: &[Requirement],
    selected: &BTreeMap<String, Version>,
    target: Target,
) -> Closure {
    let mut c = Closure {
        constraints: BTreeMap::new(),
        origins: BTreeMap::new(),
        requested: BTreeMap::new(),
        activated: BTreeMap::new(),
        paths: BTreeMap::new(),
    };

    for (i, root) in roots.iter().enumerate() {
        let path = vec![root_token(i, root)];
        c.constraints
            .entry(root.package.clone())
            .or_default()
            .push(root.constraint);
        c.origins
            .entry(root.package.clone())
            .or_default()
            .push(Origin {
                constraint: root.constraint,
                path: path.clone(),
            });
        c.requested
            .entry(root.package.clone())
            .or_default()
            .extend(root.features.iter().cloned());
        match c.paths.get(&root.package) {
            Some(old) if !path_better(&path, old) => {}
            _ => {
                c.paths.insert(root.package.clone(), path);
            }
        }
    }

    loop {
        let mut changed = false;

        let selected_names: Vec<String> = selected.keys().cloned().collect();
        for name in &selected_names {
            let Some(package) = packages.get(name) else {
                continue;
            };
            let Some(version) = selected.get(name).copied() else {
                continue;
            };
            let Some(release) = package.releases.iter().find(|r| r.version == version) else {
                continue;
            };

            let mut queue: Vec<String> = c
                .requested
                .get(name)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect();
            let mut done = c.activated.entry(name.clone()).or_default().clone();

            while let Some(feature_name) = queue.pop() {
                if !done.insert(feature_name.clone()) {
                    continue;
                }
                changed = true;
                if let Some(feature) = release.features.iter().find(|f| f.name == feature_name) {
                    for enabled in &feature.enables {
                        if !done.contains(enabled) {
                            queue.push(enabled.clone());
                        }
                    }
                }
            }
            c.activated.insert(name.clone(), done);
        }

        for name in &selected_names {
            let Some(package) = packages.get(name) else {
                continue;
            };
            let Some(version) = selected.get(name).copied() else {
                continue;
            };
            let Some(release) = package.releases.iter().find(|r| r.version == version) else {
                continue;
            };
            let activated = c.activated.get(name).cloned().unwrap_or_default();
            let source_path = c.paths.get(name).cloned().unwrap_or_default();

            for (j, dep) in release.deps.iter().enumerate() {
                if !platform_active(dep.platform, target)
                    || dep.gate.as_ref().is_some_and(|g| !activated.contains(g))
                {
                    continue;
                }

                let token = dep_token(name, version, j, dep);
                let mut path = source_path.clone();
                path.push(token);

                let origins = c.origins.entry(dep.package.clone()).or_default();
                if !origins.iter().any(|o| o.constraint == dep.constraint && o.path == path) {
                    origins.push(Origin {
                        constraint: dep.constraint,
                        path: path.clone(),
                    });
                    changed = true;
                }

                let constraints = c.constraints.entry(dep.package.clone()).or_default();
                if !constraints.contains(&dep.constraint) {
                    constraints.push(dep.constraint);
                    changed = true;
                }

                let requests = c.requested.entry(dep.package.clone()).or_default();
                for f in &dep.features {
                    if requests.insert(f.clone()) {
                        changed = true;
                    }
                }

                match c.paths.get(&dep.package) {
                    Some(old) if !path_better(&path, old) => {}
                    _ => {
                        c.paths.insert(dep.package.clone(), path);
                        changed = true;
                    }
                }
            }
        }

        if !changed {
            break;
        }
    }

    for cs in c.constraints.values_mut() {
        *cs = canonical_constraints(cs);
    }
    c
}

fn evidence_for(c: &Closure, name: &str) -> Vec<Evidence> {
    let mut out: Vec<Evidence> = c
        .origins
        .get(name)
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

fn requirements_for(c: &Closure, name: &str) -> Vec<Constraint> {
    canonical_constraints(c.constraints.get(name).map(Vec::as_slice).unwrap_or(&[]))
}

struct Search<'a> {
    packages: &'a BTreeMap<String, &'a Package>,
    roots: &'a [Requirement],
    locks: &'a BTreeMap<String, Version>,
    target: Target,
    max_attempts: usize,
    attempts: usize,
    best: Option<(usize, BTreeMap<String, Version>, Closure)>,
    first_error: Option<ResolveError>,
}

impl<'a> Search<'a> {
    fn record_leaf_error(&mut self, e: ResolveError) {
        if self.first_error.is_none() {
            self.first_error = Some(e);
        }
    }

    fn objective(&self, selection: &BTreeMap<String, Version>) -> usize {
        self.locks
            .iter()
            .filter(|(name, v)| selection.get(*name) != Some(*v))
            .count()
    }

    fn vector_better(
        &self,
        a: &BTreeMap<String, Version>,
        b: &BTreeMap<String, Version>,
    ) -> bool {
        for name in self.packages.keys() {
            match (a.get(name), b.get(name)) {
                (Some(x), Some(y)) if x != y => return x > y,
                (Some(_), None) => return true,
                (None, Some(_)) => return false,
                _ => {}
            }
        }
        false
    }

    fn visit(&mut self, selected: &mut BTreeMap<String, Version>) -> Result<(), ResolveError> {
        let closure = compute_closure(self.packages, self.roots, selected, self.target);

        if let Some(name) = closure
            .constraints
            .keys()
            .filter(|n| !self.packages.contains_key(*n))
            .next()
            .cloned()
        {
            self.record_leaf_error(ResolveError::Missing {
                package: name.clone(),
                evidence: evidence_for(&closure, &name),
            });
            return Ok(());
        }

        for (name, version) in selected.iter() {
            let cs = closure.constraints.get(name).map(Vec::as_slice).unwrap_or(&[]);
            if !cs.iter().all(|c| matches(*c, *version)) {
                self.record_leaf_error(ResolveError::Conflict {
                    package: name.clone(),
                    selected: *version,
                    requirements: requirements_for(&closure, name),
                    evidence: evidence_for(&closure, name),
                });
                return Ok(());
            }
        }

        let unselected: Vec<String> = closure
            .constraints
            .keys()
            .filter(|n| !selected.contains_key(*n))
            .cloned()
            .collect();

        for name in &unselected {
            let cs = closure.constraints.get(name).map(Vec::as_slice).unwrap_or(&[]);
            let package = self.packages.get(name).unwrap();
            let any = package
                .releases
                .iter()
                .any(|r| cs.iter().all(|c| matches(*c, r.version)));
            if !any {
                self.record_leaf_error(ResolveError::NoMatch {
                    package: name.clone(),
                    requirements: requirements_for(&closure, name),
                    evidence: evidence_for(&closure, name),
                });
                return Ok(());
            }
        }

        for name in selected.keys() {
            let package = self.packages.get(name).unwrap();
            let version = selected[name];
            let release = package.releases.iter().find(|r| r.version == version).unwrap();
            let activated = closure.activated.get(name).cloned().unwrap_or_default();
            for feature in activated {
                if !release.features.iter().any(|f| f.name == feature) {
                    self.record_leaf_error(ResolveError::MissingFeature {
                        package: name.clone(),
                        selected: version,
                        feature,
                        path: closure.paths.get(name).cloned().unwrap_or_default(),
                    });
                    return Ok(());
                }
            }
        }

        if unselected.is_empty() {
            let cost = self.objective(selected);
            let replace = match &self.best {
                None => true,
                Some((best_cost, best_selection, _)) => {
                    cost < *best_cost
                        || (cost == *best_cost && self.vector_better(selected, best_selection))
                }
            };
            if replace {
                self.best = Some((cost, selected.clone(), closure));
            }
            return Ok(());
        }

        let name = &unselected[0];
        let package = self.packages.get(name).unwrap();
        let cs = closure.constraints.get(name).map(Vec::as_slice).unwrap_or(&[]);
        let mut candidates: Vec<Version> = package
            .releases
            .iter()
            .map(|r| r.version)
            .filter(|v| cs.iter().all(|c| matches(*c, *v)))
            .collect();
        candidates.sort_by(|a, b| b.cmp(a));

        if let Some(lock_version) = self.locks.get(name) {
            if let Some(i) = candidates.iter().position(|v| v == lock_version) {
                let v = candidates.remove(i);
                candidates.insert(0, v);
            }
        }

        for version in candidates {
            if self.attempts == self.max_attempts {
                return Err(ResolveError::Limit {
                    attempts: self.max_attempts,
                });
            }
            self.attempts += 1;
            selected.insert(name.clone(), version);
            let result = self.visit(selected);
            selected.remove(name);
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
    for (i, root) in roots.iter().enumerate() {
        check_name(&root.package, format!("root[{}]", i))?;
        for (f, feature) in root.features.iter().enumerate() {
            check_name(feature, format!("root[{}].feature[{}]", i, f))?;
        }
    }
    for (i, package) in catalog.iter().enumerate() {
        check_name(&package.name, format!("catalog[{}]", i))?;
        for (j, release) in package.releases.iter().enumerate() {
            for (k, dep) in release.deps.iter().enumerate() {
                check_name(
                    &dep.package,
                    format!("catalog[{}].release[{}].dep[{}]", i, j, k),
                )?;
                if let Some(gate) = &dep.gate {
                    check_name(
                        gate,
                        format!("catalog[{}].release[{}].dep[{}].gate", i, j, k),
                    )?;
                }
                for (f, feature) in dep.features.iter().enumerate() {
                    check_name(
                        feature,
                        format!("catalog[{}].release[{}].dep[{}].feature[{}]", i, j, k, f),
                    )?;
                }
            }
            for (f, feature) in release.features.iter().enumerate() {
                check_name(
                    &feature.name,
                    format!("catalog[{}].release[{}].feature[{}]", i, j, f),
                )?;
                for (e, enabled) in feature.enables.iter().enumerate() {
                    check_name(
                        enabled,
                        format!(
                            "catalog[{}].release[{}].feature[{}].enable[{}]",
                            i, j, f, e
                        ),
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
        check_name(&lock.package, format!("lock[{}]", i))?;
    }
    let mut seen_locks = BTreeSet::new();
    for lock in locks {
        if !seen_locks.insert(lock.package.clone()) {
            return Err(ResolveError::DuplicateLock {
                package: lock.package.clone(),
            });
        }
    }

    let packages: BTreeMap<String, &Package> = catalog
        .iter()
        .map(|p| (p.name.clone(), p))
        .collect();
    let lock_map: BTreeMap<String, Version> = locks
        .iter()
        .map(|l| (l.package.clone(), l.version))
        .collect();

    let mut search = Search {
        packages: &packages,
        roots,
        locks: &lock_map,
        target,
        max_attempts,
        attempts: 0,
        best: None,
        first_error: None,
    };
    search.visit(&mut BTreeMap::new())?;

    let Some((_, chosen, final_closure)) = search.best else {
        return Err(search.first_error.unwrap_or(ResolveError::NoMatch {
            package: String::new(),
            requirements: Vec::new(),
            evidence: Vec::new(),
        }));
    };

    let selected: Vec<Selection> = chosen
        .iter()
        .map(|(package, version)| Selection {
            package: package.clone(),
            version: *version,
        })
        .collect();

    let mut all_names: BTreeSet<String> = lock_map.keys().cloned().collect();
    all_names.extend(chosen.keys().cloned());
    let mut changes = Vec::new();

    for name in all_names {
        let before = lock_map.get(&name).copied();
        let after = chosen.get(&name).copied();
        if before == after {
            continue;
        }
        let reason = match (before, after) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old), Some(_)) => {
                let exists = packages
                    .get(&name)
                    .is_some_and(|p| p.releases.iter().any(|r| r.version == old));
                if !exists {
                    ChangeReason::Unavailable
                } else {
                    let cs = final_closure
                        .constraints
                        .get(&name)
                        .map(Vec::as_slice)
                        .unwrap_or(&[]);
                    if !cs.iter().all(|c| matches(*c, old)) {
                        ChangeReason::Rejected
                    } else {
                        ChangeReason::Rebalanced
                    }
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

    Ok(Resolution { selected, changes })
}