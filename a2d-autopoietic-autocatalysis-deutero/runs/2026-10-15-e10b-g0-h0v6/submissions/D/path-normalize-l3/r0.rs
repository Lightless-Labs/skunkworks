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
    path.as_bytes().last() == Some(&b'/')
        || path
            .split('/')
            .filter(|part| !part.is_empty())
            .last()
            .is_some_and(|part| part == "." || part == "..")
}

fn build_absolute(parts: &[String], intent: bool) -> String {
    if parts.is_empty() {
        return "/".to_string();
    }
    let mut result = format!("/{}", parts.join("/"));
    if intent {
        result.push('/');
    }
    result
}

fn reduce_parts(
    parts: &mut Vec<String>,
    raw: &str,
    base_offset: usize,
    absolute: bool,
) -> Result<(), PathError> {
    let mut offset = 0;
    for component in raw.split('/') {
        let component_offset = offset;
        offset += component.len() + 1;

        if component.is_empty() || component == "." {
            continue;
        }
        if component == ".." {
            if parts.last().is_some_and(|last| last != "..") {
                parts.pop();
            } else if absolute {
                return Err(PathError::Escape {
                    at: base_offset + component_offset,
                });
            } else {
                parts.push("..".to_string());
            }
        } else {
            parts.push(component.to_string());
        }
    }
    Ok(())
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;
    let absolute = path.starts_with('/');
    let intent = directory_intent(path);
    let mut parts = Vec::new();
    reduce_parts(&mut parts, path, 0, absolute)?;

    if absolute {
        Ok(build_absolute(&parts, intent))
    } else {
        let mut result = if parts.is_empty() {
            ".".to_string()
        } else {
            parts.join("/")
        };
        if intent && result != "." {
            result.push('/');
        }
        Ok(result)
    }
}

fn validate_context(path: &str) -> Result<Vec<String>, PathError> {
    if !path.starts_with('/') {
        return Err(PathError::InvalidCwd);
    }
    let normalized = normalize(path).map_err(|_| PathError::InvalidCwd)?;
    Ok(normalized
        .split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect())
}

fn validate_home(path: &str) -> Result<Vec<String>, PathError> {
    if !path.starts_with('/') {
        return Err(PathError::InvalidHome);
    }
    let normalized = normalize(path).map_err(|_| PathError::InvalidHome)?;
    Ok(normalized
        .split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect())
}

