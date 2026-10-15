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

fn directory_intent(path: &str) -> bool {
    if path.ends_with('/') {
        return true;
    }

    path.split('/')
        .filter(|component| !component.is_empty())
        .last()
        .is_some_and(|component| component == "." || component == "..")
}

fn normalized_components(path: &str, absolute: bool) -> Result<Vec<String>, PathError> {
    let mut components: Vec<String> = Vec::new();
    let mut offset = 0;

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
                components.push("..".to_owned());
            }
        } else {
            components.push(component.to_owned());
        }
    }

    Ok(components)
}

fn format_path(components: &[String], absolute: bool, trailing: bool) -> String {
    let joined = components.join("/");

    if absolute {
        if joined.is_empty() {
            return "/".to_owned();
        }
        if trailing {
            format!("/{joined}/")
        } else {
            format!("/{joined}")
        }
    } else if joined.is_empty() {
        ".".to_owned()
    } else if trailing {
        format!("{joined}/")
    } else {
        joined
    }
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;

    let absolute = path.as_bytes()[0] == b'/';
    let trailing = directory_intent(path);
    let components = normalized_components(path, absolute)?;

    Ok(format_path(&components, absolute, trailing))
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

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_components = validate_context(cwd)?;
    let home_components = match home {
        Some(home_path) => Some(validate_context(home_path).map_err(|_| PathError::InvalidHome)?),
        None => None,
    };

    validate_raw(path)?;

    let bytes = path.as_bytes();
    let (absolute, mut components, remainder, remainder_offset) =
        if bytes[0] == b'/' {
            (true, Vec::new(), path, 0)
        } else if bytes[0] == b'~' {
            if bytes.len() > 1 && bytes[1] != b'/' {
                return Err(PathError::UnsupportedTilde { at: 1 });
            }
            let home_components = home_components.ok_or(PathError::MissingHome)?;
            let (remainder, offset) = if bytes.len() > 1 {
                (&path[2..], 2)
            } else {
                ("", 1)
            };
            (true, home_components, remainder, offset)
        } else {
            (true, cwd_components, path, 0)
        };

    for (component, relative_offset) in remainder.split('/').scan(0usize, |offset, component| {
        let current = *offset;
        *offset += component.len() + 1;
        Some((component, current))
    }) {
        if component.is_empty() || component == "." {
            continue;
        }

        if component == ".." {
            if components.pop().is_none() {
                return Err(PathError::Escape {
                    at: remainder_offset + relative_offset,
                });
            }
        } else {
            components.push(component.to_owned());
        }
    }

    let trailing = directory_intent(path);
    Ok(format_path(&components, absolute, trailing))
}

fn main() {}