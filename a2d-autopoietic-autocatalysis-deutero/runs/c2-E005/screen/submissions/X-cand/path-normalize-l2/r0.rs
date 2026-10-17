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

    Ok(())
}

fn directory_intent(path: &str) -> bool {
    if path.ends_with('/') {
        return true;
    }

    path.rsplit('/').find(|component| !component.is_empty())
        .is_some_and(|component| component == "." || component == "..")
}

fn reduce_components<'a, I>(
    components: I,
    absolute: bool,
) -> Result<Vec<&'a str>, usize>
where
    I: IntoIterator<Item = (&'a str, usize)>,
{
    let mut result = Vec::new();

    for (component, offset) in components {
        match component {
            "" | "." => {}
            ".." => {
                if result.last().is_some_and(|last| *last != "..") {
                    result.pop();
                } else if absolute {
                    return Err(offset);
                } else {
                    result.push("..");
                }
            }
            _ => result.push(component),
        }
    }

    Ok(result)
}

fn render(components: &[&str], absolute: bool, directory: bool) -> String {
    let mut result = String::new();

    if absolute {
        result.push('/');
    }

    result.push_str(&components.join("/"));

    if result.is_empty() {
        result.push('.');
    }

    if directory && result != "/" && result != "." {
        result.push('/');
    }

    result
}

fn raw_components(path: &str) -> impl Iterator<Item = (&str, usize)> {
    path.split('/')
        .scan(0usize, |offset, component| {
            let start = *offset;
            *offset += component.len() + 1;
            Some((component, start))
        })
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;

    let absolute = path.starts_with('/');
    let components = reduce_components(raw_components(path), absolute)
        .map_err(|at| PathError::Escape { at })?;

    Ok(render(&components, absolute, directory_intent(path)))
}

fn normalized_context(path: &str) -> Result<Vec<String>, ()> {
    if !path.starts_with('/') || validate_raw(path).is_err() {
        return Err(());
    }

    let components = reduce_components(raw_components(path), true).map_err(|_| ())?;
    Ok(components.into_iter().map(str::to_owned).collect())
}

pub fn resolve(
    path: &str,
    cwd: &str,
    home: Option<&str>,
) -> Result<String, PathError> {
    let cwd_components = normalized_context(cwd).map_err(|_| PathError::InvalidCwd)?;

    let home_components = match home {
        Some(home) => Some(
            normalized_context(home).map_err(|_| PathError::InvalidHome)?,
        ),
        None => None,
    };

    validate_raw(path)?;

    let (absolute, base, remainder, remainder_offset) =
        if path.starts_with('~') {
            let bytes = path.as_bytes();
            if bytes.len() > 1 && bytes[1] != b'/' {
                return Err(PathError::UnsupportedTilde { at: 1 });
            }

            let base = home_components
                .as_ref()
                .ok_or(PathError::MissingHome)?;

            let skip = if bytes.len() > 1 { 2 } else { 1 };
            (true, base.clone(), &path[skip..], skip)
        } else if path.starts_with('/') {
            (true, Vec::new(), path, 0)
        } else {
            (true, cwd_components, path, 0)
        };

    let mut components: Vec<String> = base;
    for (component, offset) in raw_components(remainder) {
        if component.is_empty() || component == "." {
            continue;
        }

        if component == ".." {
            if components.pop().is_none() {
                return Err(PathError::Escape {
                    at: remainder_offset + offset,
                });
            }
        } else {
            components.push(component.to_owned());
        }
    }

    let borrowed: Vec<&str> = components.iter().map(String::as_str).collect();
    Ok(render(&borrowed, absolute, directory_intent(path)))
}