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

fn directory_intent(path: &str) -> bool {
    path.ends_with('/')
        || path
            .split('/')
            .filter(|component| !component.is_empty())
            .last()
            .is_some_and(|component| component == "." || component == "..")
}

fn reduce_components<'a>(
    components: impl IntoIterator<Item = (&'a str, usize)>,
    absolute: bool,
) -> Result<Vec<String>, PathError> {
    let mut result: Vec<String> = Vec::new();

    for (component, at) in components {
        if component.is_empty() || component == "." {
            continue;
        }

        if component == ".." {
            if result.last().is_some_and(|last| last != "..") {
                result.pop();
            } else if absolute {
                return Err(PathError::Escape { at });
            } else {
                result.push("..".to_owned());
            }
        } else {
            result.push(component.to_owned());
        }
    }

    Ok(result)
}

fn split_with_offsets(path: &str, start: usize) -> Vec<(&str, usize)> {
    let bytes = path.as_bytes();
    let mut components = Vec::new();
    let mut i = start;

    while i < bytes.len() {
        while i < bytes.len() && bytes[i] == b'/' {
            i += 1;
        }
        if i == bytes.len() {
            break;
        }

        let component_start = i;
        while i < bytes.len() && bytes[i] != b'/' {
            i += 1;
        }
        components.push((&path[component_start..i], component_start));
    }

    components
}

fn render(components: &[String], absolute: bool, intent: bool) -> String {
    let mut result = String::new();

    if absolute {
        result.push('/');
    }
    result.push_str(&components.join("/"));

    if result.is_empty() {
        result.push('.');
    }

    if intent && result != "/" && result != "." {
        result.push('/');
    }

    result
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;

    let absolute = path.as_bytes().first() == Some(&b'/');
    let intent = directory_intent(path);
    let components = reduce_components(split_with_offsets(path, 0), absolute)?;

    Ok(render(&components, absolute, intent))
}

fn validate_context(path: &str) -> Result<Vec<String>, PathError> {
    if !path.starts_with('/') {
        return Err(PathError::InvalidCwd);
    }
    let normalized = normalize(path).map_err(|_| PathError::InvalidCwd)?;
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
    let cwd_components = validate_context(cwd).map_err(|_| PathError::InvalidCwd)?;

    let home_components = match home {
        Some(home) => Some(validate_context(home).map_err(|_| PathError::InvalidHome)?),
        None => None,
    };

    validate_raw(path)?;

    let bytes = path.as_bytes();
    let tilde_expanded = bytes.first() == Some(&b'~');

    let (mut components, remainder_start, absolute) = if tilde_expanded {
        if bytes.len() > 1 && bytes[1] != b'/' {
            return Err(PathError::UnsupportedTilde { at: 1 });
        }
        let home_components = home_components.ok_or(PathError::MissingHome)?;
        let start = if bytes.len() > 1 { 2 } else { 1 };
        (home_components, start, true)
    } else if bytes.first() == Some(&b'/') {
        (Vec::new(), 0, true)
    } else {
        (cwd_components, 0, true)
    };

    let intent = directory_intent(path);
    let request_components = split_with_offsets(path, remainder_start);

    for (component, at) in request_components {
        if component.is_empty() || component == "." {
            continue;
        }

        if component == ".." {
            if components.pop().is_none() {
                return Err(PathError::Escape { at });
            }
        } else {
            components.push(component.to_owned());
        }
    }

    Ok(render(&components, absolute, intent))
}