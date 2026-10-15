#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathError {
    Empty,
    Nul { at: usize },
    Backslash { at: usize },
    Control { at: usize },
    Escape { at: usize },
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    let bytes = path.as_bytes();

    if bytes.is_empty() {
        return Err(PathError::Empty);
    }

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

    let absolute = bytes[0] == b'/';
    let directory_intent = bytes[bytes.len() - 1] == b'/'
        || path
            .rsplit('/')
            .find(|component| !component.is_empty())
            .is_some_and(|component| component == "." || component == "..");

    let mut components: Vec<(&str, usize)> = Vec::new();
    let mut offset = 0;

    for component in path.split('/') {
        let component_start = offset;
        offset += component.len() + 1;

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
                return Err(PathError::Escape { at: component_start });
            } else {
                components.push((component, component_start));
            }
        } else {
            components.push((component, component_start));
        }
    }

    let mut result = String::new();
    if absolute {
        result.push('/');
    }

    for (index, (component, _)) in components.iter().enumerate() {
        if index > 0 {
            result.push('/');
        }
        result.push_str(component);
    }

    if result.is_empty() {
        result.push('.');
    }

    if directory_intent && result != "/" && result != "." {
        result.push('/');
    }

    Ok(result)
}