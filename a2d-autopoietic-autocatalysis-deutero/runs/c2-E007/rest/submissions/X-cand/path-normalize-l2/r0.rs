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

    if let Some(at) = path.bytes().position(|b| b == 0) {
        return Err(PathError::Nul { at });
    }
    if let Some(at) = path.bytes().position(|b| b == b'\\') {
        return Err(PathError::Backslash { at });
    }
    if let Some(at) = path
        .bytes()
        .position(|b| (1..=31).contains(&b) || b == 127)
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

fn components(path: &str) -> impl Iterator<Item = (usize, &str)> {
    path.match_indices('/')
        .map(|(at, _)| (at, ""))
        .chain(std::iter::once((path.len(), "")))
        .scan(0usize, move |start, (end, _)| {
            let result = (*start, &path[*start..end]);
            *start = end + 1;
            Some(result)
        })
}

fn reduce(
    path: &str,
    absolute: bool,
    initial: Vec<String>,
    offset: usize,
) -> Result<Vec<String>, PathError> {
    let mut stack = initial;

    for (start, component) in components(path) {
        if component.is_empty() || component == "." {
            continue;
        }
        if component == ".." {
            if stack.last().is_some_and(|last| last != "..") {
                stack.pop();
            } else if absolute {
                return Err(PathError::Escape { at: offset + start });
            } else {
                stack.push("..".to_owned());
            }
        } else {
            stack.push(component.to_owned());
        }
    }

    Ok(stack)
}

fn render(stack: &[String], absolute: bool, intent: bool) -> String {
    let mut result = if absolute {
        format!("/{}", stack.join("/"))
    } else if stack.is_empty() {
        ".".to_owned()
    } else {
        stack.join("/")
    };

    if absolute && stack.is_empty() {
        return result;
    }
    if !absolute && stack.is_empty() {
        return result;
    }
    if intent {
        result.push('/');
    }
    result
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;
    let absolute = path.starts_with('/');
    let stack = reduce(path, absolute, Vec::new(), 0)?;
    Ok(render(&stack, absolute, directory_intent(path)))
}

fn validate_context(path: &str) -> Result<Vec<String>, PathError> {
    if !path.starts_with('/') {
        return Err(PathError::InvalidCwd);
    }
    let normalized = normalize(path).map_err(|_| PathError::InvalidCwd)?;
    Ok(normalized
        .trim_start_matches('/')
        .trim_end_matches('/')
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
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
        Some(home_path) => {
            if !home_path.starts_with('/') {
                return Err(PathError::InvalidHome);
            }
            Some(validate_context(home_path).map_err(|_| PathError::InvalidHome)?)
        }
        None => None,
    };

    validate_raw(path)?;

    let (absolute, initial, remainder, offset) = if path.starts_with('~') {
        if path.len() > 1 && !path[1..].starts_with('/') {
            return Err(PathError::UnsupportedTilde { at: 1 });
        }
        let initial = home_components.ok_or(PathError::MissingHome)?;
        let remainder = if path == "~" {
            ""
        } else {
            &path[2..]
        };
        (true, initial, remainder, if path == "~" { path.len() } else { 2 })
    } else if path.starts_with('/') {
        (true, Vec::new(), path, 0)
    } else {
        (true, cwd_components, path, 0)
    };

    let stack = reduce(remainder, absolute, initial, offset)?;
    Ok(render(&stack, true, directory_intent(path)))
}