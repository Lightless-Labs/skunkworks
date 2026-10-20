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

fn components(path: &str, offset: usize) -> Vec<(String, usize)> {
    let mut result = Vec::new();
    let mut start = None;

    for (at, byte) in path.bytes().enumerate() {
        if byte == b'/' {
            if let Some(begin) = start.take() {
                result.push((path[begin..at].to_owned(), offset + begin));
            }
        } else if start.is_none() {
            start = Some(at);
        }
    }

    if let Some(begin) = start {
        result.push((path[begin..].to_owned(), offset + begin));
    }
    result
}

fn directory_intent(path: &str) -> bool {
    path.ends_with('/')
        || path
            .rsplit('/')
            .find(|component| !component.is_empty())
            .is_some_and(|component| component == "." || component == "..")
}

fn reduce_absolute(
    initial: Vec<String>,
    parts: Vec<(String, usize)>,
) -> Result<Vec<String>, PathError> {
    let mut stack = initial;
    for (part, at) in parts {
        match part.as_str() {
            "." => {}
            ".." => {
                if stack.pop().is_none() {
                    return Err(PathError::Escape { at });
                }
            }
            _ => stack.push(part),
        }
    }
    Ok(stack)
}

fn format_absolute(parts: Vec<String>, intent: bool) -> String {
    let mut output = String::from("/");
    output.push_str(&parts.join("/"));
    if intent && output.len() > 1 {
        output.push('/');
    }
    output
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;

    let absolute = path.as_bytes()[0] == b'/';
    let intent = directory_intent(path);
    let parts = components(path, 0);

    if absolute {
        let reduced = reduce_absolute(Vec::new(), parts)?;
        Ok(format_absolute(reduced, intent))
    } else {
        let mut stack: Vec<String> = Vec::new();
        for (part, _) in parts {
            match part.as_str() {
                "." => {}
                ".." => {
                    if stack.last().is_some_and(|last| last != "..") {
                        stack.pop();
                    } else {
                        stack.push(part);
                    }
                }
                _ => stack.push(part),
            }
        }

        if stack.is_empty() {
            return Ok(".".to_owned());
        }
        let mut output = stack.join("/");
        if intent {
            output.push('/');
        }
        Ok(output)
    }
}

fn validate_context(path: &str) -> Result<Vec<String>, ()> {
    if !path.starts_with('/') {
        return Err(());
    }
    validate_raw(path).map_err(|_| ())?;
    let parts = components(path, 0);
    reduce_absolute(Vec::new(), parts).map_err(|_| ())
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_parts = validate_context(cwd).map_err(|_| PathError::InvalidCwd)?;
    let home_parts = match home {
        Some(value) => Some(validate_context(value).map_err(|_| PathError::InvalidHome)?),
        None => None,
    };

    validate_raw(path)?;

    let bytes = path.as_bytes();
    let (initial, remainder, offset) = if bytes[0] == b'~' {
        if bytes.len() == 1 {
            (
                home_parts.ok_or(PathError::MissingHome)?,
                "",
                bytes.len(),
            )
        } else if bytes[1] == b'/' {
            (
                home_parts.ok_or(PathError::MissingHome)?,
                &path[2..],
                2,
            )
        } else {
            return Err(PathError::UnsupportedTilde { at: 1 });
        }
    } else if bytes[0] == b'/' {
        (Vec::new(), path, 0)
    } else {
        (cwd_parts, path, 0)
    };

    let intent = directory_intent(path);
    let reduced = reduce_absolute(initial, components(remainder, offset))?;
    Ok(format_absolute(reduced, intent))
}