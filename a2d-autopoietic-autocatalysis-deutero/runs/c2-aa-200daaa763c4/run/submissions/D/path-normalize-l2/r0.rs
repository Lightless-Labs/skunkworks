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

    if let Some(at) = path.as_bytes().iter().position(|&byte| byte == 0) {
        return Err(PathError::Nul { at });
    }
    if let Some(at) = path.as_bytes().iter().position(|&byte| byte == b'\\') {
        return Err(PathError::Backslash { at });
    }
    if let Some(at) = path
        .as_bytes()
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
            .rsplit('/')
            .find(|component| !component.is_empty())
            .is_some_and(|component| component == "." || component == "..")
}

fn components(path: &str, start: usize) -> impl Iterator<Item = (usize, &str)> {
    path[start..]
        .split('/')
        .scan(start, |offset, component| {
            let at = *offset;
            *offset += component.len() + 1;
            Some((at, component))
        })
        .filter(|(_, component)| !component.is_empty())
}

fn reduce<'a>(
    input: impl Iterator<Item = (usize, &'a str)>,
    absolute: bool,
) -> Result<Vec<&'a str>, PathError> {
    let mut result = Vec::new();

    for (at, component) in input {
        match component {
            "." => {}
            ".." => {
                if result.last().is_some_and(|part| *part != "..") {
                    result.pop();
                } else if absolute {
                    return Err(PathError::Escape { at });
                } else {
                    result.push("..");
                }
            }
            other => result.push(other),
        }
    }

    Ok(result)
}

fn join_result(parts: &[&str], want_directory: bool) -> String {
    let mut result = format!("/{}", parts.join("/"));

    if want_directory && result != "/" {
        result.push('/');
    }

    result
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;

    let absolute = path.starts_with('/');
    let parts = reduce(components(path, 0), absolute)?;
    let want_directory = directory_intent(path);

    if absolute {
        Ok(join_result(&parts, want_directory))
    } else {
        let mut result = if parts.is_empty() {
            ".".to_owned()
        } else {
            parts.join("/")
        };
        if want_directory && result != "." {
            result.push('/');
        }
        Ok(result)
    }
}

fn validate_context(path: &str) -> Result<Vec<String>, PathError> {
    if !path.starts_with('/') {
        return Err(PathError::InvalidCwd);
    }

    let normalized = normalize(path).map_err(|_| PathError::InvalidCwd)?;
    Ok(normalized
        .split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect())
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_parts = validate_context(cwd)?;

    let home_parts = match home {
        Some(value) => {
            if !value.starts_with('/') {
                return Err(PathError::InvalidHome);
            }
            let normalized = normalize(value).map_err(|_| PathError::InvalidHome)?;
            Some(
                normalized
                    .split('/')
                    .filter(|part| !part.is_empty())
                    .map(str::to_owned)
                    .collect::<Vec<_>>(),
            )
        }
        None => None,
    };

    validate_raw(path)?;

    let bytes = path.as_bytes();
    let tilde = bytes.first() == Some(&b'~');
    let remainder_start = if tilde {
        if bytes.len() > 1 && bytes[1] != b'/' {
            return Err(PathError::UnsupportedTilde { at: 1 });
        }
        if home_parts.is_none() {
            return Err(PathError::MissingHome);
        }
        if bytes.get(1) == Some(&b'/') { 2 } else { 1 }
    } else {
        0
    };

    let absolute = !tilde && path.starts_with('/');
    let mut result: Vec<String> = if absolute {
        Vec::new()
    } else if tilde {
        home_parts.expect("home was checked")
    } else {
        cwd_parts
    };

    for (at, component) in components(path, remainder_start) {
        match component {
            "." => {}
            ".." => {
                if result.pop().is_none() {
                    return Err(PathError::Escape { at });
                }
            }
            other => result.push(other.to_owned()),
        }
    }

    let parts: Vec<&str> = result.iter().map(String::as_str).collect();
    Ok(join_result(&parts, directory_intent(path)))
}