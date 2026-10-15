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
            .rsplit('/')
            .find(|component| !component.is_empty())
            .is_some_and(|component| component == "." || component == "..")
}

fn components(path: &str) -> impl Iterator<Item = (usize, &str)> {
    path.match_indices('/')
        .map(|(at, _)| (at, at))
        .chain(std::iter::once((path.len(), path.len())))
        .scan(0usize, move |start, (end, _)| {
            let current_start = *start;
            *start = end + 1;
            Some((current_start, &path[current_start..end]))
        })
        .filter(|(_, component)| !component.is_empty())
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;

    let absolute = path.as_bytes()[0] == b'/';
    let intent = directory_intent(path);
    let mut stack: Vec<&str> = Vec::new();

    for (at, component) in components(path) {
        match component {
            "." => {}
            ".." => {
                if stack.last().is_some_and(|last| *last != "..") {
                    stack.pop();
                } else if absolute {
                    return Err(PathError::Escape { at });
                } else {
                    stack.push("..");
                }
            }
            _ => stack.push(component),
        }
    }

    let mut result = if absolute {
        format!("/{}", stack.join("/"))
    } else if stack.is_empty() {
        ".".to_string()
    } else {
        stack.join("/")
    };

    if intent && result != "/" && result != "." {
        result.push('/');
    }
    Ok(result)
}

fn validate_context(path: &str) -> Result<String, ()> {
    if !path.starts_with('/') {
        return Err(());
    }
    normalize(path).map_err(|_| ())
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd = validate_context(cwd).map_err(|_| PathError::InvalidCwd)?;
    let home = match home {
        Some(home) => Some(validate_context(home).map_err(|_| PathError::InvalidHome)?),
        None => None,
    };

    validate_raw(path)?;

    let (base, remainder, offset) = if path.starts_with('~') {
        if path.len() == 1 || path.as_bytes().get(1) == Some(&b'/') {
            let home = home.ok_or(PathError::MissingHome)?;
            let remainder = if path.len() > 1 { &path[2..] } else { "" };
            (home, remainder, if path.len() > 1 { 2 } else { 1 })
        } else {
            return Err(PathError::UnsupportedTilde { at: 1 });
        }
    } else if path.starts_with('/') {
        ("/".to_string(), path, 0)
    } else {
        (cwd, path, 0)
    };

    let intent = directory_intent(path);
    let mut stack: Vec<String> = base
        .split('/')
        .filter(|component| !component.is_empty())
        .map(str::to_string)
        .collect();

    for (relative_at, component) in components(remainder) {
        match component {
            "." => {}
            ".." => {
                if stack.pop().is_none() {
                    return Err(PathError::Escape {
                        at: offset + relative_at,
                    });
                }
            }
            _ => stack.push(component.to_string()),
        }
    }

    let mut result = format!("/{}", stack.join("/"));
    if intent && result != "/" {
        result.push('/');
    }
    Ok(result)
}

fn main() {}