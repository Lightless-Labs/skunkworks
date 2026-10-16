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

    path.rsplit('/')
        .find(|component| !component.is_empty())
        .is_some_and(|component| component == "." || component == "..")
}

fn reduce_components<'a>(
    components: impl Iterator<Item = (usize, &'a str)>,
    stack: &mut Vec<&'a str>,
    absolute: bool,
) -> Result<(), PathError> {
    for (at, component) in components {
        if component.is_empty() || component == "." {
            continue;
        }
        if component == ".." {
            if stack.last().is_some_and(|last| *last != "..") {
                stack.pop();
            } else if absolute {
                return Err(PathError::Escape { at });
            } else {
                stack.push("..");
            }
        } else {
            stack.push(component);
        }
    }
    Ok(())
}

fn render(stack: &[&str], absolute: bool, wants_directory: bool) -> String {
    let mut result = String::new();

    if absolute {
        result.push('/');
    }

    result.push_str(&stack.join("/"));

    if result.is_empty() {
        result.push('.');
    }

    if wants_directory && result != "/" && result != "." {
        result.push('/');
    }

    result
}

fn normalize_validated(path: &str) -> Result<String, PathError> {
    let absolute = path.as_bytes().first() == Some(&b'/');
    let mut stack = Vec::new();

    reduce_components(
        path.match_indices('/')
            .map(|(at, _)| (at, "")) // Placeholder; components are supplied below.
            .chain(std::iter::once((0, "")))
            .take(0),
        &mut stack,
        absolute,
    )?;

    let mut offset = 0;
    let components = path.split('/').filter_map(|component| {
        let start = offset;
        offset += component.len() + 1;
        (!component.is_empty()).then_some((start, component))
    });

    reduce_components(components, &mut stack, absolute)?;
    Ok(render(&stack, absolute, directory_intent(path)))
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;
    normalize_validated(path)
}

fn validate_context(path: &str) -> Result<String, PathError> {
    if path.as_bytes().first() != Some(&b'/') {
        return Err(PathError::InvalidCwd);
    }
    normalize(path).map_err(|_| PathError::InvalidCwd)
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd = validate_context(cwd)?;

    let home = match home {
        Some(home) => {
            if home.as_bytes().first() != Some(&b'/') {
                return Err(PathError::InvalidHome);
            }
            Some(normalize(home).map_err(|_| PathError::InvalidHome)?)
        }
        None => None,
    };

    validate_raw(path)?;

    let bytes = path.as_bytes();
    let (base, remainder, remainder_offset, absolute) = if bytes.first() == Some(&b'~') {
        if bytes.len() == 1 || bytes[1] == b'/' {
            let base = home.ok_or(PathError::MissingHome)?;
            let offset = if bytes.len() > 1 { 2 } else { 1 };
            (base, &path[offset..], offset, true)
        } else {
            return Err(PathError::UnsupportedTilde { at: 1 });
        }
    } else if bytes.first() == Some(&b'/') {
        ("/".to_string(), path, 0, true)
    } else {
        (cwd, path, 0, true)
    };

    let mut stack: Vec<&str> = base.split('/').filter(|c| !c.is_empty()).collect();
    let components = remainder
        .split('/')
        .scan(remainder_offset, |offset, component| {
            let at = *offset;
            *offset += component.len() + 1;
            Some((at, component))
        });

    reduce_components(components, &mut stack, absolute)?;
    Ok(render(&stack, true, directory_intent(path)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_normalize_again() {
        assert_eq!(normalize("//p/./q"), Ok("/p/q".into()));
    }

    #[test]
    fn public_cwd() {
        assert_eq!(resolve("../file", "/work/source", None), Ok("/work/file".into()));
        assert_eq!(resolve("/log/", "/work", None), Ok("/log/".into()));
    }

    #[test]
    fn public_home() {
        assert_eq!(
            resolve("~/notes", "/work", Some("/users/me")),
            Ok("/users/me/notes".into())
        );
        assert_eq!(resolve("~", "/work", Some("/users/me")), Ok("/users/me".into()));
    }

    #[test]
    fn public_context_error() {
        assert_eq!(resolve("file", "work", None), Err(PathError::InvalidCwd));
        assert_eq!(resolve("~", "/", None), Err(PathError::MissingHome));
    }
}