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

    path.split('/').filter(|component| !component.is_empty()).last()
        .is_some_and(|component| component == "." || component == "..")
}

fn components(path: &str) -> impl Iterator<Item = (usize, &str)> {
    path.split_inclusive('/')
        .scan(0usize, |offset, part| {
            let start = *offset;
            *offset += part.len();
            let component = part.strip_suffix('/').unwrap_or(part);
            Some((start, component))
        })
        .filter(|(_, component)| !component.is_empty())
}

fn reduce(
    stack: &mut Vec<String>,
    path: &str,
    base_offset: usize,
    absolute: bool,
) -> Result<(), PathError> {
    for (at, component) in components(path) {
        match component {
            "." => {}
            ".." => {
                if stack.last().is_some_and(|last| last != "..") {
                    stack.pop();
                } else if absolute {
                    return Err(PathError::Escape {
                        at: base_offset + at,
                    });
                } else {
                    stack.push("..".to_owned());
                }
            }
            name => stack.push(name.to_owned()),
        }
    }
    Ok(())
}

fn joined(stack: &[String], absolute: bool, intent: bool) -> String {
    let mut result = stack.join("/");
    if absolute {
        result.insert(0, '/');
    }
    if result.is_empty() {
        result = if absolute { "/".to_owned() } else { ".".to_owned() };
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
    let mut stack = Vec::new();
    reduce(&mut stack, path, 0, absolute)?;
    Ok(joined(&stack, absolute, intent))
}

fn validate_context(path: &str) -> Result<Vec<String>, ()> {
    if path.as_bytes().first() != Some(&b'/') {
        return Err(());
    }
    validate_raw(path).map_err(|_| ())?;

    let mut stack = Vec::new();
    reduce(&mut stack, path, 0, true).map_err(|_| ())?;
    Ok(stack)
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_stack = validate_context(cwd).map_err(|_| PathError::InvalidCwd)?;
    let home_stack = match home {
        Some(value) => Some(validate_context(value).map_err(|_| PathError::InvalidHome)?),
        None => None,
    };

    validate_raw(path)?;

    let bytes = path.as_bytes();
    let (mut stack, remainder, remainder_offset, absolute) =
        if bytes.first() == Some(&b'/') {
            (Vec::new(), path, 0, true)
        } else if bytes.first() == Some(&b'~') {
            if bytes.len() > 1 && bytes[1] != b'/' {
                return Err(PathError::UnsupportedTilde { at: 1 });
            }
            let home_stack = home_stack.ok_or(PathError::MissingHome)?;
            let remainder_offset = if bytes.len() > 1 { 2 } else { 1 };
            let remainder = &path[remainder_offset..];
            (home_stack, remainder, remainder_offset, true)
        } else {
            (cwd_stack, path, 0, true)
        };

    let intent = directory_intent(path);
    reduce(&mut stack, remainder, remainder_offset, absolute)?;
    Ok(joined(&stack, true, intent))
}