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
    let bytes = name.as_bytes();
    if bytes.is_empty() || bytes.len() > 24 || !bytes[0].is_ascii_lowercase() {
        return false;
    }
    if bytes.iter().any(|b| !(b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')) {
        return false;
    }
    bytes.last().copied() != Some(b'-')
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

fn canonical_constraints(mut cs: Vec<Constraint>) -> Vec<Constraint> {
    cs.sort();
    cs.dedup();
    cs
}

fn constraints_text(cs: &[Constraint]) -> String {
    let mut cs = canonical_constraints(cs.to_vec());
    cs.drain(..).map(constraint_text).collect::<Vec<_>>().join(",")
}

fn encode_text(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        if b.is_ascii_alphabetic() || b.is_ascii_digit() || matches!(*b, b'.' | b'_' | b'-') {
            out.push(*b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

pub fn error_text(error: &ResolveError) -> String {
    match error {
        ResolveError::InvalidName { site, name } => {
            format!("invalid-name|site={}|name={}", encode_text(site), encode_text(name))
        }
        ResolveError::DuplicatePackage { package } => {
            format!("duplicate-package|package={}", encode_text(package))
        }
        ResolveError::DuplicateVersion { package, version } => {
            format!("duplicate-version|package={}|version={}", encode_text(package), version_text(*version))
        }
        ResolveError::Missing { package } => {
            format!("missing|package={}", encode_text(package))
        }
        ResolveError::NoMatch { package, requirements } => {
            format!("no-match|package={}|constraints={}", encode_text(package), constraints_text(requirements))
        }
        ResolveError::Conflict { package, selected, requirements } => {
            format!("conflict|package={}|selected={}|constraints={}", encode_text(package), version_text(*selected), constraints_text(requirements))
        }
        ResolveError::Limit { attempts } => format!("limit|attempts={}", attempts),
    }
}

fn matches_constraint(v: Version, c: Constraint) -> bool {
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

fn matches_all(v: Version, cs: &[Constraint]) -> bool {
    cs.iter().all(|c| matches_constraint(v, *c))
}

struct Resolver<'a> {
    catalog: &'a [Package],
    package_index: std::collections::BTreeMap<&'a str, usize>,
    max_attempts: usize,
    attempts: usize,
}

impl<'a> Resolver<'a> {
    fn inspect(
        &mut self,
        roots: &[Requirement],
        selected: &mut std::collections::BTreeMap<String, Version>,
    ) -> Result<Vec<Selection>, ResolveError> {
        let mut active: std::collections::BTreeMap<String, Vec<Constraint>> =
            std::collections::BTreeMap::new();

        for root in roots {
            active.entry(root.package.clone()).or_default().push(root.constraint);
        }

        for (name, version) in selected.iter() {
            if let Some(&idx) = self.package_index.get(name.as_str()) {
                if let Some(release) = self.catalog[idx].releases.iter().find(|r| r.version == *version) {
                    for dep in &release.deps {
                        active.entry(dep.package.clone()).or_default().push(dep.constraint);
                    }
                }
            }
        }

        for cs in active.values_mut() {
            *cs = canonical_constraints(std::mem::take(cs));
        }

        for name in active.keys() {
            if !self.package_index.contains_key(name.as_str()) {
                return Err(ResolveError::Missing { package: name.clone() });
            }
        }

        for (name, version) in selected.iter() {
            if let Some(cs) = active.get(name) {
                if !matches_all(*version, cs) {
                    return Err(ResolveError::Conflict {
                        package: name.clone(),
                        selected: *version,
                        requirements: cs.clone(),
                    });
                }
            }
        }

        for (name, cs) in &active {
            if selected.contains_key(name) {
                continue;
            }
            let idx = self.package_index[name.as_str()];
            if !self.catalog[idx].releases.iter().any(|r| matches_all(r.version, cs)) {
                return Err(ResolveError::NoMatch {
                    package: name.clone(),
                    requirements: cs.clone(),
                });
            }
        }

        let next = active.keys().find(|name| !selected.contains_key(*name)).cloned();
        let Some(name) = next else {
            return Ok(selected
                .iter()
                .map(|(package, version)| Selection {
                    package: package.clone(),
                    version: *version,
                })
                .collect());
        };

        let idx = self.package_index[name.as_str()];
        let cs = &active[&name];
        let mut candidates: Vec<&Release> = self.catalog[idx]
            .releases
            .iter()
            .filter(|r| matches_all(r.version, cs))
            .collect();
        candidates.sort_by(|a, b| b.version.cmp(&a.version));

        let mut first_error = None;
        for release in candidates {
            if self.attempts == self.max_attempts {
                return Err(ResolveError::Limit {
                    attempts: self.max_attempts,
                });
            }
            self.attempts += 1;
            selected.insert(name.clone(), release.version);
            match self.inspect(roots, selected) {
                Ok(result) => return Ok(result),
                Err(err @ ResolveError::Limit { .. }) => return Err(err),
                Err(err) => {
                    selected.remove(&name);
                    if first_error.is_none() {
                        first_error = Some(err);
                    }
                }
            }
        }

        Err(first_error.unwrap_or_else(|| ResolveError::NoMatch {
            package: name.clone(),
            requirements: cs.clone(),
        }))
    }
}

pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
    max_attempts: usize,
) -> Result<Vec<Selection>, ResolveError> {
    for (i, root) in roots.iter().enumerate() {
        if !valid_name(&root.package) {
            return Err(ResolveError::InvalidName {
                site: format!("root[{}]", i),
                name: root.package.clone(),
            });
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
            }
        }
    }

    let mut seen_packages = std::collections::BTreeSet::new();
    for package in catalog {
        if !seen_packages.insert(package.name.as_str()) {
            return Err(ResolveError::DuplicatePackage {
                package: package.name.clone(),
            });
        }
    }

    for package in catalog {
        let mut seen_versions = std::collections::BTreeSet::new();
        for release in &package.releases {
            if !seen_versions.insert(release.version) {
                return Err(ResolveError::DuplicateVersion {
                    package: package.name.clone(),
                    version: release.version,
                });
            }
        }
    }

    let package_index = catalog
        .iter()
        .enumerate()
        .map(|(i, p)| (p.name.as_str(), i))
        .collect();

    let mut resolver = Resolver {
        catalog,
        package_index,
        max_attempts,
        attempts: 0,
    };
    let mut selected = std::collections::BTreeMap::new();
    resolver.inspect(roots, &mut selected)
}

fn main() {}