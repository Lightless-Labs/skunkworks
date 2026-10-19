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

    let bytes = path.as_bytes();
    if let Some(at) = bytes.iter().position(|&b| b == 0) {
        return Err(PathError::Nul { at });
    }
    if let Some(at) = bytes.iter().position(|&b| b == b'\\') {
        return Err(PathError::Backslash { at });
    }
    if let Some(at) = bytes
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

fn reduce(
    stack: &mut Vec<String>,
    path: &str,
    offset_base: usize,
    absolute: bool,
) -> Result<(), PathError> {
    let bytes = path.as_bytes();
    let mut start = 0;

    while start < bytes.len() {
        while start < bytes.len() && bytes[start] == b'/' {
            start += 1;
        }
        if start == bytes.len() {
            break;
        }

        let mut end = start;
        while end < bytes.len() && bytes[end] != b'/' {
            end += 1;
        }

        match &path[start..end] {
            "." => {}
            ".." => {
                if stack.last().is_some_and(|component| component != "..") {
                    stack.pop();
                } else if absolute {
                    return Err(PathError::Escape {
                        at: offset_base + start,
                    });
                } else {
                    stack.push("..".to_owned());
                }
            }
            component => stack.push(component.to_owned()),
        }

        start = end;
    }

    Ok(())
}

fn render(components: &[String], absolute: bool, directory: bool) -> String {
    let mut result = if absolute {
        if components.is_empty() {
            "/".to_owned()
        } else {
            format!("/{}", components.join("/"))
        }
    } else if components.is_empty() {
        ".".to_owned()
    } else {
        components.join("/")
    };

    if directory && result != "/" && result != "." {
        result.push('/');
    }

    result
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;

    let absolute = path.as_bytes()[0] == b'/';
    let directory = directory_intent(path);
    let mut components = Vec::new();
    reduce(&mut components, path, 0, absolute)?;

    Ok(render(&components, absolute, directory))
}

fn validated_context(path: &str) -> Result<Vec<String>, ()> {
    if !path.starts_with('/') || validate_raw(path).is_err() {
        return Err(());
    }

    let mut components = Vec::new();
    reduce(&mut components, path, 0, true).map_err(|_| ())?;
    Ok(components)
}

pub fn resolve(
    path: &str,
    cwd: &str,
    home: Option<&str>,
) -> Result<String, PathError> {
    let cwd_components =
        validated_context(cwd).map_err(|_| PathError::InvalidCwd)?;
    let home_components = match home {
        Some(path) => Some(
            validated_context(path).map_err(|_| PathError::InvalidHome)?,
        ),
        None => None,
    };

    validate_raw(path)?;

    let bytes = path.as_bytes();
    let directory = directory_intent(path);

    let (mut components, remainder, offset_base) = if bytes[0] == b'/' {
        (Vec::new(), path, 0)
    } else if bytes[0] == b'~' {
        if bytes.len() > 1 && bytes[1] != b'/' {
            return Err(PathError::UnsupportedTilde { at: 1 });
        }

        let components = home_components.ok_or(PathError::MissingHome)?;
        let offset = if bytes.len() > 1 { 2 } else { 1 };
        (components, &path[offset..], offset)
    } else {
        (cwd_components, path, 0)
    };

    reduce(&mut components, remainder, offset_base, true)?;
    Ok(render(&components, true, directory))
}