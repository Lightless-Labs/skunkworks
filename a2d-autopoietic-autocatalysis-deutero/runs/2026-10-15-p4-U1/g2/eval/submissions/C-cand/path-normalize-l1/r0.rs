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

    let components: Vec<(usize, &str)> = path
        .split('/')
        .scan(0usize, |offset, component| {
            let start = *offset;
            *offset += component.len() + 1;
            Some((start, component))
        })
        .filter(|(_, component)| !component.is_empty())
        .collect();

    let last_nonempty_is_directory_marker = components
        .last()
        .is_some_and(|(_, component)| *component == "." || *component == "..");
    let directory_intent = directory_intent || last_nonempty_is_directory_marker;

    let mut stack: Vec<&str> = Vec::new();

    for &(at, component) in &components {
        match component {
            "." => {}
            ".." => {
                if stack.last().is_some_and(|previous| *previous != "..") {
                    stack.pop();
                } else if absolute {
                    return Err(PathError::Escape { at });
                } else {
                    stack.push("..");
                }
            }
            _ => stack.push(component),
        }
    }

    let mut result = String::new();
    if absolute {
        result.push('/');
    }
    result.push_str(&stack.join("/"));

    if result.is_empty() {
        result.push('.');
    }

    if directory_intent && result != "/" && result != "." {
        result.push('/');
    }

    Ok(result)
}

fn main() {}