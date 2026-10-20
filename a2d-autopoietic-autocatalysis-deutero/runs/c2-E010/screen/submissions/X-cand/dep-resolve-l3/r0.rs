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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Requirement {
    pub package: String,
    pub constraint: Constraint,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    pub deps: Vec<Requirement>,
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
    DuplicateLock {
        package: String,
    },
    Missing {
        package: String,
    },
    NoMatch {
        package: String,
        requirements: Vec<Constraint>,
    },
    Conflict {
        package: String,
        selected: Version,
        requirements: Vec<Constraint>,
    },
    Limit {
        attempts: usize,
    },
}

fn valid_name(name: &str) -> bool {
    let b = name.as_bytes();
    !b.is_empty()
        && b.len() <= 24
        && b[0].is_ascii_lowercase()
        && b.last() != Some(&b'-')
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

fn matches(c: Constraint, v: Version) -> bool {
    match c {
        Constraint::Exact(x) => v == x,
        Constraint::AtLeast(x) => v >= x,
        Constraint::Before(x) => v < x,
        Constraint::Caret(x) => {
            v >= x
                && if x.major != 0 {
                    v.major == x.major
                } else if x.minor != 0 {
                    v.major == 0 && v.minor == x.minor
                } else {
                    v == x
                }
        }
    }
}

fn canon(cs: &[Constraint]) -> Vec<Constraint> {
    let mut result = cs.to_vec();
    result.sort();
    result.dedup();
    result
}

fn vstr(v: Version) -> String {
    format!("{}.{}.{}", v.major, v.minor, v.patch)
}

fn cstr(c: Constraint) -> String {
    match c {
        Constraint::Exact(v) => format!("={}", vstr(v)),
        Constraint::AtLeast(v) => format!(">={}", vstr(v)),
        Constraint::Before(v) => format!("<{}", vstr(v)),
        Constraint::Caret(v) => format!("^{}", vstr(v)),
    }
}

fn enc(s: &str) -> String {
    let mut result = String::new();
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-' => {
                result.push(byte as char)
            }
            _ => result.push_str(&format!("%{:02X}", byte)),
        }
    }
    result
}

pub fn error_text(e: &ResolveError) -> String {
    match e {
        ResolveError::InvalidName { site, name } => {
            format!("invalid-name|site={}|name={}", enc(site), enc(name))
        }
        ResolveError::DuplicatePackage { package } => {
            format!("duplicate-package|package={}", enc(package))
        }
        ResolveError::DuplicateVersion { package, version } => format!(
            "duplicate-version|package={}|version={}",
            enc(package),
            vstr(*version)
        ),
        ResolveError::DuplicateLock { package } => {
            format!("duplicate-lock|package={}", enc(package))
        }
        ResolveError::Missing { package } => format!("missing|package={}", enc(package)),
        ResolveError::NoMatch {
            package,
            requirements,
        } => format!(
            "no-match|package={}|constraints={}",
            enc(package),
            canon(requirements).iter().map(|c| cstr(*c)).collect::<Vec<_>>().join(",")
        ),
        ResolveError::Conflict {
            package,
            selected,
            requirements,
        } => format!(
            "conflict|package={}|selected={}|constraints={}",
            enc(package),
            vstr(*selected),
            canon(requirements).iter().map(|c| cstr(*c)).collect::<Vec<_>>().join(",")
        ),
        ResolveError::Limit { attempts } => format!("limit|attempts={}", attempts),
    }
}

