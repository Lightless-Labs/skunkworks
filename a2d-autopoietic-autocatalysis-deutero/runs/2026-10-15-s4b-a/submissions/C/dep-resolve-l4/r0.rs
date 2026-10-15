use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

fn constraint_string(c: Constraint) -> String {
    match c {
        Constraint::Exact(v) => format!("={}.{}.{}", v.major, v.minor, v.patch),
        Constraint::AtLeast(v) => format!(">={}.{}.{}", v.major, v.minor, v.patch),
        Constraint::Before(v) => format!("<{}.{}.{}", v.major, v.minor, v.patch),
        Constraint::Caret(v) => format!("^{}.{}.{}", v.major, v.minor, v.patch),
    }
}

fn version_string(v: Version) -> String {
    format!("{}.{}.{}", v.major, v.minor, v.patch)
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

fn path_cmp(a: &[String], b: &[String]) -> std::cmp::Ordering {
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

fn evidence_cmp(a: &Evidence, b: &Evidence) -> std::cmp::Ordering {
    constraint_key(&a.constraint)
        .cmp(&constraint_key(&b.constraint))
        .then_with(|| a.path.cmp(&b.path))
}

fn evidence_lines(evidence: &[Evidence]) -> String {
    let mut es = evidence.to_vec();
    es.sort_by(evidence_cmp);
    let mut out = String::new();
    for e in es {
        out.push_str("\n  C:");
        out.push_str(&constraint_string(e.constraint));
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
            version_string(*version)
        ),
        ResolveError::DuplicateFeature { package, version, feature } => format!(
            "duplicate-feature|package={}|version={}|feature={}",
            encode(package),
            version_string(*version),
            encode(feature)
        ),
        ResolveError::DuplicateLock { package } => {
            format!("duplicate-lock|package={}", encode(package))
        }
        ResolveError::Missing { package, evidence } => {
            format!("missing|package={}{}", encode(package), evidence_lines(evidence))
        }
        ResolveError::NoMatch { package, requirements, evidence } => {
            let cs = canonical(requirements.clone())
                .iter()
                .map(|c| constraint_string(*c))
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "no-match|package={}|constraints={}{}",
                encode(package),
                cs,
                evidence_lines(evidence)
            )
        }
        ResolveError::Conflict { package, selected, requirements, evidence } => {
            let cs = canonical(requirements.clone())
                .iter()
                .map(|c| constraint_string(*c))
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "conflict|package={}|selected={}|constraints={}{}",
                encode(package),
                version_string(*selected),
                cs,
                evidence_lines(evidence)
            )
        }
        ResolveError::MissingFeature { package, selected, feature, path } => format!(
            "missing-feature|package={}|selected={}|feature={}{}",
            encode(package),
            version_string(*selected),
            encode(feature),
            format!("\n  F:{} via {}", encode(feature), path.join(" -> "))
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
struct State {
    selected: BTreeMap<String, Version>,
    origins: BTreeMap<String, Vec<Origin>>,
    requests: BTreeMap<String, BTreeSet<String>>,
    active_features: BTreeMap<String, BTreeSet<String>>,
    active_deps: BTreeMap<String, BTreeSet<usize>>,
    provenance: BTreeMap<String, Vec<String>>,
}

fn target_name(t: Target) -> &'static str {
    match t {
        Target::Linux => "linux",
        Target::Mac => "mac",
        Target::Windows => "windows",
    }
}

fn platform_matches(p: Platform, t: Target) -> bool {
    match p {
        Platform::Any => true,
        Platform::Linux => t == Target::Linux,
        Platform::Mac => t == Target::Mac,
        Platform::Windows => t == Target::Windows,
    }
}

fn feature_map(release: &Release) -> BTreeMap<&str, &Feature> {
    release.features.iter().map(|f| (f.name.as_str(), f)).collect()
}

fn root_token(i: usize, r: &Requirement) -> String {
    let mut fs = r.features.clone();
    fs.sort();
    fs.dedup();
    format!(
        "root[{}]:{}@{}+{{{}}}",
        i,
        r.package,
        constraint_string(r.constraint),
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
        version_string(version),
        j,
        d.package,
        constraint_string(d.constraint),
        fs.join(","),
        d.gate.as_deref().unwrap_or("*"),
        match d.platform {
            Platform::Any => "any",
            Platform::Linux => "linux",
            Platform::Mac => "mac",
            Platform::Windows => "windows",
        }
    )
}

fn compute_state(
    catalog: &BTreeMap<String, &Package>,
    roots: &[Requirement],
    selected: &BTreeMap<String, Version>,
    target: Target,
) -> State {
    let mut s = State {
        selected: selected.clone(),
        ..State::default()
    };

    for (i, r) in roots.iter().enumerate() {
        let token = root_token(i, r);
        s.origins.entry(r.package.clone()).or_default().push(Origin {
            constraint: r.constraint,
            path: vec![token.clone()],
        });
        s.requests
            .entry(r.package.clone())
            .or_default()
            .extend(r.features.iter().cloned());
        let p = vec![token];
        match s.provenance.get(&r.package) {
            None => {
                s.provenance.insert(r.package.clone(), p);
            }
            Some(old) if path_cmp(&p, old).is_lt() => {
                s.provenance.insert(r.package.clone(), p);
            }
            _ => {}
        }
    }

    loop {
        let mut changed = false;
        let package_names: Vec<String> = selected.keys().cloned().collect();
        for name in package_names {
            let Some(pkg) = catalog.get(&name) else { continue };
            let version = selected[&name];
            let Some(release) = pkg.releases.iter().find(|r| r.version == version) else { continue };
            let declarations = feature_map(release);
            let requested = s.requests.entry(name.clone()).or_default().clone();
            let active = s.active_features.entry(name.clone()).or_default();
            let mut queue: Vec<String> = requested.into_iter().collect();
            while let Some(f) = queue.pop() {
                if !active.insert(f.clone()) {
                    continue;
                }
                changed = true;
                if let Some(decl) = declarations.get(f.as_str()) {
                    for en in &decl.enables {
                        if !active.contains(en) {
                            queue.push(en.clone());
                        }
                    }
                }
            }
            let activated = s.active_features.get(&name).cloned().unwrap_or_default();
            for (j, dep) in release.deps.iter().enumerate() {
                let gate_ok = dep.gate.as_ref().map_or(true, |g| activated.contains(g));
                if !platform_matches(dep.platform, target) || !gate_ok {
                    continue;
                }
                if s.active_deps.entry(name.clone()).or_default().insert(j) {
                    changed = true;
                }
                let src_path = s.provenance.get(&name).cloned().unwrap_or_default();
                let mut path = src_path;
                path.push(dep_token(&name, version, j, dep));
                let origins = s.origins.entry(dep.package.clone()).or_default();
                if !origins.iter().any(|o| o.constraint == dep.constraint && o.path == path) {
                    origins.push(Origin { constraint: dep.constraint, path: path.clone() });
                    changed = true;
                }
                let dest_req = s.requests.entry(dep.package.clone()).or_default();
                for f in &dep.features {
                    if dest_req.insert(f.clone()) {
                        changed = true;
                    }
                }
                match s.provenance.get(&dep.package) {
                    None => {
                        s.provenance.insert(dep.package.clone(), path);
                        changed = true;
                    }
                    Some(old) if path_cmp(&path, old).is_lt() => {
                        s.provenance.insert(dep.package.clone(), path);
                        changed = true;
                    }
                    _ => {}
                }
            }
        }
        if !changed {
            break;
        }
    }

    s
}

fn active_constraints(state: &State) -> BTreeMap<String, Vec<Constraint>> {
    state
        .origins
        .iter()
        .map(|(p, os)| (p.clone(), canonical(os.iter().map(|o| o.constraint).collect())))
        .collect()
}

fn sorted_evidence(state: &State, package: &str) -> Vec<Evidence> {
    let mut es = state
        .origins
        .get(package)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|o| Evidence { constraint: o.constraint, path: o.path })
        .collect::<Vec<_>>();
    es.sort_by(evidence_cmp);
    es
}

