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

    let bytes = path.as_bytes();
    let absolute = bytes[0] == b'/';
    let directory_intent = bytes.last() == Some(&b'/');

    let mut components: Vec<(usize, &str)> = Vec::new();
    let mut i = 0;
    let mut last_nonempty: Option<&str> = None;

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
        last_nonempty = Some(component);

        match component {
            "." => {}
            ".." => {
                if components.last().is_some_and(|(_, c)| *c != "..") {
                    components.pop();
                } else if absolute {
                    return Err(PathError::Escape { at: start });
                } else {
                    components.push((start, component));
                }
            }
            _ => components.push((start, component)),
        }
    }

    let directory_intent =
        directory_intent || matches!(last_nonempty, Some(".") | Some(".."));

    let mut result = String::new();
    if absolute {
        result.push('/');
    }

    for (_, component) in &components {
        if result.len() > usize::from(absolute) {
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
