#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathError {
    Empty,
    Nul { at: usize },
    Backslash { at: usize },
    Control { at: usize },
    Escape { at: usize },
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    if path.is_empty() {
        return Err(PathError::Empty);
    }

    if let Some(at) = path.as_bytes().iter().position(|&byte| byte == 0) {
        return Err(PathError::Nul { at });
    }

    if let Some(at) = path.as_bytes().iter().position(|&byte| byte == b'\\') {
        return Err(PathError::Backslash { at });
    }

    if let Some(at) = path
        .as_bytes()
        .iter()
        .position(|&byte| (1..=31).contains(&byte) || byte == 127)
    {
        return Err(PathError::Control { at });
    }

    let absolute = path.as_bytes()[0] == b'/';
    let directory_intent = path.ends_with('/');
    let mut components: Vec<(&str, usize)> = Vec::new();
    let mut last_nonempty: Option<&str> = None;

    let mut start = 0;
    while start < path.len() {
        while start < path.len() && path.as_bytes()[start] == b'/' {
            start += 1;
        }
        if start == path.len() {
            break;
        }

        let end = path[start..]
            .find('/')
            .map_or(path.len(), |offset| start + offset);
        let component = &path[start..end];
        last_nonempty = Some(component);

        match component {
            "." => {}
            ".." => {
                if components
                    .last()
                    .is_some_and(|(previous, _)| *previous != "..")
                {
                    components.pop();
                } else if absolute {
                    return Err(PathError::Escape { at: start });
                } else {
                    components.push((component, start));
                }
            }
            _ => components.push((component, start)),
        }
        start = end;
    }

    let directory_intent =
        directory_intent || matches!(last_nonempty, Some("." | ".."));

    let joined = components
        .iter()
        .map(|(component, _)| *component)
        .collect::<Vec<_>>()
        .join("/");

    let mut result = if absolute {
        if joined.is_empty() {
            return Ok("/".to_owned());
        }
        format!("/{joined}")
    } else if joined.is_empty() {
        ".".to_owned()
    } else {
        joined
    };

    if directory_intent && result != "/" && result != "." {
        result.push('/');
    }

    Ok(result)
}