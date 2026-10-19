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
    let directory_intent = bytes.last() == Some(&b'/')
        || path
            .rsplit('/')
            .find(|component| !component.is_empty())
            .is_some_and(|component| component == "." || component == "..");

    let mut components: Vec<&str> = Vec::new();
    let mut start = 0;

    while start < bytes.len() {
        while start < bytes.len() && bytes[start] == b'/' {
            start += 1;
        }

        if start == bytes.len() {
            break;
        }

        let component_start = start;
        while start < bytes.len() && bytes[start] != b'/' {
            start += 1;
        }

        let component = &path[component_start..start];

        match component {
            "." => {}
            ".." => {
                if components.last().is_some_and(|last| *last != "..") {
                    components.pop();
                } else if absolute {
                    return Err(PathError::Escape { at: component_start });
                } else {
                    components.push(component);
                }
            }
            _ => components.push(component),
        }
    }

    let mut result = String::new();

    if absolute {
        result.push('/');
    }

    result.push_str(&components.join("/"));

    if result.is_empty() {
        result.push('.');
    }

    if directory_intent && result != "/" && result != "." {
        result.push('/');
    }

    Ok(result)
}