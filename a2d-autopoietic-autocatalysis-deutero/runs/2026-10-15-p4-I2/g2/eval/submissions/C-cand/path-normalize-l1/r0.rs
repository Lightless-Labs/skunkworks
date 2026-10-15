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

    let absolute = path.as_bytes()[0] == b'/';
    let directory_intent = path.ends_with('/')
        || path
            .split('/')
            .filter(|component| !component.is_empty())
            .last()
            .is_some_and(|component| component == "." || component == "..");

    let mut components: Vec<(usize, &str)> = Vec::new();

    for (at, component) in path
        .split('/')
        .scan(0usize, |offset, component| {
            let at = *offset;
            *offset += component.len() + 1;
            Some((at, component))
        })
    {
        if component.is_empty() || component == "." {
            continue;
        }

        if component == ".." {
            if components.last().is_some_and(|(_, last)| *last != "..") {
                components.pop();
            } else if absolute {
                return Err(PathError::Escape { at });
            } else {
                components.push((at, component));
            }
        } else {
            components.push((at, component));
        }
    }

    let mut result = String::new();
    if absolute {
        result.push('/');
    }

    for (index, (_, component)) in components.iter().enumerate() {
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

fn main() {}