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

    let bytes = path.as_bytes();
    if let Some(at) = bytes.iter().position(|&byte| byte == 0) {
        return Err(PathError::Nul { at });
    }
    if let Some(at) = bytes.iter().position(|&byte| byte == b'\\') {
        return Err(PathError::Backslash { at });
    }
    if let Some(at) = bytes
        .iter()
        .position(|&byte| (1..=31).contains(&byte) || byte == 127)
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

fn reduce_component(
    components: &mut Vec<String>,
    component: &str,
    absolute: bool,
    at: usize,
) -> Result<(), PathError> {
    match component {
        "" | "." => {}
        ".." => {
            if components.last().is_some_and(|last| last != "..") {
                components.pop();
            } else if absolute {
                return Err(PathError::Escape { at });
            } else {
                components.push("..".to_owned());
            }
        }
        _ => components.push(component.to_owned()),
    }
    Ok(())
}

fn component_offsets(path: &str) -> impl Iterator<Item = (usize, &str)> {
    let bytes = path.as_bytes();
    let mut start = 0;

    std::iter::from_fn(move || {
        while start < bytes.len() && bytes[start] == b'/' {
            start += 1;
        }
        if start >= bytes.len() {
            return None;
        }

        let component_start = start;
        while start < bytes.len() && bytes[start] != b'/' {
            start += 1;
        }
        Some((component_start, &path[component_start..start]))
    })
}

fn assemble(components: &[String], absolute: bool, intent: bool) -> String {
    let joined = components.join("/");
    let mut result = if absolute {
        if joined.is_empty() {
            "/".to_owned()
        } else {
            format!("/{joined}")
        }
    } else if joined.is_empty() {
        ".".to_owned()
    } else {
        joined
    };

    if intent && result != "/" && result != "." {
        result.push('/');
    }
    result
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;

    let absolute = path.as_bytes().first() == Some(&b'/');
    let intent = directory_intent(path);
    let mut components = Vec::new();

    for (at, component) in component_offsets(path) {
        reduce_component(&mut components, component, absolute, at)?;
    }

    Ok(assemble(&components, absolute, intent))
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
    let cwd_components = validate_context(cwd)?;

    let home_components = match home {
        Some(home) => Some(validate_context(home).map_err(|_| PathError::InvalidHome)?),
        None => None,
    };

    validate_raw(path)?;

    let bytes = path.as_bytes();
    let (mut components, request_offset) = if path.starts_with('/') {
        (Vec::new(), 0)
    } else if path.starts_with('~') {
        if bytes.len() == 1 || bytes[1] == b'/' {
            let home_components = home_components.ok_or(PathError::MissingHome)?;
            let offset = if bytes.len() > 1 { 2 } else { 1 };
            (home_components, offset)
        } else {
            return Err(PathError::UnsupportedTilde { at: 1 });
        }
    } else {
        (cwd_components, 0)
    };

    for (relative_at, component) in component_offsets(&path[request_offset..]) {
        reduce_component(
            &mut components,
            component,
            true,
            request_offset + relative_at,
        )?;
    }

    Ok(assemble(&components, true, directory_intent(path)))
}