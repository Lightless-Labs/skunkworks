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
            .split('/')
            .filter(|component| !component.is_empty())
            .last()
            .is_some_and(|component| component == "." || component == "..")
}

fn reduce(
    components: &mut Vec<String>,
    path: &str,
    offset: usize,
    absolute: bool,
) -> Result<(), PathError> {
    let bytes = path.as_bytes();
    let mut i = 0;

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
        let component = &path[start..i];

        match component {
            "." => {}
            ".." => {
                if components.last().is_some_and(|last| last != "..") {
                    components.pop();
                } else if absolute {
                    return Err(PathError::Escape { at: offset + start });
                } else {
                    components.push("..".to_owned());
                }
            }
            _ => components.push(component.to_owned()),
        }
    }

    Ok(())
}

fn render(components: &[String], absolute: bool, dir_intent: bool) -> String {
    let mut result = if absolute {
        let joined = components.join("/");
        if joined.is_empty() {
            "/".to_owned()
        } else {
            format!("/{joined}")
        }
    } else if components.is_empty() {
        ".".to_owned()
    } else {
        components.join("/")
    };

    if dir_intent && result != "/" && result != "." && !result.ends_with('/') {
        result.push('/');
    }
    result
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;

    let absolute = path.starts_with('/');
    let mut components = Vec::new();
    reduce(&mut components, path, 0, absolute)?;

    Ok(render(&components, absolute, directory_intent(path)))
}

fn validated_context(path: &str) -> Result<Vec<String>, ()> {
    if !path.starts_with('/') || validate_raw(path).is_err() {
        return Err(());
    }

    let mut components = Vec::new();
    reduce(&mut components, path, 0, true).map_err(|_| ())?;
    Ok(components)
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_components = validated_context(cwd).map_err(|_| PathError::InvalidCwd)?;
    let home_components = match home {
        Some(home_path) => Some(validated_context(home_path).map_err(|_| PathError::InvalidHome)?),
        None => None,
    };

    validate_raw(path)?;

    let (absolute, mut components, remainder, offset) = if path.starts_with('~') {
        let bytes = path.as_bytes();
        if bytes.len() > 1 && bytes[1] != b'/' {
            return Err(PathError::UnsupportedTilde { at: 1 });
        }
        let home_components = home_components.ok_or(PathError::MissingHome)?;
        let offset = if bytes.len() > 1 { 2 } else { 1 };
        let remainder = if bytes.len() > 1 { &path[2..] } else { "" };
        (true, home_components, remainder, offset)
    } else if path.starts_with('/') {
        (true, Vec::new(), path, 0)
    } else {
        (true, cwd_components, path, 0)
    };

    reduce(&mut components, remainder, offset, absolute)?;
    Ok(render(
        &components,
        absolute,
        directory_intent(path),
    ))
}

fn main() {}