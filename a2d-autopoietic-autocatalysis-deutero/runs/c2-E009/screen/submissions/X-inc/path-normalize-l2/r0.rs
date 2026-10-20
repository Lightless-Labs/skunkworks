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

fn components(path: &str) -> impl Iterator<Item = (usize, &str)> {
    path.split('/')
        .scan(0usize, |offset, component| {
            let start = *offset;
            *offset += component.len() + 1;
            Some((start, component))
        })
        .filter(|(_, component)| !component.is_empty())
}

fn reduce(
    stack: &mut Vec<String>,
    path: &str,
    absolute: bool,
    offset_base: usize,
) -> Result<(), PathError> {
    for (at, component) in components(path) {
        match component {
            "." => {}
            ".." => {
                if stack.last().is_some_and(|last| last != "..") {
                    stack.pop();
                } else if absolute {
                    return Err(PathError::Escape {
                        at: offset_base + at,
                    });
                } else {
                    stack.push("..".to_owned());
                }
            }
            ordinary => stack.push(ordinary.to_owned()),
        }
    }
    Ok(())
}

fn render(stack: &[String], absolute: bool, directory_intent: bool) -> String {
    let mut result = if absolute {
        format!("/{}", stack.join("/"))
    } else if stack.is_empty() {
        ".".to_owned()
    } else {
        stack.join("/")
    };

    if directory_intent && result != "/" && result != "." {
        result.push('/');
    }
    result
}

fn directory_intent(path: &str) -> bool {
    path.ends_with('/')
        || components(path)
            .last()
            .is_some_and(|(_, component)| component == "." || component == "..")
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;

    let absolute = path.as_bytes()[0] == b'/';
    let intent = directory_intent(path);
    let mut stack = Vec::new();
    reduce(&mut stack, path, absolute, 0)?;

    Ok(render(&stack, absolute, intent))
}

fn validated_absolute_context(path: &str) -> Result<Vec<String>, ()> {
    if !path.starts_with('/') {
        return Err(());
    }
    validate_raw(path).map_err(|_| ())?;

    let mut stack = Vec::new();
    reduce(&mut stack, path, true, 0).map_err(|_| ())?;
    Ok(stack)
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_stack = validated_absolute_context(cwd).map_err(|_| PathError::InvalidCwd)?;
    let home_stack = match home {
        Some(home) => Some(
            validated_absolute_context(home).map_err(|_| PathError::InvalidHome)?,
        ),
        None => None,
    };

    validate_raw(path)?;

    let (absolute, mut stack, remainder, offset_base) = if path.starts_with('/') {
        (true, Vec::new(), path, 0)
    } else if path.starts_with('~') {
        let bytes = path.as_bytes();
        if bytes.len() > 1 && bytes[1] != b'/' {
            return Err(PathError::UnsupportedTilde { at: 1 });
        }
        let stack = home_stack.ok_or(PathError::MissingHome)?;
        let remainder = if bytes.len() > 1 { &path[2..] } else { "" };
        (true, stack, remainder, if bytes.len() > 1 { 2 } else { 1 })
    } else {
        (true, cwd_stack, path, 0)
    };

    reduce(&mut stack, remainder, absolute, offset_base)?;

    Ok(render(&stack, true, directory_intent(path)))
}