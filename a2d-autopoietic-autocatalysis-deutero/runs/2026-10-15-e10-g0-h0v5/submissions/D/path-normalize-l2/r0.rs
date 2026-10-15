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
            .rsplit('/')
            .find(|component| !component.is_empty())
            .is_some_and(|component| component == "." || component == "..")
}

fn components(path: &str) -> Vec<&str> {
    path.split('/').filter(|component| !component.is_empty()).collect()
}

fn reduce<'a>(
    stack: &mut Vec<&'a str>,
    path: &'a str,
    absolute: bool,
    offset: usize,
) -> Result<(), PathError> {
    let mut search_from = 0;

    for component in components(path) {
        let relative_at = path[search_from..]
            .find(component)
            .map(|i| search_from + i)
            .unwrap_or(search_from);
        let at = offset + relative_at;
        search_from = relative_at + component.len();

        match component {
            "." => {}
            ".." => {
                if stack.last().is_some_and(|last| *last != "..") {
                    stack.pop();
                } else if absolute {
                    return Err(PathError::Escape { at });
                } else {
                    stack.push("..");
                }
            }
            ordinary => stack.push(ordinary),
        }
    }

    Ok(())
}

fn format_path(stack: &[&str], absolute: bool, intent: bool) -> String {
    let mut result = if absolute {
        format!("/{}", stack.join("/"))
    } else if stack.is_empty() {
        ".".to_owned()
    } else {
        stack.join("/")
    };

    if intent && result != "/" && result != "." && !result.ends_with('/') {
        result.push('/');
    }

    result
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;

    let absolute = path.starts_with('/');
    let intent = directory_intent(path);
    let mut stack = Vec::new();
    reduce(&mut stack, path, absolute, 0)?;

    Ok(format_path(&stack, absolute, intent))
}

fn normalized_context(path: &str) -> Option<Vec<String>> {
    if !path.starts_with('/') || validate_raw(path).is_err() {
        return None;
    }

    let mut stack: Vec<&str> = Vec::new();
    reduce(&mut stack, path, true, 0).ok()?;
    Some(stack.into_iter().map(str::to_owned).collect())
}

pub fn resolve(
    path: &str,
    cwd: &str,
    home: Option<&str>,
) -> Result<String, PathError> {
    let cwd_components = normalized_context(cwd).ok_or(PathError::InvalidCwd)?;
    let home_components = match home {
        Some(home) => Some(normalized_context(home).ok_or(PathError::InvalidHome)?),
        None => None,
    };

    validate_raw(path)?;

    let intent = directory_intent(path);
    let bytes = path.as_bytes();
    let (absolute, mut stack, remainder, offset) = if bytes[0] == b'~' {
        if bytes.len() == 1 {
            let base = home_components.as_ref().ok_or(PathError::MissingHome)?;
            (
                true,
                base.iter().map(String::as_str).collect(),
                "",
                path.len(),
            )
        } else if bytes[1] == b'/' {
            let base = home_components.as_ref().ok_or(PathError::MissingHome)?;
            (
                true,
                base.iter().map(String::as_str).collect(),
                &path[2..],
                2,
            )
        } else {
            return Err(PathError::UnsupportedTilde { at: 1 });
        }
    } else if bytes[0] == b'/' {
        (true, Vec::new(), path, 0)
    } else {
        (
            true,
            cwd_components.iter().map(String::as_str).collect(),
            path,
            0,
        )
    };

    reduce(&mut stack, remainder, absolute, offset)?;
    Ok(format_path(&stack, true, intent))
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