fn dynamic_error(
    catalog: &BTreeMap<String, &Package>,
    state: &State,
) -> Option<ResolveError> {
    for package in state.origins.keys() {
        if !catalog.contains_key(package) {
            return Some(ResolveError::Missing {
                package: package.clone(),
                evidence: sorted_evidence(state, package),
            });
        }
    }

    for (package, version) in &state.selected {
        let cs = state.origins.get(package).map(|os| os.iter().map(|o| o.constraint).collect::<Vec<_>>()).unwrap_or_default();
        if cs.iter().any(|c| !matches(*version, *c)) {
            return Some(ResolveError::Conflict {
                package: package.clone(),
                selected: *version,
                requirements: canonical(cs),
                evidence: sorted_evidence(state, package),
            });
        }
    }

    for (package, requests) in &state.requests {
        if let Some(version) = state.selected.get(package) {
            if let Some(pkg) = catalog.get(package) {
                if let Some(release) = pkg.releases.iter().find(|r| r.version == *version) {
                    let declared: BTreeSet<&str> = release.features.iter().map(|f| f.name.as_str()).collect();
                    for feature in state.active_features.get(package).into_iter().flatten() {
                        if !declared.contains(feature.as_str()) {
                            return Some(ResolveError::MissingFeature {
                                package: package.clone(),
                                selected: *version,
                                feature: feature.clone(),
                                path: state.provenance.get(package).cloned().unwrap_or_default(),
                            });
                        }
                    }
                }
            }
        }
        let _ = requests;
    }

    let constraints = active_constraints(state);
    for package in state.origins.keys() {
        if state.selected.contains_key(package) {
            continue;
        }
        let cs = constraints.get(package).cloned().unwrap_or_default();
        let matches_any = catalog
            .get(package)
            .map(|p| p.releases.iter().any(|r| cs.iter().all(|c| matches(r.version, *c))))
            .unwrap_or(false);
        if !matches_any {
            return Some(ResolveError::NoMatch {
                package: package.clone(),
                requirements: cs,
                evidence: sorted_evidence(state, package),
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
    best: Option<(usize, Vec<(String, Version)>, State)>,
    first_error: Option<ResolveError>,
}

fn solution_cost(state: &State, locks: &BTreeMap<String, Version>) -> usize {
    locks.iter().filter(|(p, v)| state.selected.get(*p) != Some(*v)).count()
}

fn better_vector(
    a: &BTreeMap<String, Version>,
    b: &BTreeMap<String, Version>,
    names: &[String],
) -> std::cmp::Ordering {
    for n in names {
        match (a.get(n), b.get(n)) {
            (Some(x), Some(y)) => {
                let c = x.cmp(y);
                if !c.is_eq() { return c; }
            }
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (None, None) => {}
        }
    }
    std::cmp::Ordering::Equal
}

impl<'a> Search<'a> {
    fn visit(&mut self, selected: BTreeMap<String, Version>) -> Result<(), ResolveError> {
        let state = compute_state(self.catalog, self.roots, &selected, self.target);
        if let Some(err) = dynamic_error(self.catalog, &state) {
            if self.first_error.is_none() {
                self.first_error = Some(err);
            }
            return Ok(());
        }

        let next = state
            .origins
            .keys()
            .find(|p| !selected.contains_key(*p))
            .cloned();

        let Some(package) = next else {
            let cost = solution_cost(&state, self.locks);
            let mut names = self.catalog.keys().cloned().collect::<Vec<_>>();
            names.sort();
            let replace = match &self.best {
                None => true,
                Some((best_cost, _, best_state)) => {
                    cost < *best_cost
                        || (cost == *best_cost
                            && better_vector(&state.selected, &best_state.selected, &names).is_gt())
                }
            };
            if replace {
                let vector = state.selected.iter().map(|(p, v)| (p.clone(), *v)).collect();
                self.best = Some((cost, vector, state));
            }
            return Ok(());
        };

        let cs = state.origins.get(&package).map(|os| canonical(os.iter().map(|o| o.constraint).collect())).unwrap_or_default();
        let pkg = self.catalog[&package];
        let mut candidates = pkg
            .releases
            .iter()
            .filter(|r| cs.iter().all(|c| matches(r.version, *c)))
            .map(|r| r.version)
            .collect::<Vec<_>>();
        candidates.sort_by(|a, b| b.cmp(a));
        if let Some(lock) = self.locks.get(&package) {
            if let Some(pos) = candidates.iter().position(|v| v == lock) {
                let v = candidates.remove(pos);
                candidates.insert(0, v);
            }
        }

        if candidates.is_empty() {
            let err = ResolveError::NoMatch {
                package: package.clone(),
                requirements: cs,
                evidence: sorted_evidence(&state, &package),
            };
            if self.first_error.is_none() {
                self.first_error = Some(err);
            }
            return Ok(());
        }

        for candidate in candidates {
            if self.attempts == self.budget {
                return Err(ResolveError::Limit { attempts: self.budget });
            }
            self.attempts += 1;
            let mut child = selected.clone();
            child.insert(package.clone(), candidate);
            self.visit(child)?;
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
            return Err(ResolveError::InvalidName { site: format!("root[{}]", i), name: r.package.clone() });
        }
        for (f, name) in r.features.iter().enumerate() {
            if !valid_name(name) {
                return Err(ResolveError::InvalidName { site: format!("root[{}].feature[{}]", i, f), name: name.clone() });
            }
        }
    }

    for (i, p) in catalog.iter().enumerate() {
        if !valid_name(&p.name) {
            return Err(ResolveError::InvalidName { site: format!("catalog[{}]", i), name: p.name.clone() });
        }
        for (j, r) in p.releases.iter().enumerate() {
            for (k, d) in r.deps.iter().enumerate() {
                let base = format!("catalog[{}].release[{}].dep[{}]", i, j, k);
                if !valid_name(&d.package) {
                    return Err(ResolveError::InvalidName { site: base, name: d.package.clone() });
                }
                if let Some(g) = &d.gate {
                    if !valid_name(g) {
                        return Err(ResolveError::InvalidName { site: format!("{}.gate", base), name: g.clone() });
                    }
                }
                for (f, name) in d.features.iter().enumerate() {
                    if !valid_name(name) {
                        return Err(ResolveError::InvalidName { site: format!("{}.feature[{}]", base, f), name: name.clone() });
                    }
                }
            }
            for (f, feature) in r.features.iter().enumerate() {
                if !valid_name(&feature.name) {
                    return Err(ResolveError::InvalidName {
                        site: format!("catalog[{}].release[{}].feature[{}]", i, j, f),
                        name: feature.name.clone(),
                    });
                }
                for (e, enabled) in feature.enables.iter().enumerate() {
                    if !valid_name(enabled) {
                        return Err(ResolveError::InvalidName {
                            site: format!("catalog[{}].release[{}].feature[{}].enable[{}]", i, j, f, e),
                            name: enabled.clone(),
                        });
                    }
                }
            }
        }
    }

    let mut seen_packages = BTreeSet::new();
    for p in catalog {
        if !seen_packages.insert(p.name.clone()) {
            return Err(ResolveError::DuplicatePackage { package: p.name.clone() });
        }
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
                    return Err(ResolveError::DuplicateFeature {
                        package: p.name.clone(),
                        version: r.version,
                        feature: f.name.clone(),
                    });
                }
            }
        }
    }

    for (i, l) in locks.iter().enumerate() {
        if !valid_name(&l.package) {
            return Err(ResolveError::InvalidName { site: format!("lock[{}]", i), name: l.package.clone() });
        }
    }
    let mut seen_locks = BTreeSet::new();
    for l in locks {
        if !seen_locks.insert(l.package.clone()) {
            return Err(ResolveError::DuplicateLock { package: l.package.clone() });
        }
    }

    let catalog_map: BTreeMap<String, &Package> = catalog.iter().map(|p| (p.name.clone(), p)).collect();
    let lock_map: BTreeMap<String, Version> = locks.iter().map(|l| (l.package.clone(), l.version)).collect();

    let mut search = Search {
        catalog: &catalog_map,
        roots,
        locks: &lock_map,
        target,
        budget: max_attempts,
        attempts: 0,
        best: None,
        first_error: None,
    };
    search.visit(BTreeMap::new())?;
    let Some((_, _, state)) = search.best else {
        return Err(search.first_error.unwrap_or_else(|| ResolveError::NoMatch {
            package: String::new(),
            requirements: Vec::new(),
            evidence: Vec::new(),
        }));
    };

    let selected = state
        .selected
        .iter()
        .map(|(package, version)| Selection { package: package.clone(), version: *version })
        .collect::<Vec<_>>();

    let mut all_names = BTreeSet::new();
    all_names.extend(lock_map.keys().cloned());
    all_names.extend(state.selected.keys().cloned());
    let mut changes = Vec::new();
    for package in all_names {
        let before = lock_map.get(&package).copied();
        let after = state.selected.get(&package).copied();
        if before == after {
            continue;
        }
        let reason = match (before, after) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old), Some(_)) => {
                let exists = catalog_map
                    .get(&package)
                    .map(|p| p.releases.iter().any(|r| r.version == old))
                    .unwrap_or(false);
                if !exists {
                    ChangeReason::Unavailable
                } else {
                    let cs = state.origins.get(&package).map(|os| os.iter().map(|o| o.constraint).collect::<Vec<_>>()).unwrap_or_default();
                    if cs.iter().any(|c| !matches(old, *c)) {
                        ChangeReason::Rejected
                    } else {
                        ChangeReason::Rebalanced
                    }
                }
            }
            (None, None) => continue,
        };
        changes.push(Change { package, before, after, reason });
    }

    Ok(Resolution { selected, changes })
}

fn main() {}