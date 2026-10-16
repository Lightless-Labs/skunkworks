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

    let absolute = path.as_bytes()[0] == b'/';
    let directory_intent = path.ends_with('/');
    let mut components: Vec<(String, usize)> = Vec::new();
    let mut last_nonempty: Option<(&str, usize)> = None;

    let mut start = 0;
    for part in path.split('/') {
        let at = start;
        start += part.len() + 1;

        if part.is_empty() {
            continue;
        }

        last_nonempty = Some((part, at));

        match part {
            "." => {}
            ".." => {
                if components
                    .last()
                    .is_some_and(|(component, _)| component != "..")
                {
                    components.pop();
                } else if absolute {
                    return Err(PathError::Escape { at });
                } else {
                    components.push(("..".to_string(), at));
                }
            }
            _ => components.push((part.to_string(), at)),
        }
    }

    let directory_intent =
        directory_intent || last_nonempty.is_some_and(|(part, _)| part == "." || part == "..");

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