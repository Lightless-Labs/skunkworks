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
    InvalidLink { index: usize },
    DuplicateLink { first: usize, second: usize },
    Loop { index: usize },
    Limit { index: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub path: String,
    pub target: String,
}

fn validate_raw(path: &str) -> Result<(), PathError> {
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
    Ok(())
}

fn directory_intent(path: &str) -> bool {
    path.ends_with('/')
        || path
            .split('/')
            .filter(|component| !component.is_empty())
            .last()
            .is_some_and(|component| component == "." || component == "..")
}

fn components(path: &str) -> Vec<(String, usize)> {
    let mut result = Vec::new();
    let bytes = path.as_bytes();
    let mut start = 0;

    while start < bytes.len() {
        while start < bytes.len() && bytes[start] == b'/' {
            start += 1;
        }
        if start == bytes.len() {
            break;
        }

        let mut end = start;
        while end < bytes.len() && bytes[end] != b'/' {
            end += 1;
        }

        result.push((path[start..end].to_owned(), start));
        start = end;
    }

    result
}

fn render(parts: &[String], absolute: bool, intent: bool) -> String {
    let mut result = if absolute {
        format!("/{}", parts.join("/"))
    } else if parts.is_empty() {
        ".".to_owned()
    } else {
        parts.join("/")
    };

    if absolute && parts.is_empty() {
        return "/".to_owned();
    }
    if intent && result != "/" && result != "." {
        result.push('/');
    }
    result
}

fn reduce(
    parts: &mut Vec<String>,
    component: &str,
    at: usize,
    absolute: bool,
) -> Result<(), PathError> {
    match component {
        "." => {}
        ".." => {
            if parts.last().is_some_and(|part| part != "..") {
                parts.pop();
            } else if absolute {
                return Err(PathError::Escape { at });
            } else {
                parts.push("..".to_owned());
            }
        }
        _ => parts.push(component.to_owned()),
    }
    Ok(())
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;
    let absolute = path.starts_with('/');
    let intent = directory_intent(path);
    let mut parts = Vec::new();

    for (component, at) in components(path) {
        reduce(&mut parts, &component, at, absolute)?;
    }

    Ok(render(&parts, absolute, intent))
}

fn validated_context(path: &str, invalid: PathError) -> Result<Vec<String>, PathError> {
    if !path.starts_with('/') {
        return Err(invalid);
    }

    let normalized = normalize(path).map_err(|_| invalid)?;
    Ok(components(&normalized)
        .into_iter()
        .map(|(component, _)| component)
        .collect())
}

fn expand_tilde<'a>(
    path: &'a str,
    home: Option<&'a [String]>,
) -> Result<(&'a str, bool, Option<Vec<String>>), PathError> {
    if !path.starts_with('~') {
        return Ok((path, path.starts_with('/'), None));
    }

    let bytes = path.as_bytes();
    if bytes.len() > 1 && bytes[1] != b'/' {
        return Err(PathError::UnsupportedTilde { at: 1 });
    }

    let home = home.ok_or(PathError::MissingHome)?;
    let remainder = if bytes.len() > 1 { &path[2..] } else { "" };
    Ok((remainder, true, Some(home.to_vec())))
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_parts = validated_context(cwd, PathError::InvalidCwd)?;
    let home_parts = match home {
        Some(home) => Some(validated_context(home, PathError::InvalidHome)?),
        None => None,
    };

    validate_raw(path)?;
    let (remainder, absolute, tilde_parts) =
        expand_tilde(path, home_parts.as_deref())?;
    let intent = directory_intent(path);

    let mut parts = if let Some(home) = tilde_parts {
        home
    } else if absolute {
        Vec::new()
    } else {
        cwd_parts
    };

    for (component, at) in components(remainder) {
        reduce(&mut parts, &component, at, true)?;
    }

    Ok(render(&parts, true, intent))
}

fn validate_links(links: &[Link]) -> Result<(), PathError> {
    let mut seen: Vec<(&str, usize)> = Vec::new();

    for (index, link) in links.iter().enumerate() {
        let path_valid = link.path.starts_with('/')
            && !link.path.ends_with('/')
            && link.path != "/"
            && normalize(&link.path).is_ok_and(|normalized| normalized == link.path);

        if !path_valid || validate_raw(&link.target).is_err() {
            return Err(PathError::InvalidLink { index });
        }

        if let Some((_, first)) = seen.iter().find(|(path, _)| *path == link.path) {
            return Err(PathError::DuplicateLink {
                first: *first,
                second: index,
            });
        }

        seen.push((&link.path, index));
    }

    Ok(())
}

fn current_path(parts: &[String]) -> String {
    format!("/{}", parts.join("/"))
}

fn traverse_with_links(
    parts: &mut Vec<String>,
    input: &str,
    links: &[Link],
    active: &mut Vec<usize>,
    expansions: &mut usize,
    max_expansions: usize,
    context_offsets: bool,
) -> Result<(), PathError> {
    for (component, at) in components(input) {
        let at = if context_offsets { 0 } else { at };

        if component == "." || component == ".." {
            reduce(parts, &component, at, true)?;
            continue;
        }

        parts.push(component);
        let matched = current_path(parts);

        if let Some(index) = links.iter().position(|link| link.path == matched) {
            if active.contains(&index) {
                return Err(PathError::Loop { index });
            }
            if *expansions >= max_expansions {
                return Err(PathError::Limit { index });
            }

            *expansions += 1;
            active.push(index);
            parts.pop();

            let target = &links[index].target;
            if target.starts_with('/') {
                parts.clear();
            }

            traverse_with_links(
                parts,
                target,
                links,
                active,
                expansions,
                max_expansions,
                false,
            )?;

            active.pop();
        }
    }

    Ok(())
}

pub fn resolve_links(
    path: &str,
    cwd: &str,
    home: Option<&str>,
    links: &[Link],
    max_expansions: usize,
) -> Result<String, PathError> {
    let cwd_parts = validated_context(cwd, PathError::InvalidCwd)?;
    let home_parts = match home {
        Some(home) => Some(validated_context(home, PathError::InvalidHome)?),
        None => None,
    };

    validate_links(links)?;
    validate_raw(path)?;

    let (remainder, absolute, tilde_parts) =
        expand_tilde(path, home_parts.as_deref())?;
    let intent = directory_intent(path);

    let mut parts = Vec::new();
    let mut active = Vec::new();
    let mut expansions = 0;

    if let Some(home) = tilde_parts {
        let context = format!("/{}", home.join("/"));
        traverse_with_links(
            &mut parts,
            &context,
            links,
            &mut active,
            &mut expansions,
            max_expansions,
            true,
        )?;
    } else if !absolute {
        let context = format!("/{}", cwd_parts.join("/"));
        traverse_with_links(
            &mut parts,
            &context,
            links,
            &mut active,
            &mut expansions,
            max_expansions,
            true,
        )?;
    }

    traverse_with_links(
        &mut parts,
        remainder,
        links,
        &mut active,
        &mut expansions,
        max_expansions,
        false,
    )?;

    Ok(render(&parts, true, intent))
}