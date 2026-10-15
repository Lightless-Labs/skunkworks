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

fn constraint_text(c: Constraint) -> String {
    match c {
        Constraint::Exact(v) => format!("={}", version_text(v)),
        Constraint::AtLeast(v) => format!(">={}", version_text(v)),
        Constraint::Before(v) => format!("<{}", version_text(v)),
        Constraint::Caret(v) => format!("^{}", version_text(v)),
    }
}

fn canonicalize(constraints: &mut Vec<Constraint>) {
    constraints.sort();
    constraints.dedup();
}

fn constraints_text(constraints: &[Constraint]) -> String {
    let mut canonical = constraints.to_vec();
    canonicalize(&mut canonical);
    canonical
        .iter()
        .map(|c| constraint_text(*c))
        .collect::<Vec<_>>()
        .join(",")
}

fn encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'.' | b'_' | b'-') {
            out.push(*byte as char);
        } else {
            out.push_str(&format!("%{:02X}", byte));
        }
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
    }
}

fn matches_constraint(version: Version, constraint: Constraint) -> bool {
    match constraint {
        Constraint::Exact(x) => version == x,
        Constraint::AtLeast(x) => version >= x,
        Constraint::Before(x) => version < x,
        Constraint::Caret(x) => {
            version >= x
                && if x.major > 0 {
                    version.major == x.major
                } else if x.minor > 0 {
                    version.major == 0 && version.minor == x.minor
                } else {
                    version == x
                }
        }
    }
}

fn matches_all(version: Version, constraints: &[Constraint]) -> bool {
    constraints
        .iter()
        .all(|c| matches_constraint(version, *c))
}

fn validate_names(catalog: &[Package], roots: &[Requirement]) -> Result<(), ResolveError> {
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
    Ok(())
}

fn validate_duplicates(catalog: &[Package]) -> Result<(), ResolveError> {
    for i in 0..catalog.len() {
        if catalog[..i]
            .iter()
            .any(|previous| previous.name == catalog[i].name)
        {
            return Err(ResolveError::DuplicatePackage {
                package: catalog[i].name.clone(),
            });
        }
    }

    for package in catalog {
        for i in 0..package.releases.len() {
            if package.releases[..i]
                .iter()
                .any(|previous| previous.version == package.releases[i].version)
            {
                return Err(ResolveError::DuplicateVersion {
                    package: package.name.clone(),
                    version: package.releases[i].version,
                });
            }
        }
    }
    Ok(())
}

fn package_by_name<'a>(catalog: &'a [Package], name: &str) -> Option<&'a Package> {
    catalog.iter().find(|package| package.name == name)
}

fn resolve_node(
    catalog: &[Package],
    constraints: &mut Vec<(String, Constraint)>,
    selected: &mut Vec<(String, Version)>,
) -> Result<Vec<Selection>, ResolveError> {
    let mut active: Vec<(String, Vec<Constraint>)> = Vec::new();

    for (name, constraint) in constraints.iter() {
        if let Some((_, list)) = active.iter_mut().find(|(n, _)| n == name) {
            list.push(*constraint);
        } else {
            active.push((name.clone(), vec![*constraint]));
        }
    }

    active.sort_by(|a, b| a.0.cmp(&b.0));
    for (_, list) in &mut active {
        canonicalize(list);
    }

    if let Some((name, _)) = active
        .iter()
        .find(|(name, _)| package_by_name(catalog, name).is_none())
    {
        return Err(ResolveError::Missing {
            package: name.clone(),
        });
    }

    for (name, requirements) in &active {
        if let Some((_, version)) = selected.iter().find(|(n, _)| n == name) {
            if !matches_all(*version, requirements) {
                return Err(ResolveError::Conflict {
                    package: name.clone(),
                    selected: *version,
                    requirements: requirements.clone(),
                });
            }
        }
    }

    for (name, requirements) in &active {
        if selected.iter().any(|(n, _)| n == name) {
            continue;
        }
        let package = package_by_name(catalog, name).expect("missing checked above");
        if !package
            .releases
            .iter()
            .any(|release| matches_all(release.version, requirements))
        {
            return Err(ResolveError::NoMatch {
                package: name.clone(),
                requirements: requirements.clone(),
            });
        }
    }

    let next = active
        .iter()
        .find(|(name, _)| !selected.iter().any(|(n, _)| n == name));
    let Some((name, requirements)) = next else {
        let mut result: Vec<Selection> = selected
            .iter()
            .map(|(package, version)| Selection {
                package: package.clone(),
                version: *version,
            })
            .collect();
        result.sort_by(|a, b| a.package.cmp(&b.package));
        return Ok(result);
    };

    let package = package_by_name(catalog, name).expect("missing checked above");
    let release = package
        .releases
        .iter()
        .filter(|release| matches_all(release.version, requirements))
        .max_by_key(|release| release.version)
        .expect("zero candidates checked above");

    selected.push((name.clone(), release.version));
    let previous_len = constraints.len();
    constraints.extend(
        release
            .deps
            .iter()
            .map(|dep| (dep.package.clone(), dep.constraint)),
    );

    let result = resolve_node(catalog, constraints, selected);

    constraints.truncate(previous_len);
    selected.pop();
    result
}

pub fn resolve(
    catalog: &[Package],
    roots: &[Requirement],
) -> Result<Vec<Selection>, ResolveError> {
    validate_names(catalog, roots)?;
    validate_duplicates(catalog)?;

    let mut constraints: Vec<(String, Constraint)> = roots
        .iter()
        .map(|root| (root.package.clone(), root.constraint))
        .collect();
    let mut selected = Vec::new();
    resolve_node(catalog, &mut constraints, &mut selected)
}

fn main() {}