fn validate(c: &[Package], r: &[Requirement], l: &[Lock]) -> Result<(), ResolveError> {
    for (i, root) in r.iter().enumerate() {
        if !valid_name(&root.package) {
            return Err(ResolveError::InvalidName {
                site: format!("root[{}]", i),
                name: root.package.clone(),
            });
        }
    }
    for (i, pkg) in c.iter().enumerate() {
        if !valid_name(&pkg.name) {
            return Err(ResolveError::InvalidName {
                site: format!("catalog[{}]", i),
                name: pkg.name.clone(),
            });
        }
        for (j, rel) in pkg.releases.iter().enumerate() {
            for (k, dep) in rel.deps.iter().enumerate() {
                if !valid_name(&dep.package) {
                    return Err(ResolveError::InvalidName {
                        site: format!("catalog[{}].release[{}].dep[{}]", i, j, k),
                        name: dep.package.clone(),
                    });
                }
            }
        }
    }

    let mut package_names = BTreeSet::new();
    for pkg in c {
        if !package_names.insert(pkg.name.clone()) {
            return Err(ResolveError::DuplicatePackage {
                package: pkg.name.clone(),
            });
        }
    }
    for pkg in c {
        let mut versions = BTreeSet::new();
        for rel in &pkg.releases {
            if !versions.insert(rel.version) {
                return Err(ResolveError::DuplicateVersion {
                    package: pkg.name.clone(),
                    version: rel.version,
                });
            }
        }
    }
    for (i, lock) in l.iter().enumerate() {
        if !valid_name(&lock.package) {
            return Err(ResolveError::InvalidName {
                site: format!("lock[{}]", i),
                name: lock.package.clone(),
            });
        }
    }
    let mut locks = BTreeSet::new();
    for lock in l {
        if !locks.insert(lock.package.clone()) {
            return Err(ResolveError::DuplicateLock {
                package: lock.package.clone(),
            });
        }
    }
    Ok(())
}

type Chosen = BTreeMap<String, Version>;
type RequirementsByName = BTreeMap<String, Vec<Constraint>>;

fn collect(
    roots: &[Requirement],
    chosen: &Chosen,
    packages: &BTreeMap<String, &Package>,
) -> RequirementsByName {
    let mut map = RequirementsByName::new();
    for root in roots {
        map.entry(root.package.clone()).or_default().push(root.constraint);
    }
    for (name, version) in chosen {
        if let Some(package) = packages.get(name) {
            if let Some(release) = package.releases.iter().find(|x| x.version == *version) {
                for dep in &release.deps {
                    map.entry(dep.package.clone()).or_default().push(dep.constraint);
                }
            }
        }
    }
    for cs in map.values_mut() {
        *cs = canon(cs);
    }
    map
}

struct Resolver<'a> {
    roots: &'a [Requirement],
    packages: BTreeMap<String, &'a Package>,
    locks: BTreeMap<String, Version>,
    names: Vec<String>,
    budget: usize,
    used: usize,
    best: Option<(usize, Vec<Option<Version>>, Chosen, RequirementsByName)>,
    first_leaf: Option<ResolveError>,
}

impl<'a> Resolver<'a> {
    fn leaf(&mut self, e: ResolveError) {
        if self.first_leaf.is_none() {
            self.first_leaf = Some(e);
        }
    }

