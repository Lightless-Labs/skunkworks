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
        && b.last() != Some(&b'-')
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
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

fn canonical(mut cs: Vec<Constraint>) -> Vec<Constraint> {
    cs.sort_by_key(|c| constraint_rank(*c));
    cs.dedup();
    cs
}

fn constraint_text(c: Constraint) -> String {
    match c {
        Constraint::Exact(v) => format!("={}", version_text(v)),
        Constraint::AtLeast(v) => format!(">={}", version_text(v)),
        Constraint::Before(v) => format!("<{}", version_text(v)),
        Constraint::Caret(v) => format!("^{}", version_text(v)),
    }
}

fn constraint_texts(cs: &[Constraint]) -> String {
    canonical(cs.to_vec())
        .iter()
        .map(|c| constraint_text(*c))
        .collect::<Vec<_>>()
        .join(",")
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

fn evidence_text(e: &Evidence) -> String {
    format!(
        "  C:{} via {}",
        constraint_text(e.constraint),
        e.path.join(" -> ")
    )
}

fn format_evidence(evidence: &[Evidence]) -> String {
    let mut sorted = evidence.to_vec();
    sorted.sort_by(|a, b| {
        constraint_rank(a.constraint)
            .cmp(&constraint_rank(b.constraint))
            .then(a.path.cmp(&b.path))
    });
    sorted
        .iter()
        .map(evidence_text)
        .map(|s| format!("\n{s}"))
        .collect()
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
            format!("missing|package={}", encode(package)) + &format_evidence(evidence)
        }
        ResolveError::NoMatch {
            package,
            requirements,
            evidence,
        } => format!(
            "no-match|package={}|constraints={}",
            encode(package),
            constraint_texts(requirements)
        ) + &format_evidence(evidence),
        ResolveError::Conflict {
            package,
            selected,
            requirements,
            evidence,
        } => format!(
            "conflict|package={}|selected={}|constraints={}",
            encode(package),
            version_text(*selected),
            constraint_texts(requirements)
        ) + &format_evidence(evidence),
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
        ResolveError::Limit { attempts } => format!("limit|attempts={attempts}"),
    }
}

fn satisfies(version: Version, c: Constraint) -> bool {
    match c {
        Constraint::Exact(v) => version == v,
        Constraint::AtLeast(v) => version >= v,
        Constraint::Before(v) => version < v,
        Constraint::Caret(v) => {
            version >= v
                && if v.major > 0 {
                    version.major == v.major
                } else if v.minor > 0 {
                    version.major == 0 && version.minor == v.minor
                } else {
                    version == v
                }
        }
    }
}

fn constraints_match(version: Version, cs: &[Constraint]) -> bool {
    cs.iter().all(|c| satisfies(version, *c))
}

fn feature_names(mut names: Vec<String>) -> Vec<String> {
    names.sort();
    names.dedup();
    names
}

fn feature_token(names: &[String]) -> String {
    format!("+{{{}}}", feature_names(names.to_vec()).join(","))
}

fn target_text(platform: Platform) -> &'static str {
    match platform {
        Platform::Any => "any",
        Platform::Linux => "linux",
        Platform::Mac => "mac",
        Platform::Windows => "windows",
    }
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

