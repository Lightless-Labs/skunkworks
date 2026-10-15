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
    if path.ends_with('/') {
        return true;
    }

    path.rsplit('/')
        .find(|component| !component.is_empty())
        .is_some_and(|component| component == "." || component == "..")
}

fn reduce_components(
    stack: &mut Vec<String>,
    path: &str,
    offset: usize,
    absolute: bool,
) -> Result<(), PathError> {
    let mut start = 0;

    for end in path
        .match_indices('/')
        .map(|(at, _)| at)
        .chain(std::iter::once(path.len()))
    {
        let component = &path[start..end];

        if !component.is_empty() && component != "." {
            if component == ".." {
                if stack.last().is_some_and(|last| last != "..") {
                    stack.pop();
                } else if absolute {
                    return Err(PathError::Escape { at: offset + start });
                } else {
                    stack.push("..".to_string());
                }
            } else {
                stack.push(component.to_string());
            }
        }

        start = end.saturating_add(1);
    }

    Ok(())
}

fn render(stack: &[String], absolute: bool, intent: bool) -> String {
    let mut result = if absolute {
        if stack.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", stack.join("/"))
        }
    } else if stack.is_empty() {
        ".".to_string()
    } else {
        stack.join("/")
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
    let mut stack = Vec::new();
    reduce_components(&mut stack, path, 0, absolute)?;

    Ok(render(&stack, absolute, intent))
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_normalized = if cwd.starts_with('/') {
        normalize(cwd).map_err(|_| PathError::InvalidCwd)?
    } else {
        return Err(PathError::InvalidCwd);
    };

    let home_normalized = match home {
        Some(value) => {
            if !value.starts_with('/') {
                return Err(PathError::InvalidHome);
            }
            Some(normalize(value).map_err(|_| PathError::InvalidHome)?)
        }
        None => None,
    };

    validate_raw(path)?;

    let intent = directory_intent(path);

    let (base, remainder, remainder_offset) = if path.starts_with('/') {
        ("/", path, 0)
    } else if path == "~" || path.starts_with("~/") {
        let home = home_normalized
            .as_deref()
            .ok_or(PathError::MissingHome)?;
        let remainder_offset = if path == "~" { path.len() } else { 2 };
        (home, &path[remainder_offset..], remainder_offset)
    } else if path.starts_with('~') {
        return Err(PathError::UnsupportedTilde { at: 1 });
    } else {
        (cwd_normalized.as_str(), path, 0)
    };

    let mut stack: Vec<String> = base
        .split('/')
        .filter(|component| !component.is_empty())
        .map(str::to_string)
        .collect();

    reduce_components(&mut stack, remainder, remainder_offset, true)?;
    Ok(render(&stack, true, intent))
}