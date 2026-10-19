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

fn components(path: &str) -> Vec<(usize, &str)> {
    path.split('/')
        .scan(0usize, |offset, part| {
            let start = *offset;
            *offset += part.len() + 1;
            Some((start, part))
        })
        .filter(|(_, part)| !part.is_empty())
        .collect()
}

fn directory_intent(path: &str) -> bool {
    path.ends_with('/')
        || components(path)
            .last()
            .is_some_and(|(_, component)| *component == "." || *component == "..")
}

fn reduce(
    initial: Vec<String>,
    input: &str,
    parts: &[(usize, &str)],
    absolute: bool,
) -> Result<Vec<String>, PathError> {
    let mut stack = initial;

    for &(at, part) in parts {
        match part {
            "." => {}
            ".." => {
                if stack.last().is_some_and(|last| last != "..") {
                    stack.pop();
                } else if absolute {
                    return Err(PathError::Escape { at });
                } else {
                    stack.push("..".to_owned());
                }
            }
            ordinary => stack.push(ordinary.to_owned()),
        }
    }

    let _ = input;
    Ok(stack)
}

fn render(parts: &[String], absolute: bool, want_directory: bool) -> String {
    let mut result = if absolute {
        format!("/{}", parts.join("/"))
    } else if parts.is_empty() {
        ".".to_owned()
    } else {
        parts.join("/")
    };

    if absolute && parts.is_empty() {
        return result;
    }
    if !absolute && parts.is_empty() {
        return result;
    }
    if want_directory {
        result.push('/');
    }
    result
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;
    let absolute = path.as_bytes().first() == Some(&b'/');
    let want_directory = directory_intent(path);
    let parts = components(path);
    let reduced = reduce(Vec::new(), path, &parts, absolute)?;
    Ok(render(&reduced, absolute, want_directory))
}

fn validated_context(path: &str) -> Result<Vec<String>, ()> {
    if !path.starts_with('/') {
        return Err(());
    }
    let normalized = normalize(path).map_err(|_| ())?;
    let body = normalized.strip_prefix('/').ok_or(())?;
    if body.is_empty() {
        Ok(Vec::new())
    } else {
        Ok(body
            .trim_end_matches('/')
            .split('/')
            .filter(|part| !part.is_empty())
            .map(str::to_owned)
            .collect())
    }
}

pub fn resolve(
    path: &str,
    cwd: &str,
    home: Option<&str>,
) -> Result<String, PathError> {
    let cwd_parts = validated_context(cwd).map_err(|_| PathError::InvalidCwd)?;
    let home_parts = match home {
        Some(home) => Some(validated_context(home).map_err(|_| PathError::InvalidHome)?),
        None => None,
    };

    validate_raw(path)?;

    let bytes = path.as_bytes();
    let tilde = bytes.first() == Some(&b'~');
    if tilde && bytes.len() > 1 && bytes[1] != b'/' {
        return Err(PathError::UnsupportedTilde { at: 1 });
    }

    let (initial, remainder, offset, absolute) = if tilde {
        let home_parts = home_parts.ok_or(PathError::MissingHome)?;
        let consumed = if bytes.len() > 1 { 2 } else { 1 };
        (home_parts, &path[consumed..], consumed, true)
    } else if bytes.first() == Some(&b'/') {
        (Vec::new(), path, 0, true)
    } else {
        (cwd_parts, path, 0, true)
    };

    let want_directory = directory_intent(path);
    let parts: Vec<(usize, &str)> = components(remainder)
        .into_iter()
        .map(|(at, part)| (at + offset, part))
        .collect();

    let reduced = reduce(initial, path, &parts, absolute)?;
    Ok(render(&reduced, true, want_directory))
}