fn validate(
    catalog: &[Package],
    roots: &[Requirement],
    locks: &[Lock],
) -> Result<(), ResolveError> {
    for (i, root) in roots.iter().enumerate() {
        validate_name(format!("root[{i}]"), &root.package)?;
        for (f, feature) in root.features.iter().enumerate() {
            validate_name(format!("root[{i}].feature[{f}]"), feature)?;
        }
    }

    for (i, package) in catalog.iter().enumerate() {
        validate_name(format!("catalog[{i}]"), &package.name)?;
        for (j, release) in package.releases.iter().enumerate() {
            for (k, dep) in release.deps.iter().enumerate() {
                let base = format!("catalog[{i}].release[{j}].dep[{k}]");
                validate_name(base.clone(), &dep.package)?;
                if let Some(gate) = &dep.gate {
                    validate_name(format!("{base}.gate"), gate)?;
                }
                for (f, feature) in dep.features.iter().enumerate() {
                    validate_name(format!("{base}.feature[{f}]"), feature)?;
                }
            }
            for (f, feature) in release.features.iter().enumerate() {
                let base = format!("catalog[{i}].release[{j}].feature[{f}]");
                validate_name(base.clone(), &feature.name)?;
                for (e, enabled) in feature.enables.iter().enumerate() {
                    validate_name(format!("{base}.enable[{e}]"), enabled)?;
                }
            }
        }
    }

    let mut package_names = std::collections::BTreeSet::new();
    for package in catalog {
        if !package_names.insert(package.name.clone()) {
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
        validate_name(format!("lock[{i}]"), &lock.package)?;
    }

    let mut lock_names = std::collections::BTreeSet::new();
    for lock in locks {
        if !lock_names.insert(lock.package.clone()) {
            return Err(ResolveError::DuplicateLock {
                package: lock.package.clone(),
            });
        }
    }
    Ok(())
}

#[derive(Clone)]
struct Origin {
    constraint: Constraint,
    path: Vec<String>,
}

#[derive(Clone, Default)]
struct NodeInfo {
    constraints: std::collections::BTreeMap<String, Vec<Constraint>>,
    origins: std::collections::BTreeMap<String, Vec<Origin>>,
    requests: std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    activated: std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    paths: std::collections::BTreeMap<String, Vec<String>>,
}

fn path_better(a: &[String], b: &[String]) -> bool {
    a.len() < b.len() || (a.len() == b.len() && a < b)
}

fn insert_path(
    paths: &mut std::collections::BTreeMap<String, Vec<String>>,
    package: &str,
    path: Vec<String>,
) -> bool {
    match paths.get(package) {
        Some(old) if !path_better(&path, old) => false,
        _ => {
            paths.insert(package.to_string(), path);
            true
        }
    }
}

fn path_for(
    paths: &std::collections::BTreeMap<String, Vec<String>>,
    package: &str,
) -> Vec<String> {
    paths.get(package).cloned().unwrap_or_default()
}

fn make_node(
    catalog: &[Package],
    roots: &[Requirement],
    assigned: &std::collections::BTreeMap<String, Version>,
    target: Target,
) -> NodeInfo {
    let package_map: std::collections::BTreeMap<&str, &Package> =
        catalog.iter().map(|p| (p.name.as_str(), p)).collect();
    let mut info = NodeInfo::default();

    for (i, root) in roots.iter().enumerate() {
        let fs = feature_names(root.features.clone());
        let token = format!(
            "root[{i}]:{}@{}{}",
            root.package,
            constraint_text(root.constraint),
            feature_token(&fs)
        );
        info.requests
            .entry(root.package.clone())
            .or_default()
            .extend(fs);
        insert_path(&mut info.paths, &root.package, vec![token]);
    }

    // Track dependency edges by source and original dependency index. This
    // prevents repeated closure passes from duplicating constraints/evidence.
    let mut edges: std::collections::BTreeMap<(String, usize), (String, Origin)> =
        std::collections::BTreeMap::new();

    loop {
        let mut changed = false;

        // Every active selected package is scanned, including packages with
        // no feature requests. This is necessary for featureless dependency
        // chains to continue reaching their descendants.
        let active_packages: Vec<String> = info.paths.keys().cloned().collect();
        for package_name in active_packages {
            let Some(version) = assigned.get(&package_name) else {
                continue;
            };
            let Some(package) = package_map.get(package_name.as_str()) else {
                continue;
            };
            let Some(release) = package.releases.iter().find(|r| r.version == *version) else {
                continue;
            };

            let requested: Vec<String> = info
                .requests
                .get(&package_name)
                .map(|s| s.iter().cloned().collect())
                .unwrap_or_default();

            for name in requested {
                if info
                    .activated
                    .entry(package_name.clone())
                    .or_default()
                    .insert(name.clone())
                {
                    changed = true;
                }
                if let Some(feature) = release.features.iter().find(|f| f.name == name) {
                    for enabled in &feature.enables {
                        if info
                            .requests
                            .entry(package_name.clone())
                            .or_default()
                            .insert(enabled.clone())
                        {
                            changed = true;
                        }
                    }
                }
            }

            for (index, dep) in release.deps.iter().enumerate() {
                if !platform_matches(dep.platform, target) {
                    continue;
                }
                if dep.gate.as_ref().is_some_and(|gate| {
                    !info
                        .activated
                        .get(&package_name)
                        .is_some_and(|features| features.contains(gate))
                }) {
                    continue;
                }

                let fs = feature_names(dep.features.clone());
                let token = format!(
                    "{}@{}/deps[{}]:{}@{}{};gate={};platform={}",
                    package_name,
                    version_text(*version),
                    index,
                    dep.package,
                    constraint_text(dep.constraint),
                    feature_token(&fs),
                    dep.gate.as_deref().unwrap_or("*"),
                    target_text(dep.platform)
                );

                let mut incoming = path_for(&info.paths, &package_name);
                incoming.push(token);
                let key = (package_name.clone(), index);
                if !edges.contains_key(&key) {
                    edges.insert(
                        key,
                        (
                            dep.package.clone(),
                            Origin {
                                constraint: dep.constraint,
                                path: incoming.clone(),
                            },
                        ),
                    );
                    changed = true;
                } else if let Some((_, origin)) = edges.get_mut(&key) {
                    if origin.path != incoming {
                        origin.path = incoming.clone();
                        changed = true;
                    }
                }

                if insert_path(&mut info.paths, &dep.package, incoming) {
                    changed = true;
                }
                let requests = info.requests.entry(dep.package.clone()).or_default();
                for feature in fs {
                    if requests.insert(feature) {
                        changed = true;
                    }
                }
            }
        }

        if !changed {
            break;
        }
    }

    for (i, root) in roots.iter().enumerate() {
        let fs = feature_names(root.features.clone());
        let token = format!(
            "root[{i}]:{}@{}{}",
            root.package,
            constraint_text(root.constraint),
            feature_token(&fs)
        );
        info.constraints
            .entry(root.package.clone())
            .or_default()
            .push(root.constraint);
        info.origins
            .entry(root.package.clone())
            .or_default()
            .push(Origin {
                constraint: root.constraint,
                path: vec![token],
            });
    }

    for (_, (destination, origin)) in edges {
        info.constraints
            .entry(destination.clone())
            .or_default()
            .push(origin.constraint);
        info.origins.entry(destination).or_default().push(origin);
    }

    for constraints in info.constraints.values_mut() {
        *constraints = canonical(std::mem::take(constraints));
    }
    info
}

fn sorted_evidence(origins: &[Origin]) -> Vec<Evidence> {
    let mut out: Vec<Evidence> = origins
        .iter()
        .map(|o| Evidence {
            constraint: o.constraint,
            path: o.path.clone(),
        })
        .collect();
    out.sort_by(|a, b| {
        constraint_rank(a.constraint)
            .cmp(&constraint_rank(b.constraint))
            .then(a.path.cmp(&b.path))
    });
    out
}

struct SearchResult {
    selected: std::collections::BTreeMap<String, Version>,
    constraints: std::collections::BTreeMap<String, Vec<Constraint>>,
    cost: usize,
}

struct Search<'a> {
    catalog: &'a [Package],
    roots: &'a [Requirement],
    locks: &'a [Lock],
    target: Target,
    attempts: usize,
    budget: usize,
    first_error: Option<ResolveError>,
    best: Option<SearchResult>,
    catalog_names: Vec<String>,
}