    fn visit(&mut self, chosen: &mut Chosen) -> Result<(), ResolveError> {
        let reqs = collect(self.roots, chosen, &self.packages);

        if let Some(name) = reqs.keys().find(|name| !self.packages.contains_key(*name)) {
            return Err(ResolveError::Missing {
                package: name.clone(),
            });
        }

        for (name, version) in chosen.iter() {
            if let Some(all) = reqs.get(name) {
                if !all.iter().all(|c| matches(*c, *version)) {
                    return Err(ResolveError::Conflict {
                        package: name.clone(),
                        selected: *version,
                        requirements: all.clone(),
                    });
                }
            }
        }

        for (name, all) in &reqs {
            if chosen.contains_key(name) {
                continue;
            }
            let pkg = self.packages[name];
            if !pkg.releases.iter().any(|rel| {
                all.iter().all(|c| matches(*c, rel.version))
            }) {
                return Err(ResolveError::NoMatch {
                    package: name.clone(),
                    requirements: all.clone(),
                });
            }
        }

        let pending = reqs.keys().find(|name| !chosen.contains_key(*name)).cloned();
        let Some(name) = pending else {
            let cost = self
                .locks
                .iter()
                .filter(|(name, version)| chosen.get(*name) != Some(*version))
                .count();
            let vector = self.names.iter().map(|n| chosen.get(n).copied()).collect::<Vec<_>>();
            let better = self.best.as_ref().map_or(true, |(bc, bv, _, _)| {
                cost < *bc || (cost == *bc && vector > *bv)
            });
            if better {
                self.best = Some((cost, vector, chosen.clone(), reqs));
            }
            return Ok(());
        };

        let all = reqs.get(&name).cloned().unwrap_or_default();
        let mut versions = self.packages[&name]
            .releases
            .iter()
            .filter(|rel| all.iter().all(|c| matches(*c, rel.version)))
            .map(|rel| rel.version)
            .collect::<Vec<_>>();
        versions.sort_by(|a, b| b.cmp(a));
        if let Some(lock) = self.locks.get(&name) {
            if let Some(pos) = versions.iter().position(|v| v == lock) {
                let locked = versions.remove(pos);
                versions.insert(0, locked);
            }
        }

        for version in versions {
            if self.used == self.budget {
                return Err(ResolveError::Limit {
                    attempts: self.budget,
                });
            }
            self.used += 1;
            chosen.insert(name.clone(), version);
            match self.visit(chosen) {
                Ok(()) => {}
                Err(e @ ResolveError::Limit { .. }) => {
                    chosen.remove(&name);
                    return Err(e);
                }
                Err(e) => self.leaf(e),
            }
            chosen.remove(&name);
        }
        Ok(())
    }
}

pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
    locks: &[Lock],
    max_attempts: usize,
) -> Result<Resolution, ResolveError> {
    validate(catalog, roots, locks)?;
    let packages = catalog
        .iter()
        .map(|p| (p.name.clone(), p))
        .collect::<BTreeMap<_, _>>();
    let locks_map = locks
        .iter()
        .map(|l| (l.package.clone(), l.version))
        .collect::<BTreeMap<_, _>>();
    let mut names = catalog.iter().map(|p| p.name.clone()).collect::<Vec<_>>();
    names.sort();

    let mut resolver = Resolver {
        roots,
        packages,
        locks: locks_map,
        names,
        budget: max_attempts,
        used: 0,
        best: None,
        first_leaf: None,
    };
    let mut chosen = Chosen::new();
    resolver.visit(&mut chosen)?;

    let Some((_, _, chosen, final_reqs)) = resolver.best else {
        if let Some(error) = resolver.first_leaf {
            return Err(error);
        }
        return Err(ResolveError::NoMatch {
            package: roots.first().map(|r| r.package.clone()).unwrap_or_default(),
            requirements: Vec::new(),
        });
    };

    let selected = chosen
        .iter()
        .map(|(package, version)| Selection {
            package: package.clone(),
            version: *version,
        })
        .collect::<Vec<_>>();

    let before = locks
        .iter()
        .map(|l| (l.package.clone(), l.version))
        .collect::<BTreeMap<_, _>>();
    let all_names = before
        .keys()
        .chain(chosen.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut changes = Vec::new();

    for package in all_names {
        let old = before.get(&package).copied();
        let new = chosen.get(&package).copied();
        if old == new {
            continue;
        }
        let reason = match (old, new) {
            (None, Some(_)) => ChangeReason::Added,
            (Some(_), None) => ChangeReason::Removed,
            (Some(old_version), Some(_)) => {
                let available = resolver
                    .packages
                    .get(&package)
                    .map_or(false, |pkg| pkg.releases.iter().any(|r| r.version == old_version));
                if !available {
                    ChangeReason::Unavailable
                } else if final_reqs.get(&package).map_or(false, |all| {
                    !all.iter().all(|c| matches(*c, old_version))
                }) {
                    ChangeReason::Rejected
                } else {
                    ChangeReason::Rebalanced
                }
            }
            (None, None) => continue,
        };
        changes.push(Change {
            package,
            before: old,
            after: new,
            reason,
        });
    }

    Ok(Resolution { selected, changes })
}
