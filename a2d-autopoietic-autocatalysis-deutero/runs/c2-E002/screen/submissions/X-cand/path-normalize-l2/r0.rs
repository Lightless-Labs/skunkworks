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
            .is_some_and(|(_, part)| *part == "." || *part == "..")
}

fn reduce(
    stack: &mut Vec<String>,
    parts: &[(usize, &str)],
    absolute: bool,
) -> Result<(), PathError> {
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
            _ => stack.push(part.to_owned()),
        }
    }
    Ok(())
}

fn render(stack: &[String], absolute: bool, intent: bool) -> String {
    let mut result = if absolute {
        format!("/{}", stack.join("/"))
    } else if stack.is_empty() {
        ".".to_owned()
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
    let parts = components(path);
    let mut stack = Vec::new();

    reduce(&mut stack, &parts, absolute)?;
    Ok(render(&stack, absolute, intent))
}

fn validated_context(path: &str, error: PathError) -> Result<Vec<String>, PathError> {
    if path.as_bytes().first() != Some(&b'/') {
        return Err(error);
    }

    match normalize(path) {
        Ok(normalized) => Ok(components(&normalized)
            .into_iter()
            .map(|(_, part)| part.to_owned())
            .collect()),
        Err(_) => Err(error),
    }
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_parts = validated_context(cwd, PathError::InvalidCwd)?;
    let home_parts = match home {
        Some(value) => Some(validated_context(value, PathError::InvalidHome)?),
        None => None,
    };

    validate_raw(path)?;

    let (mut stack, remainder, offset, absolute) = if path.starts_with('~') {
        if path != "~" && !path.starts_with("~/") {
            return Err(PathError::UnsupportedTilde { at: 1 });
        }
        let home_parts = home_parts.ok_or(PathError::MissingHome)?;
        let remainder = if path == "~" {
            ""
        } else {
            &path[2..]
        };
        (home_parts, remainder, if path == "~" { path.len() } else { 2 }, true)
    } else if path.as_bytes().first() == Some(&b'/') {
        (Vec::new(), path, 0, true)
    } else {
        (cwd_parts, path, 0, true)
    };

    let intent = directory_intent(path);
    let parts: Vec<(usize, &str)> = components(remainder)
        .into_iter()
        .map(|(at, part)| (at + offset, part))
        .collect();

    reduce(&mut stack, &parts, absolute)?;
    Ok(render(&stack, true, intent))
}