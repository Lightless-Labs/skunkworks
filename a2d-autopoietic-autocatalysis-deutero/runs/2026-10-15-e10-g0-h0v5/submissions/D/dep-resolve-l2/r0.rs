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
    if bytes.is_empty() || bytes.len() > 24 {
        return false;
    }
    if !bytes[0].is_ascii_lowercase() || bytes[bytes.len() - 1] == b'-' {
        return false;
    }
    bytes
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

fn version_text(v: Version) -> String {
    format!("{}.{}.{}", v.major, v.minor, v.patch)
}

fn constraint_matches(c: Constraint, v: Version) -> bool {
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

fn canonical(mut constraints: Vec<Constraint>) -> Vec<Constraint> {
    constraints.sort();
    constraints.dedup();
    constraints
}

fn constraint_text(c: Constraint) -> String {
    match c {
        Constraint::Exact(v) => format!("={}", version_text(v)),
        Constraint::AtLeast(v) => format!(">={}", version_text(v)),
        Constraint::Before(v) => format!("<{}", version_text(v)),
        Constraint::Caret(v) => format!("^{}", version_text(v)),
    }
}

fn constraints_text(constraints: &[Constraint]) -> String {
    canonical(constraints.to_vec())
        .into_iter()
        .map(constraint_text)
        .collect::<Vec<_>>()
        .join(",")
}

fn encode(text: &str) -> String {
    let mut result = String::new();
    for byte in text.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'.' | b'_' | b'-') {
            result.push(*byte as char);
        } else {
            result.push('%');
            result.push_str(&format!("{:02X}", byte));
        }
    }
    result
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
        ResolveError::Missing { package } => {
            format!("missing|package={}", encode(package))
        }
        ResolveError::NoMatch {
            package,
            requirements,
        } => format!(
            "no-match|package={}|constraints={}",
            encode(package),
            constraints_text(requirements)
        ),
        ResolveError::Conflict {
            package,
            selected,
            requirements,
        } => format!(
            "conflict|package={}|selected={}|constraints={}",
            encode(package),
            version_text(*selected),
            constraints_text(requirements)
        ),
        ResolveError::Limit { attempts } => format!("limit|attempts={}", attempts),
    }
}

fn validate(catalog: &[Package], roots: &[Requirement]) -> Result<(), ResolveError> {
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

    let mut package_names = std::collections::BTreeSet::new();
    for package in catalog {
        if !package_names.insert(package.name.as_str()) {
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
    Ok(())
}

type Active = std::collections::BTreeMap<String, Vec<Constraint>>;
type Assigned = std::collections::BTreeMap<String, usize>;

fn collect_active(
    catalog: &[Package],
    roots: &[Requirement],
    assigned: &Assigned,
) -> Active {
    let mut active = Active::new();
    for root in roots {
        active
            .entry(root.package.clone())
            .or_default()
            .push(root.constraint);
    }

    let package_map: std::collections::BTreeMap<&str, &Package> =
        catalog.iter().map(|p| (p.name.as_str(), p)).collect();

    for (name, release_index) in assigned {
        if let Some(package) = package_map.get(name.as_str()) {
            if let Some(release) = package.releases.get(*release_index) {
                for dep in &release.deps {
                    active
                        .entry(dep.package.clone())
                        .or_default()
                        .push(dep.constraint);
                }
            }
        }
    }

    for constraints in active.values_mut() {
        *constraints = canonical(std::mem::take(constraints));
    }
    active
}

fn search(
    catalog: &[Package],
    roots: &[Requirement],
    assigned: &mut Assigned,
    attempts: &mut usize,
    max_attempts: usize,
) -> Result<Vec<Selection>, ResolveError> {
    let active = collect_active(catalog, roots, assigned);
    let package_map: std::collections::BTreeMap<&str, &Package> =
        catalog.iter().map(|p| (p.name.as_str(), p)).collect();

    if let Some(name) = active
        .keys()
        .filter(|name| !package_map.contains_key(name.as_str()))
        .next()
    {
        return Err(ResolveError::Missing {
            package: name.clone(),
        });
    }

    for (name, release_index) in assigned.iter() {
        if let Some(requirements) = active.get(name) {
            let package = package_map[name.as_str()];
            let version = package.releases[*release_index].version;
            if requirements
                .iter()
                .any(|constraint| !constraint_matches(*constraint, version))
            {
                return Err(ResolveError::Conflict {
                    package: name.clone(),
                    selected: version,
                    requirements: requirements.clone(),
                });
            }
        }
    }

    for (name, requirements) in &active {
        if assigned.contains_key(name) {
            continue;
        }
        let package = package_map[name.as_str()];
        if !package.releases.iter().any(|release| {
            requirements
                .iter()
                .all(|constraint| constraint_matches(*constraint, release.version))
        }) {
            return Err(ResolveError::NoMatch {
                package: name.clone(),
                requirements: requirements.clone(),
            });
        }
    }

    let next = active.keys().find(|name| !assigned.contains_key(*name));
    let Some(name) = next else {
        let mut selections = assigned
            .iter()
            .map(|(name, release_index)| Selection {
                package: name.clone(),
                version: package_map[name.as_str()].releases[*release_index].version,
            })
            .collect::<Vec<_>>();
        selections.sort_by(|a, b| a.package.cmp(&b.package));
        return Ok(selections);
    };

    let package = package_map[name.as_str()];
    let requirements = &active[name];
    let mut candidates = package
        .releases
        .iter()
        .enumerate()
        .filter(|(_, release)| {
            requirements
                .iter()
                .all(|constraint| constraint_matches(*constraint, release.version))
        })
        .map(|(index, release)| (index, release.version))
        .collect::<Vec<_>>();
    candidates.sort_by(|a, b| b.1.cmp(&a.1));

    let mut first_error = None;
    for (release_index, _) in candidates {
        if *attempts == max_attempts {
            return Err(ResolveError::Limit {
                attempts: max_attempts,
            });
        }
        *attempts += 1;
        assigned.insert(name.clone(), release_index);
        match search(catalog, roots, assigned, attempts, max_attempts) {
            Ok(solution) => return Ok(solution),
            Err(error @ ResolveError::Limit { .. }) => {
                assigned.remove(name);
                return Err(error);
            }
            Err(error) => {
                assigned.remove(name);
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }

    Err(first_error.unwrap_or_else(|| ResolveError::NoMatch {
        package: name.clone(),
        requirements: requirements.clone(),
    }))
}

pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
    max_attempts: usize,
) -> Result<Vec<Selection>, ResolveError> {
    validate(catalog, roots)?;
    let mut attempts = 0usize;
    search(
        catalog,
        roots,
        &mut Assigned::new(),
        &mut attempts,
        max_attempts,
    )
}