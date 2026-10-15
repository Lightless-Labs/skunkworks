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

    if let Some(at) = path.bytes().position(|byte| byte == 0) {
        return Err(PathError::Nul { at });
    }

    if let Some(at) = path.bytes().position(|byte| byte == b'\\') {
        return Err(PathError::Backslash { at });
    }

    if let Some(at) = path
        .bytes()
        .position(|byte| (1..=31).contains(&byte) || byte == 127)
    {
        return Err(PathError::Control { at });
    }

    let absolute = path.as_bytes()[0] == b'/';
    let directory_intent = path.ends_with('/')
        || path
            .rsplit('/')
            .find(|component| !component.is_empty())
            .is_some_and(|component| component == "." || component == "..");

    let mut components: Vec<(&str, usize)> = Vec::new();

    for (at, component) in path.split('/').scan(0usize, |offset, component| {
        let at = *offset;
        *offset += component.len() + 1;
        Some((at, component))
    }) {
        if component.is_empty() || component == "." {
            continue;
        }

        if component == ".." {
            if components
                .last()
                .is_some_and(|(previous, _)| *previous != "..")
            {
                components.pop();
            } else if absolute {
                return Err(PathError::Escape { at });
            } else {
                components.push((component, at));
            }
        } else {
            components.push((component, at));
        }
    }

    let joined = components
        .iter()
        .map(|(component, _)| *component)
        .collect::<Vec<_>>()
        .join("/");

    let mut result = if absolute {
        if joined.is_empty() {
            "/".to_owned()
        } else {
            format!("/{joined}")
        }
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