use std::collections::HashMap;

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
    if path.ends_with('/') {
        return true;
    }
    path.split('/')
        .filter(|component| !component.is_empty())
        .next_back()
        .is_some_and(|component| component == "." || component == "..")
}

fn components(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|component| !component.is_empty())
        .map(str::to_owned)
        .collect()
}

fn normalize_parts(
    path: &str,
    initial: &mut Vec<String>,
    absolute: bool,
    offset_base: usize,
) -> Result<(), PathError> {
    let bytes = path.as_bytes();
    let mut start = 0;

    while start < bytes.len() {
        while start < bytes.len() && bytes[start] == b'/' {
            start += 1;
        }
        if start == bytes.len() {
            break;
        }
        let end = bytes[start..]
            .iter()
            .position(|&b| b == b'/')
            .map_or(bytes.len(), |n| start + n);
        let component = &path[start..end];

        match component {
            "." => {}
            ".." => {
                if initial.last().is_some_and(|s| s != "..") {
                    initial.pop();
                } else if absolute {
                    return Err(PathError::Escape {
                        at: offset_base + start,
                    });
                } else {
                    initial.push("..".to_owned());
                }
            }
            _ => initial.push(component.to_owned()),
        }

        start = end;
    }
    Ok(())
}

fn render(parts: &[String], absolute: bool, intent: bool) -> String {
    let joined = parts.join("/");
    let mut result = if absolute {
        if joined.is_empty() {
            "/".to_owned()
        } else {
            format!("/{joined}")
        }
    } else if joined.is_empty() {
        ".".to_owned()
    } else {
        joined
    };

    if intent && result != "/" && result != "." {
        result.push('/');
    }
    result
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;
    let absolute = path.starts_with('/');
    let intent = directory_intent(path);
    let mut parts = Vec::new();
    normalize_parts(path, &mut parts, absolute, 0)?;
    Ok(render(&parts, absolute, intent))
}

fn validate_context(path: &str) -> Result<Vec<String>, PathError> {
    if !path.starts_with('/') {
        return Err(PathError::InvalidCwd);
    }
    match normalize(path) {
        Ok(normalized) => Ok(components(&normalized)),
        Err(_) => Err(PathError::InvalidCwd),
    }
}

fn validate_home(home: &str) -> Result<Vec<String>, PathError> {
    if !home.starts_with('/') {
        return Err(PathError::InvalidHome);
    }
    match normalize(home) {
        Ok(normalized) => Ok(components(&normalized)),
        Err(_) => Err(PathError::InvalidHome),
    }
}

fn validate_request_tilde<'a>(
    path: &'a str,
    home: Option<&'a [String]>,
) -> Result<(bool, Vec<String>, usize), PathError> {
    let tilde = path.starts_with('~');
    if !tilde {
        return Ok((false, Vec::new(), 0));
    }

    if path.len() > 1 && !path[1..].starts_with('/') {
        return Err(PathError::UnsupportedTilde { at: 1 });
    }
    let home = home.ok_or(PathError::MissingHome)?;
    let skip = if path.len() > 1 { 2 } else { 1 };
    Ok((true, home.to_vec(), skip))
}

fn resolve_core(
    path: &str,
    cwd: &[String],
    home: Option<&[String]>,
) -> Result<String, PathError> {
    validate_raw(path)?;
    let intent = directory_intent(path);
    let (tilde, tilde_base, skip) = validate_request_tilde(path, home)?;

    let absolute = path.starts_with('/') && !tilde;
    let mut parts = if absolute {
        Vec::new()
    } else if tilde {
        tilde_base
    } else {
        cwd.to_vec()
    };

    let remainder = &path[skip..];
    normalize_parts(remainder, &mut parts, true, skip)?;
    Ok(render(&parts, true, intent))
}

pub fn resolve(
    path: &str,
    cwd: &str,
    home: Option<&str>,
) -> Result<String, PathError> {
    let cwd_parts = validate_context(cwd)?;
    let home_parts = match home {
        Some(value) => Some(validate_home(value)?),
        None => None,
    };
    resolve_core(path, &cwd_parts, home_parts.as_deref())
}

fn validate_links(links: &[Link]) -> Result<HashMap<String, usize>, PathError> {
    let mut indices = HashMap::new();
    for (index, link) in links.iter().enumerate() {
        let path_valid = link.path.starts_with('/')
            && !link.path.ends_with('/')
            && link.path != "/"
            && normalize(&link.path).is_ok_and(|normalized| normalized == link.path);
        let target_valid = validate_raw(&link.target).is_ok();

        if !path_valid || !target_valid {
            return Err(PathError::InvalidLink { index });
        }

        if let Some(&first) = indices.get(&link.path) {
            return Err(PathError::DuplicateLink {
                first,
                second: index,
            });
        }
        indices.insert(link.path.clone(), index);
    }
    Ok(indices)
}

#[derive(Clone)]
struct Pending {
    text: String,
    offset_base: usize,
    cursor: usize,
    is_request: bool,
    active_link: Option<usize>,
}

fn next_component(pending: &mut Pending) -> Option<(String, usize)> {
    let bytes = pending.text.as_bytes();
    while pending.cursor < bytes.len() && bytes[pending.cursor] == b'/' {
        pending.cursor += 1;
    }
    if pending.cursor >= bytes.len() {
        return None;
    }
    let start = pending.cursor;
    let end = bytes[start..]
        .iter()
        .position(|&b| b == b'/')
        .map_or(bytes.len(), |n| start + n);
    pending.cursor = end;
    Some((
        pending.text[start..end].to_owned(),
        pending.offset_base + start,
    ))
}