impl<'a> Search<'a> {
    fn remember_error(&mut self, error: ResolveError) {
        if self.first_error.is_none() {
            self.first_error = Some(error);
        }
    }

    fn better(&self, candidate: &SearchResult, current: &SearchResult) -> bool {
        if candidate.cost != current.cost {
            return candidate.cost < current.cost;
        }
        for name in &self.catalog_names {
            match (candidate.selected.get(name), current.selected.get(name)) {
                (Some(a), Some(b)) if a != b => return a > b,
                (Some(_), None) => return true,
                (None, Some(_)) => return false,
                _ => {}
            }
        }
        false
    }

    fn solution_cost(
        &self,
        selected: &std::collections::BTreeMap<String, Version>,
    ) -> usize {
        self.locks
            .iter()
            .filter(|lock| selected.get(&lock.package) != Some(&lock.version))
            .count()
    }

    fn inspect(
        &mut self,
        assigned: &mut std::collections::BTreeMap<String, Version>,
    ) -> Result<(), ResolveError> {
        let info = make_node(self.catalog, self.roots, assigned, self.target);
        let package_map: std::collections::BTreeMap<&str, &Package> =
            self.catalog.iter().map(|p| (p.name.as_str(), p)).collect();
        let active: Vec<String> = info.constraints.keys().cloned().collect();

        for package in &active {
            if !package_map.contains_key(package.as_str()) {
                self.remember_error(ResolveError::Missing {
                    package: package.clone(),
                    evidence: info
                        .origins
                        .get(package)
                        .map(|o| sorted_evidence(o))
                        .unwrap_or_default(),
                });
                return Ok(());
            }
        }

        // MissingFeature precedes both Conflict and NoMatch.
        for package in &active {
            let Some(version) = assigned.get(package) else {
                continue;
            };
            let release = package_map[package.as_str()]
                .releases
                .iter()
                .find(|r| r.version == *version)
                .unwrap();
            if let Some(features) = info.activated.get(package) {
                for feature in features {
                    if !release.features.iter().any(|f| f.name == *feature) {
                        self.remember_error(ResolveError::MissingFeature {
                            package: package.clone(),
                            selected: *version,
                            feature: feature.clone(),
                            path: path_for(&info.paths, package),
                        });
                        return Ok(());
                    }
                }
            }
        }

        for package in &active {
            let Some(version) = assigned.get(package) else {
                continue;
            };
            let requirements = info.constraints.get(package).cloned().unwrap_or_default();
            if !constraints_match(*version, &requirements) {
                self.remember_error(ResolveError::Conflict {
                    package: package.clone(),
                    selected: *version,
                    requirements,
                    evidence: info
                        .origins
                        .get(package)
                        .map(|o| sorted_evidence(o))
                        .unwrap_or_default(),
                });
                return Ok(());
            }
        }

        for package in &active {
            if assigned.contains_key(package) {
                continue;
            }
            let requirements = info.constraints.get(package).cloned().unwrap_or_default();
            if !package_map[package.as_str()]
                .releases
                .iter()
                .any(|r| constraints_match(r.version, &requirements))
            {
                self.remember_error(ResolveError::NoMatch {
                    package: package.clone(),
                    requirements,
                    evidence: info
                        .origins
                        .get(package)
                        .map(|o| sorted_evidence(o))
                        .unwrap_or_default(),
                });
                return Ok(());
            }
        }

        let Some(package) = active
            .iter()
            .find(|name| !assigned.contains_key(*name))
            .cloned()
        else {
            let result = SearchResult {
                selected: assigned.clone(),
                constraints: info.constraints,
                cost: self.solution_cost(assigned),
            };
            if self
                .best
                .as_ref()
                .is_none_or(|current| self.better(&result, current))
            {
                self.best = Some(result);
            }
            return Ok(());
        };

        let requirements = info.constraints.get(&package).cloned().unwrap_or_default();
        let catalog_package = package_map[package.as_str()];
        let mut candidates: Vec<Version> = catalog_package
            .releases
            .iter()
            .map(|r| r.version)
            .filter(|v| constraints_match(*v, &requirements))
            .collect();
        candidates.sort_by(|a, b| b.cmp(a));

        if let Some(lock) = self.locks.iter().find(|l| l.package == package) {
            if let Some(position) = candidates.iter().position(|v| *v == lock.version) {
                let locked = candidates.remove(position);
                candidates.insert(0, locked);
            }
        }

        for candidate in candidates {
            if self.attempts == self.budget {
                return Err(ResolveError::Limit {
                    attempts: self.budget,
                });
            }
            self.attempts += 1;
            assigned.insert(package.clone(), candidate);
            let result = self.inspect(assigned);
            assigned.remove(&package);
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
    validate(catalog, roots, locks)?;

    let mut catalog_names: Vec<String> = catalog.iter().map(|p| p.name.clone()).collect();
    catalog_names.sort();
    catalog_names.dedup();

    let mut search = Search {
        catalog,
        roots,
        locks,
        target,
        attempts: 0,
        budget: max_attempts,
        first_error: None,
        best: None,
        catalog_names,
    };
    let mut assigned = std::collections::BTreeMap::new();
    search.inspect(&mut assigned)?;

    let Some(best) = search.best else {
        return Err(search.first_error.unwrap_or_else(|| ResolveError::NoMatch {
            package: String::new(),
            requirements: Vec::new(),
            evidence: Vec::new(),
        }));
    };

    let selected: Vec<Selection> = best
        .selected
        .iter()
        .map(|(package, version)| Selection {
            package: package.clone(),
            version: *version,
        })
        .collect();

    let lock_map: std::collections::BTreeMap<&str, Version> = locks
        .iter()
        .map(|l| (l.package.as_str(), l.version))
        .collect();
    let mut package_names: std::collections::BTreeSet<String> =
        lock_map.keys().map(|name| (*name).to_string()).collect();
    package_names.extend(best.selected.keys().cloned());

    let package_catalog: std::collections::BTreeMap<&str, &Package> =
        catalog.iter().map(|p| (p.name.as_str(), p)).collect();

    let mut changes = Vec::new();
    for package in package_names {
        let before = lock_map.get(package.as_str()).copied();
        let after = best.selected.get(&package).copied();
        if before == after {
            continue;
        }
        let reason = match (before, after) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old), Some(_)) => {
                let exists = package_catalog
                    .get(package.as_str())
                    .is_some_and(|p| p.releases.iter().any(|r| r.version == old));
                if !exists {
                    ChangeReason::Unavailable
                } else if !constraints_match(
                    old,
                    best.constraints
                        .get(&package)
                        .map(Vec::as_slice)
                        .unwrap_or(&[]),
                ) {
                    ChangeReason::Rejected
                } else {
                    ChangeReason::Rebalanced
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