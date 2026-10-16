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

fn reduce_components<'a, I>(
    components: I,
    absolute: bool,
    offset: usize,
    retained: &mut Vec<String>,
    error_offset: impl Fn(usize) -> usize,
) -> Result<(), PathError>
where
    I: IntoIterator<Item = (usize, &'a str)>,
{
    for (local_at, component) in components {
        if component.is_empty() || component == "." {
            continue;
        }

        if component == ".." {
            if retained.last().is_some_and(|part| part != "..") {
                retained.pop();
            } else if absolute {
                return Err(PathError::Escape {
                    at: error_offset(offset + local_at),
                });
            } else {
                retained.push("..".to_owned());
            }
        } else {
            retained.push(component.to_owned());
        }
    }
    Ok(())
}

fn normalized_components(path: &str) -> Result<(bool, Vec<String>), PathError> {
    validate_raw(path)?;
    let absolute = path.starts_with('/');
    let mut retained = Vec::new();
    let components = path
        .split('/')
        .scan(0usize, |position, component| {
            let at = *position;
            *position += component.len() + 1;
            Some((at, component))
        });

    reduce_components(components, absolute, 0, &mut retained, |at| at)?;
    Ok((absolute, retained))
}

fn render(components: &[String], absolute: bool, intent: bool) -> String {
    let mut result = if absolute {
        if components.is_empty() {
            "/".to_owned()
        } else {
            format!("/{}", components.join("/"))
        }
    } else if components.is_empty() {
        ".".to_owned()
    } else {
        components.join("/")
    };

    if intent && result != "/" && result != "." {
        result.push('/');
    }
    result
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    let intent = directory_intent(path);
    let (absolute, components) = normalized_components(path)?;
    Ok(render(&components, absolute, intent))
}

fn validate_context(path: &str) -> Result<Vec<String>, PathError> {
    if !path.starts_with('/') {
        return Err(PathError::InvalidCwd);
    }

    normalized_components(path)
        .map(|(_, components)| components)
        .map_err(|_| PathError::InvalidCwd)
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_components = validate_context(cwd)?;
    let home_components = match home {
        Some(value) => {
            if !value.starts_with('/') {
                return Err(PathError::InvalidHome);
            }
            Some(
                normalized_components(value)
                    .map(|(_, components)| components)
                    .map_err(|_| PathError::InvalidHome)?,
            )
        }
        None => None,
    };

    validate_raw(path)?;

    let mut request_offset = 0usize;
    let (absolute, mut retained) = if path.starts_with('~') {
        let bytes = path.as_bytes();
        if bytes.len() > 1 && bytes[1] != b'/' {
            return Err(PathError::UnsupportedTilde { at: 1 });
        }

        let base = home_components.ok_or(PathError::MissingHome)?;
        request_offset = if bytes.len() > 1 { 2 } else { 1 };
        (true, base)
    } else if path.starts_with('/') {
        (true, Vec::new())
    } else {
        (true, cwd_components)
    };

    let remainder = &path[request_offset..];
    let components = remainder
        .split('/')
        .scan(request_offset, |position, component| {
            let at = *position;
            *position += component.len() + 1;
            Some((at, component))
        });

    reduce_components(
        components,
        absolute,
        0,
        &mut retained,
        |at| at,
    )?;

    Ok(render(&retained, true, directory_intent(path)))
}