fn push_component(
    stack: &mut Vec<String>,
    component: &str,
    absolute: bool,
    offset: usize,
) -> Result<bool, PathError> {
    match component {
        "." => Ok(false),
        ".." => {
            if stack.last().is_some_and(|s| s != "..") {
                stack.pop();
            } else if absolute {
                return Err(PathError::Escape { at: offset });
            } else {
                stack.push("..".to_owned());
            }
            Ok(false)
        }
        _ => {
            stack.push(component.to_owned());
            Ok(true)
        }
    }
}

fn resolve_links_core(
    path: &str,
    cwd: &[String],
    home: Option<&[String]>,
    links: &[Link],
    index_by_path: &HashMap<String, usize>,
    max_expansions: usize,
) -> Result<String, PathError> {
    validate_raw(path)?;
    let intent = directory_intent(path);
    let (tilde, tilde_base, skip) = validate_request_tilde(path, home)?;
    let absolute_request = path.starts_with('/') && !tilde;

    let mut stack = if absolute_request {
        Vec::new()
    } else if tilde {
        tilde_base
    } else {
        cwd.to_vec()
    };

    let mut pending = vec![Pending {
        text: path[skip..].to_owned(),
        offset_base: skip,
        cursor: 0,
        is_request: true,
        active_link: None,
    }];
    let mut active = Vec::<usize>::new();
    let mut expansions = 0usize;

    while let Some(current) = pending.last_mut() {
        let Some((component, offset)) = next_component(current) else {
            let finished = pending.pop().unwrap();
            if let Some(index) = finished.active_link {
                let popped = active.pop();
                debug_assert_eq!(popped, Some(index));
            }
            continue;
        };

        if !push_component(&mut stack, &component, true, offset)? {
            continue;
        }

        let key = format!("/{}", stack.join("/"));
        if let Some(&link_index) = index_by_path.get(&key) {
            if active.contains(&link_index) {
                return Err(PathError::Loop { index: link_index });
            }
            if expansions >= max_expansions {
                return Err(PathError::Limit { index: link_index });
            }
            expansions += 1;

            // Replace the just-pushed link name with its target's starting point.
            stack.pop();
            let target = &links[link_index].target;
            if target.starts_with('/') {
                stack.clear();
            }
            active.push(link_index);
            pending.push(Pending {
                text: target.clone(),
                offset_base: 0,
                cursor: 0,
                is_request: false,
                active_link: Some(link_index),
            });
        }
    }

    Ok(render(&stack, true, intent))
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
    let index_by_path = validate_links(links)?;
    resolve_links_core(
        path,
        &cwd_parts,
        home_parts.as_deref(),
        links,
        &index_by_path,
        max_expansions,
    )
}

fn route(base: &[String], target: &[String]) -> String {
    let mut common = 0;
    while common < base.len() && common < target.len() && base[common] == target[common] {
        common += 1;
    }
    let mut result = Vec::new();
    result.extend((common..base.len()).map(|_| "..".to_owned()));
    result.extend(target[common..].iter().cloned());
    if result.is_empty() {
        ".".to_owned()
    } else {
        result.join("/")
    }
}

fn absolute_components(path: &str) -> Vec<String> {
    components(path)
}

pub fn relative_to(
    base: &str,
    target: &str,
    cwd: &str,
    home: Option<&str>,
    links: &[Link],
    max_expansions: usize,
) -> Result<String, PathError> {
    let resolved_base = resolve_links(base, cwd, home, links, max_expansions)?;
    let resolved_target = resolve_links(target, cwd, home, links, max_expansions)?;
    let base_parts = absolute_components(&resolved_base);
    let target_parts = absolute_components(&resolved_target);
    let target_is_directory = resolved_target.ends_with('/');

    let mut candidates = vec![route(&base_parts, &target_parts)];
    let slash_cwd = "/";
    for link in links {
        let Ok(probe) = resolve_links(
            &link.path,
            slash_cwd,
            None,
            links,
            max_expansions,
        ) else {
            continue;
        };
        let alias_parts = absolute_components(&probe);
        if alias_parts.len() > target_parts.len()
            || alias_parts
                .iter()
                .zip(target_parts.iter())
                .any(|(a, b)| a != b)
        {
            continue;
        }

        let mut candidate = route(&base_parts, &components(&link.path));
        let suffix = &target_parts[alias_parts.len()..];
        if !suffix.is_empty() {
            if candidate == "." {
                candidate = suffix.join("/");
            } else {
                candidate.push('/');
                candidate.push_str(&suffix.join("/"));
            }
        }
        candidates.push(candidate);
    }

    let mut valid = Vec::<String>::new();
    for mut candidate in candidates {
        if candidate.starts_with('~') {
            candidate.insert_str(0, "./");
        }
        if target_is_directory && candidate != "." && !candidate.ends_with('/') {
            candidate.push('/');
        }

        let Ok(resolved) = resolve_links(
            &candidate,
            &resolved_base,
            home,
            links,
            max_expansions,
        ) else {
            continue;
        };
        if absolute_components(&resolved) == target_parts {
            valid.push(candidate);
        }
    }

    valid.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes())));
    valid
        .into_iter()
        .next()
        .ok_or(PathError::Escape { at: 0 })
}