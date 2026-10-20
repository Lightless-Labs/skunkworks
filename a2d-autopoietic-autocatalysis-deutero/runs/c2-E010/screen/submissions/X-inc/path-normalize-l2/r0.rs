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
    if path.as_bytes().last() == Some(&b'/') {
        return true;
    }
    path.rsplit('/')
        .find(|component| !component.is_empty())
        .is_some_and(|component| component == "." || component == "..")
}

fn components(path: &str, skip_prefix: usize) -> Vec<(usize, &str)> {
    let mut result = Vec::new();
    let bytes = path.as_bytes();
    let mut i = skip_prefix;

    while i < bytes.len() {
        while i < bytes.len() && bytes[i] == b'/' {
            i += 1;
        }
        if i == bytes.len() {
            break;
        }
        let start = i;
        while i < bytes.len() && bytes[i] != b'/' {
            i += 1;
        }
        result.push((start, &path[start..i]));
    }
    result
}

fn reduce(
    stack: &mut Vec<String>,
    path: &str,
    skip_prefix: usize,
    absolute: bool,
    offset_base: usize,
) -> Result<(), PathError> {
    for (at, component) in components(path, skip_prefix) {
        match component {
            "." => {}
            ".." => {
                if stack.last().is_some_and(|item| item != "..") {
                    stack.pop();
                } else if absolute {
                    return Err(PathError::Escape {
                        at: offset_base + at,
                    });
                } else {
                    stack.push("..".to_string());
                }
            }
            _ => stack.push(component.to_string()),
        }
    }
    Ok(())
}

fn render(stack: &[String], absolute: bool, dir_intent: bool) -> String {
    let mut output = if absolute {
        format!("/{}", stack.join("/"))
    } else if stack.is_empty() {
        ".".to_string()
    } else {
        stack.join("/")
    };

    if absolute && stack.is_empty() {
        output = "/".to_string();
    }

    if dir_intent && output != "/" && output != "." {
        output.push('/');
    }
    output
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;
    let absolute = path.as_bytes().first() == Some(&b'/');
    let mut stack = Vec::new();
    reduce(&mut stack, path, 0, absolute, 0)?;
    Ok(render(&stack, absolute, directory_intent(path)))
}

fn normalized_absolute_context(path: &str) -> Result<Vec<String>, PathError> {
    if path.as_bytes().first() != Some(&b'/') {
        return Err(PathError::InvalidCwd);
    }
    let normalized = normalize(path).map_err(|_| PathError::InvalidCwd)?;
    Ok(components(&normalized, 0)
        .into_iter()
        .map(|(_, component)| component.to_string())
        .collect())
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_components = normalized_absolute_context(cwd)?;
    let home_components = match home {
        Some(value) => Some(normalized_absolute_context(value).map_err(|_| PathError::InvalidHome)?),
        None => None,
    };

    validate_raw(path)?;

    let bytes = path.as_bytes();
    let tilde = bytes.first() == Some(&b'~');
    let expanded = if tilde {
        if bytes.len() == 1 {
            true
        } else if bytes[1] == b'/' {
            true
        } else {
            return Err(PathError::UnsupportedTilde { at: 1 });
        }
    } else {
        false
    };

    let (mut stack, skip_prefix) = if expanded {
        let home_stack = home_components.ok_or(PathError::MissingHome)?;
        let skip = if bytes.len() > 1 { 2 } else { 1 };
        (home_stack, skip)
    } else if bytes.first() == Some(&b'/') {
        (Vec::new(), 0)
    } else {
        (cwd_components, 0)
    };

    reduce(&mut stack, path, skip_prefix, true, 0)?;
    Ok(render(&stack, true, directory_intent(path)))
}