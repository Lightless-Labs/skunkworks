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

fn validate(path: &str) -> Result<(), PathError> {
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
            .split('/')
            .filter(|part| !part.is_empty())
            .last()
            .is_some_and(|part| part == "." || part == "..")
}

fn reduce(
    input: &str,
    start: Vec<String>,
    absolute: bool,
    offset_base: usize,
) -> Result<Vec<String>, PathError> {
    let mut components = start;
    let mut byte_offset = offset_base;

    for component in input.split('/') {
        if !component.is_empty() {
            match component {
                "." => {}
                ".." => {
                    if components.last().is_some_and(|last| last != "..") {
                        components.pop();
                    } else if absolute {
                        return Err(PathError::Escape { at: byte_offset });
                    } else {
                        components.push("..".to_string());
                    }
                }
                name => components.push(name.to_string()),
            }
        }
        byte_offset += component.len() + 1;
    }

    Ok(components)
}

fn render(components: &[String], absolute: bool, intent: bool) -> String {
    let joined = components.join("/");
    let mut result = if absolute {
        if joined.is_empty() {
            "/".to_string()
        } else {
            format!("/{joined}")
        }
    } else if joined.is_empty() {
        ".".to_string()
    } else {
        joined
    };

    if intent && result != "/" && result != "." {
        result.push('/');
    }
    result
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate(path)?;
    let absolute = path.as_bytes()[0] == b'/';
    let intent = directory_intent(path);
    let components = reduce(path, Vec::new(), absolute, 0)?;
    Ok(render(&components, absolute, intent))
}

fn normalize_context(path: &str) -> Result<Vec<String>, PathError> {
    if !path.starts_with('/') {
        return Err(PathError::InvalidCwd);
    }
    validate(path).map_err(|_| PathError::InvalidCwd)?;
    reduce(path, Vec::new(), true, 0).map_err(|_| PathError::InvalidCwd)
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_components = normalize_context(cwd)?;

    let home_components = match home {
        Some(value) => Some(normalize_context(value).map_err(|_| PathError::InvalidHome)?),
        None => None,
    };

    validate(path)?;

    let bytes = path.as_bytes();
    let (remainder, start, absolute, offset_base) = if bytes[0] == b'~' {
        if path == "~" {
            let home = home_components.ok_or(PathError::MissingHome)?;
            ("", home, true, 1)
        } else if bytes.get(1) == Some(&b'/') {
            let home = home_components.ok_or(PathError::MissingHome)?;
            (&path[2..], home, true, 2)
        } else {
            return Err(PathError::UnsupportedTilde { at: 1 });
        }
    } else if bytes[0] == b'/' {
        (path, Vec::new(), true, 0)
    } else {
        (path, cwd_components, true, 0)
    };

    let intent = directory_intent(path);
    let components = reduce(remainder, start, absolute, offset_base)?;
    Ok(render(&components, true, intent))
}

fn main() {}