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

fn format_components(components: &[String], absolute: bool, intent: bool) -> String {
    let mut result = if absolute {
        format!("/{}", components.join("/"))
    } else if components.is_empty() {
        ".".to_string()
    } else {
        components.join("/")
    };

    if absolute && components.is_empty() {
        return "/".to_string();
    }
    if intent && result != "/" && result != "." {
        result.push('/');
    }
    result
}

fn reduce(
    components: &mut Vec<String>,
    path: &str,
    absolute: bool,
    base_offset: usize,
) -> Result<(), PathError> {
    let mut offset = base_offset;

    for component in path.split('/') {
        let component_offset = offset;
        offset += component.len() + 1;

        if component.is_empty() || component == "." {
            continue;
        }
        if component == ".." {
            if components.last().is_some_and(|last| last != "..") {
                components.pop();
            } else if absolute {
                return Err(PathError::Escape {
                    at: component_offset,
                });
            } else {
                components.push("..".to_string());
            }
        } else {
            components.push(component.to_string());
        }
    }

    Ok(())
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;

    let absolute = path.as_bytes()[0] == b'/';
    let intent = directory_intent(path);
    let mut components = Vec::new();

    reduce(&mut components, path, absolute, 0)?;
    Ok(format_components(&components, absolute, intent))
}

fn context_components(path: &str) -> Result<Vec<String>, PathError> {
    if !path.starts_with('/') {
        return Err(PathError::InvalidCwd);
    }
    let normalized = normalize(path).map_err(|_| PathError::InvalidCwd)?;
    Ok(normalized
        .split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect())
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_components = context_components(cwd)?;

    let home_components = match home {
        Some(home_path) => Some(context_components(home_path).map_err(|_| PathError::InvalidHome)?),
        None => None,
    };

    validate_raw(path)?;

    let (absolute, mut components, remainder, base_offset) = if path.starts_with('/') {
        (true, Vec::new(), path, 0)
    } else if path.starts_with('~') {
        if path == "~" {
            let components = home_components.ok_or(PathError::MissingHome)?;
            (true, components, "", path.len())
        } else if path.as_bytes().get(1) == Some(&b'/') {
            let components = home_components.ok_or(PathError::MissingHome)?;
            (true, components, &path[2..], 2)
        } else {
            return Err(PathError::UnsupportedTilde { at: 1 });
        }
    } else {
        (true, cwd_components, path, 0)
    };

    let intent = directory_intent(path);
    reduce(&mut components, remainder, absolute, base_offset)?;
    Ok(format_components(&components, true, intent))
}

fn main() {}