fn validate_links(links: &[Link]) -> Result<(), PathError> {
    let mut seen: Vec<(&str, usize)> = Vec::new();
    for (index, link) in links.iter().enumerate() {
        let valid_path = link.path.starts_with('/')
            && !link.path.ends_with('/')
            && link.path != "/"
            && normalize(&link.path).is_ok_and(|normalized| normalized == link.path);
        if !valid_path || validate_raw(&link.target).is_err() {
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

fn request_parts<'a>(
    path: &'a str,
    home_parts: Option<&[String]>,
) -> Result<(Vec<String>, &'a str, usize), PathError> {
    if path.starts_with('~') {
        let bytes = path.as_bytes();
        if bytes.len() == 1 || bytes.get(1) == Some(&b'/') {
            let base = home_parts.ok_or(PathError::MissingHome)?.to_vec();
            let consumed = if bytes.len() > 1 { 2 } else { 1 };
            Ok((base, &path[consumed..], consumed))
        } else {
            Err(PathError::UnsupportedTilde { at: 1 })
        }
    } else if path.starts_with('/') {
        Ok((Vec::new(), path, 0))
    } else {
        Ok((Vec::new(), path, 0))
    }
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_parts = validate_context(cwd)?;
    let home_parts = match home {
        Some(value) => Some(validate_home(value)?),
        None => None,
    };

    validate_raw(path)?;
    let intent = directory_intent(path);

    let (mut parts, request, offset) = if path.starts_with('~') {
        request_parts(path, home_parts.as_deref())?
    } else if path.starts_with('/') {
        (Vec::new(), path, 0)
    } else {
        (cwd_parts, path, 0)
    };

    reduce_parts(&mut parts, request, offset, true)?;
    Ok(build_absolute(&parts, intent))
}

#[derive(Clone)]
struct Work {
    raw: String,
    offset: usize,
    pos: usize,
    owner: Option<usize>,
}

fn next_component(work: &mut Work) -> Option<(String, usize)> {
    let bytes = work.raw.as_bytes();
    while work.pos < bytes.len() && bytes[work.pos] == b'/' {
        work.pos += 1;
    }
    if work.pos >= bytes.len() {
        return None;
    }
    let start = work.pos;
    while work.pos < bytes.len() && bytes[work.pos] != b'/' {
        work.pos += 1;
    }
    Some((work.raw[start..work.pos].to_string(), work.offset + start))
}

fn link_index_at(parts: &[String], links: &[Link]) -> Option<usize> {
    let candidate = format!("/{}", parts.join("/"));
    links.iter().position(|link| link.path == candidate)
}

fn process_work(
    mut work: Work,
    stack: &mut Vec<String>,
    pending: &mut Vec<Work>,
    active: &mut Vec<usize>,
    expansions: &mut usize,
    links: &[Link],
    max_expansions: usize,
) -> Result<(), PathError> {
    while let Some((component, at)) = next_component(&mut work) {
        if component == "." {
            continue;
        }
        if component == ".." {
            if stack.pop().is_none() {
                return Err(PathError::Escape { at });
            }
            continue;
        }

        stack.push(component);
        if let Some(index) = link_index_at(stack, links) {
            if active.contains(&index) {
                return Err(PathError::Loop { index });
            }
            if *expansions >= max_expansions {
                return Err(PathError::Limit { index });
            }
            *expansions += 1;

            let remainder = work.raw[work.pos..].to_string();
            let remainder_offset = work.offset + work.pos;
            pending.push(Work {
                raw: remainder,
                offset: remainder_offset,
                pos: 0,
                owner: work.owner,
            });

            // Replace the matched link path with its target's result. A relative
            // target starts at the matched link's parent.
            stack.pop();
            active.push(index);
            pending.push(Work {
                raw: links[index].target.clone(),
                offset: 0,
                pos: 0,
                owner: Some(index),
            });
            return Ok(());
        }
    }

    if let Some(owner) = work.owner {
        if let Some(position) = active.iter().rposition(|&index| index == owner) {
            active.remove(position);
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
    let cwd_parts = validate_context(cwd)?;
    let home_parts = match home {
        Some(value) => Some(validate_home(value)?),
        None => None,
    };
    validate_links(links)?;
    validate_raw(path)?;

    let intent = directory_intent(path);
    let (base_parts, request, request_offset) = if path.starts_with('~') {
        request_parts(path, home_parts.as_deref())?
    } else if path.starts_with('/') {
        (Vec::new(), path, 0)
    } else {
        (cwd_parts.clone(), path, 0)
    };

    let absolute_request = path.starts_with('/');
    let mut stack = Vec::<String>::new();
    let mut pending = Vec::<Work>::new();
    let mut active = Vec::<usize>::new();
    let mut expansions = 0usize;

    if !absolute_request {
        for component in base_parts {
            stack.push(component);
            if let Some(index) = link_index_at(&stack, links) {
                if active.contains(&index) {
                    return Err(PathError::Loop { index });
                }
                if expansions >= max_expansions {
                    return Err(PathError::Limit { index });
                }
                expansions += 1;

                active.push(index);
                pending.push(Work {
                    raw: links[index].target.clone(),
                    offset: 0,
                    pos: 0,
                    owner: Some(index),
                });
                while let Some(work) = pending.pop() {
                    process_work(
                        work,
                        &mut stack,
                        &mut pending,
                        &mut active,
                        &mut expansions,
                        links,
                        max_expansions,
                    )?;
                }
            }
        }
    }

    pending.push(Work {
        raw: request.to_string(),
        offset: request_offset,
        pos: 0,
        owner: None,
    });

    while let Some(work) = pending.pop() {
        process_work(
            work,
            &mut stack,
            &mut pending,
            &mut active,
            &mut expansions,
            links,
            max_expansions,
        )?;
    }

    Ok(build_absolute(&stack, intent))
}