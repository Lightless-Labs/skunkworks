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

fn reduce_components<'a, I>(
    components: I,
    stack: &mut Vec<&'a str>,
    absolute: bool,
    offset: usize,
) -> Result<(), PathError>
where
    I: IntoIterator<Item = (usize, &'a str)>,
{
    for (at, component) in components {
        if component.is_empty() || component == "." {
            continue;
        }
        if component == ".." {
            if stack.last().is_some_and(|last| *last != "..") {
                stack.pop();
            } else if absolute {
                return Err(PathError::Escape { at: offset + at });
            } else {
                stack.push(component);
            }
        } else {
            stack.push(component);
        }
    }
    Ok(())
}

fn join_result(stack: &[&str], absolute: bool, dir: bool) -> String {
    let mut result = String::new();
    if absolute {
        result.push('/');
    }
    result.push_str(&stack.join("/"));

    if result.is_empty() {
        result.push('.');
    }

    if dir && result != "/" && result != "." {
        result.push('/');
    }
    result
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;

    let absolute = path.starts_with('/');
    let dir = directory_intent(path);
    let components = path
        .split('/')
        .scan(0usize, |offset, component| {
            let at = *offset;
            *offset += component.len() + 1;
            Some((at, component))
        });

    let mut stack = Vec::new();
    reduce_components(components, &mut stack, absolute, 0)?;
    Ok(join_result(&stack, absolute, dir))
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
        Some(home_path) => {
            if !home_path.starts_with('/') {
                return Err(PathError::InvalidHome);
            }
            Some(
                normalize(home_path)
                    .map_err(|_| PathError::InvalidHome)?
                    .split('/')
                    .filter(|component| !component.is_empty())
                    .map(str::to_owned)
                    .collect::<Vec<_>>(),
            )
        }
        None => None,
    };

    validate_raw(path)?;

    let bytes = path.as_bytes();
    let tilde = bytes.first() == Some(&b'~');
    let expanded = if tilde {
        if bytes.len() == 1 || bytes.get(1) == Some(&b'/') {
            Some(home_components.as_ref().ok_or(PathError::MissingHome)?)
        } else {
            return Err(PathError::UnsupportedTilde { at: 1 });
        }
    } else {
        None
    };

    let absolute = !tilde && path.starts_with('/');
    let mut stack: Vec<String> = if absolute {
        Vec::new()
    } else if let Some(home) = expanded {
        home.clone()
    } else {
        cwd_components
    };

    let remainder_start = if tilde {
        if bytes.get(1) == Some(&b'/') {
            2
        } else {
            1
        }
    } else if absolute {
        1
    } else {
        0
    };
    let remainder = &path[remainder_start..];
    let components = remainder
        .split('/')
        .scan(remainder_start, |offset, component| {
            let at = *offset;
            *offset += component.len() + 1;
            Some((at, component))
        });

    let mut borrowed: Vec<&str> = stack.iter().map(String::as_str).collect();
    reduce_components(components, &mut borrowed, true, 0)?;

    let dir = directory_intent(path);
    Ok(join_result(&borrowed, true, dir))
}