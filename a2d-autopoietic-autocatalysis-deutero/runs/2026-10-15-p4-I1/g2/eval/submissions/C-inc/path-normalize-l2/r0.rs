#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathError {
    Empty,
    Nul { at: usize },
    Backslash { at: usize },
    Control { at: usize },
    Escape { at: usize },
    InvalidCwd,
    InvalidHome,
    MissingHome,
    UnsupportedTilde { at: usize },
}

fn validate_raw(path: &str) -> Result<(), PathError> {
    if path.is_empty() {
        return Err(PathError::Empty);
    }

    if let Some(at) = path.as_bytes().iter().position(|&b| b == 0) {
        return Err(PathError::Nul { at });
    }
    if let Some(at) = path.as_bytes().iter().position(|&b| b == b'\\') {
        return Err(PathError::Backslash { at });
    }
    if let Some(at) = path
        .as_bytes()
        .iter()
        .position(|&b| (1..=31).contains(&b) || b == 127)
    {
        return Err(PathError::Control { at });
    }

    Ok(())
}

fn is_parent(component: &str) -> bool {
    component == ".."
}

fn directory_intent(path: &str) -> bool {
    path.ends_with('/')
        || path
            .rsplit('/')
            .find(|component| !component.is_empty())
            .is_some_and(|component| component == "." || component == "..")
}

fn reduce_components<'a, I>(
    components: I,
    absolute: bool,
) -> Result<Vec<&'a str>, PathError>
where
    I: IntoIterator<Item = (&'a str, usize)>,
{
    let mut result = Vec::new();

    for (component, offset) in components {
        if component.is_empty() || component == "." {
            continue;
        }

        if is_parent(component) {
            if result.last().is_some_and(|last| *last != "..") {
                result.pop();
            } else if absolute {
                return Err(PathError::Escape { at: offset });
            } else {
                result.push(component);
            }
        } else {
            result.push(component);
        }
    }

    Ok(result)
}

fn join_components<'a>(components: &[&'a str], absolute: bool) -> String {
    let joined = components.join("/");
    if absolute {
        if joined.is_empty() {
            "/".to_owned()
        } else {
            format!("/{joined}")
        }
    } else if joined.is_empty() {
        ".".to_owned()
    } else {
        joined
    }
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;

    let absolute = path.as_bytes().first() == Some(&b'/');
    let components = path
        .split('/')
        .scan(0usize, |offset, component| {
            let start = *offset;
            *offset += component.len() + 1;
            Some((component, start))
        });

    let reduced = reduce_components(components, absolute)?;
    let mut result = join_components(&reduced, absolute);

    if directory_intent(path) && result != "/" && result != "." {
        result.push('/');
    }

    Ok(result)
}

fn normalized_context(path: &str) -> Result<Vec<String>, ()> {
    if !path.starts_with('/') {
        return Err(());
    }
    let normalized = normalize(path).map_err(|_| ())?;
    Ok(normalized
        .split('/')
        .filter(|component| !component.is_empty())
        .map(str::to_owned)
        .collect())
}

pub fn resolve(
    path: &str,
    cwd: &str,
    home: Option<&str>,
) -> Result<String, PathError> {
    let cwd_components = normalized_context(cwd).map_err(|_| PathError::InvalidCwd)?;

    let home_components = match home {
        Some(home) => Some(normalized_context(home).map_err(|_| PathError::InvalidHome)?),
        None => None,
    };

    validate_raw(path)?;

    let tilde_expanded = path.starts_with('~');
    let (absolute, mut base, remainder, remainder_offset) = if tilde_expanded {
        let bytes = path.as_bytes();
        if bytes.len() > 1 && bytes[1] != b'/' {
            return Err(PathError::UnsupportedTilde { at: 1 });
        }

        let home_components = home_components.ok_or(PathError::MissingHome)?;
        let remainder_offset = if bytes.len() > 1 { 2 } else { 1 };
        (
            true,
            home_components,
            &path[remainder_offset..],
            remainder_offset,
        )
    } else if path.starts_with('/') {
        (true, Vec::new(), path, 0)
    } else {
        (true, cwd_components, path, 0)
    };

    let components = remainder
        .split('/')
        .scan(remainder_offset, |offset, component| {
            let start = *offset;
            *offset += component.len() + 1;
            Some((component, start))
        });

    let mut combined: Vec<&str> = base.iter().map(String::as_str).collect();
    for (component, offset) in components {
        if component.is_empty() || component == "." {
            continue;
        }
        if component == ".." {
            if combined.is_empty() {
                return Err(PathError::Escape { at: offset });
            }
            combined.pop();
        } else {
            combined.push(component);
        }
    }

    let mut result = join_components(&combined, absolute);
    if directory_intent(path) && result != "/" {
        result.push('/');
    }

    base.clear();
    Ok(result)
}

fn main() {}