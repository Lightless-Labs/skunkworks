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

    let bytes = path.as_bytes();
    let absolute = bytes[0] == b'/';
    let directory_intent = bytes.last() == Some(&b'/');

    let mut components: Vec<(&str, usize)> = Vec::new();
    let mut last_nonempty: Option<(&str, usize)> = None;
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'/' {
            index += 1;
            continue;
        }

        let start = index;
        while index < bytes.len() && bytes[index] != b'/' {
            index += 1;
        }

        let component = &path[start..index];
        last_nonempty = Some((component, start));

        match component {
            "." => {}
            ".." => {
                if components.last().is_some_and(|(name, _)| *name != "..") {
                    components.pop();
                } else if absolute {
                    return Err(PathError::Escape { at: start });
                } else {
                    components.push((component, start));
                }
            }
            _ => components.push((component, start)),
        }
    }

    let directory_intent = directory_intent
        || last_nonempty.is_some_and(|(component, _)| component == "." || component == "..");

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