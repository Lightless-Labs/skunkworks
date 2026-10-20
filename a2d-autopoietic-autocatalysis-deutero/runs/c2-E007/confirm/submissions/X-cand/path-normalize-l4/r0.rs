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

#[derive(Clone, Copy)]
struct Component<'a> {
    text: &'a str,
    at: usize,
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

fn components(path: &str, offset: usize) -> Vec<Component<'_>> {
    let bytes = path.as_bytes();
    let mut result = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'/' {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && bytes[i] != b'/' {
            i += 1;
        }
        result.push(Component {
            text: &path[start..i],
            at: offset + start,
        });
    }
    result
}

fn has_directory_intent(path: &str) -> bool {
    if path.as_bytes().last() == Some(&b'/') {
        return true;
    }
    components(path, 0)
        .last()
        .map(|c| c.text == "." || c.text == "..")
        .unwrap_or(false)
}

fn join_absolute(stack: &[String], directory: bool) -> String {
    if stack.is_empty() {
        return "/".to_string();
    }
    let mut result = String::from("/");
    result.push_str(&stack.join("/"));
    if directory {
        result.push('/');
    }
    result
}

fn join_relative(stack: &[String], directory: bool) -> String {
    if stack.is_empty() {
        return ".".to_string();
    }
    let mut result = stack.join("/");
    if directory {
        result.push('/');
    }
    result
}

fn reduce(
    stack: &mut Vec<String>,
    items: &[Component<'_>],
    absolute: bool,
) -> Result<(), PathError> {
    for item in items {
        match item.text {
            "." => {}
            ".." => {
                if stack.last().map(|s| s.as_str()) != Some("..") && !stack.is_empty() {
                    stack.pop();
                } else if absolute {
                    return Err(PathError::Escape { at: item.at });
                } else {
                    stack.push("..".to_string());
                }
            }
            name => stack.push(name.to_string()),
        }
    }
    Ok(())
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;
    let absolute = path.as_bytes().first() == Some(&b'/');
    let directory = has_directory_intent(path);
    let mut stack = Vec::new();
    reduce(&mut stack, &components(path, 0), absolute)?;
    if absolute {
        Ok(join_absolute(&stack, directory))
    } else {
        Ok(join_relative(&stack, directory))
    }
}

fn normalized_context(value: &str, invalid: PathError) -> Result<Vec<String>, PathError> {
    if value.as_bytes().first() != Some(&b'/') {
        return Err(invalid);
    }
    let normalized = normalize(value).map_err(|_| invalid.clone())?;
    Ok(components(&normalized, 0)
        .into_iter()
        .map(|c| c.text.to_string())
        .collect())
}

fn validate_tilde(path: &str) -> Result<(), PathError> {
    if path.starts_with('~') && path != "~" && !path.starts_with("~/") {
        return Err(PathError::UnsupportedTilde { at: 1 });
    }
    Ok(())
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_components = normalized_context(cwd, PathError::InvalidCwd)?;
    let home_components = match home {
        Some(value) => Some(normalized_context(value, PathError::InvalidHome)?),
        None => None,
    };
    validate_raw(path)?;
    validate_tilde(path)?;

    let (mut stack, remainder, offset) = if path.starts_with('~') {
        let h = home_components.ok_or(PathError::MissingHome)?;
        let remainder = if path == "~" { "" } else { &path[2..] };
        let offset = if path == "~" { path.len() } else { 2 };
        (h, remainder, offset)
    } else if path.starts_with('/') {
        (Vec::new(), path, 0)
    } else {
        (cwd_components, path, 0)
    };

    reduce(&mut stack, &components(remainder, offset), true)?;
    Ok(join_absolute(&stack, has_directory_intent(path)))
}

fn validate_links(links: &[Link]) -> Result<HashMap<String, usize>, PathError> {
    let mut map = HashMap::new();
    for (index, link) in links.iter().enumerate() {
        let path_ok = link.path.starts_with('/')
            && !link.path.ends_with('/')
            && normalize(&link.path)
                .map(|p| p == link.path && link.path != "/")
                .unwrap_or(false);
        let target_ok = validate_raw(&link.target).is_ok();
        if !path_ok || !target_ok {
            return Err(PathError::InvalidLink { index });
        }
        if let Some(&first) = map.get(&link.path) {
            return Err(PathError::DuplicateLink {
                first,
                second: index,
            });
        }
        map.insert(link.path.clone(), index);
    }
    Ok(map)
}

#[derive(Clone)]
struct Work<'a> {
    items: Vec<Component<'a>>,
    pos: usize,
    link_index: Option<usize>,
}

fn expand_request<'a>(
    path: &'a str,
    cwd: &[String],
    home: Option<&[String]>,
) -> Result<(Vec<String>, Vec<Component<'a>>, bool), PathError> {
    let directory = has_directory_intent(path);
    if path.starts_with('~') {
        validate_tilde(path)?;
        let initial = home.ok_or(PathError::MissingHome)?.to_vec();
        let rest = if path == "~" { "" } else { &path[2..] };
        let offset = if path == "~" { path.len() } else { 2 };
        Ok((initial, components(rest, offset), directory))
    } else if path.starts_with('/') {
        Ok((Vec::new(), components(path, 0), directory))
    } else {
        Ok((cwd.to_vec(), components(path, 0), directory))
    }
}

fn traversal(
    initial: Vec<String>,
    request: Vec<Component<'_>>,
    links: &[Link],
    link_map: &HashMap<String, usize>,
    max_expansions: usize,
) -> Result<Vec<String>, PathError> {
    let mut stack = Vec::<String>::new();
    let mut active = Vec::<usize>::new();
    let mut expansions = 0usize;
    let mut work = Vec::<Work<'_>>::new();

    // The request frame is pushed first so context components are traversed first.
    work.push(Work {
        items: request,
        pos: 0,
        link_index: None,
    });
    work.push(Work {
        items: initial
            .iter()
            .map(|s| Component { text: s, at: 0 })
            .collect(),
        pos: 0,
        link_index: None,
    });

    while let Some(frame) = work.last_mut() {
        if frame.pos >= frame.items.len() {
            let finished = work.pop().unwrap();
            if let Some(index) = finished.link_index {
                if active.last() == Some(&index) {
                    active.pop();
                }
            }
            continue;
        }

        let component = frame.items[frame.pos];
        frame.pos += 1;
        match component.text {
            "." => continue,
            ".." => {
                if stack.pop().is_none() {
                    return Err(PathError::Escape { at: component.at });
                }
            }
            name => {
                stack.push(name.to_string());
                let key = join_absolute(&stack, false);
                if let Some(&index) = link_map.get(&key) {
                    if active.contains(&index) {
                        return Err(PathError::Loop { index });
                    }
                    if expansions >= max_expansions {
                        return Err(PathError::Limit { index });
                    }
                    expansions += 1;
                    active.push(index);

                    let link = &links[index];
                    if link.target.starts_with('/') {
                        stack.clear();
                    } else {
                        stack.pop();
                    }
                    work.push(Work {
                        items: components(&link.target, 0),
                        pos: 0,
                        link_index: Some(index),
                    });
                }
            }
        }
    }
    Ok(stack)
}

fn resolve_links_core(
    path: &str,
    cwd: &str,
    home: Option<&str>,
    links: &[Link],
    max_expansions: usize,
) -> Result<(String, bool), PathError> {
    let cwd_components = normalized_context(cwd, PathError::InvalidCwd)?;
    let home_components = match home {
        Some(value) => Some(normalized_context(value, PathError::InvalidHome)?),
        None => None,
    };
    let link_map = validate_links(links)?;
    validate_raw(path)?;
    validate_tilde(path)?;
    let (initial, request, directory) =
        expand_request(path, &cwd_components, home_components.as_deref())?;
    let stack = traversal(initial, request, links, &link_map, max_expansions)?;
    Ok((join_absolute(&stack, directory), directory))
}

pub fn resolve_links(
    path: &str,
    cwd: &str,
    home: Option<&str>,
    links: &[Link],
    max_expansions: usize,
) -> Result<String, PathError> {
    resolve_links_core(path, cwd, home, links, max_expansions).map(|v| v.0)
}

fn absolute_components(path: &str) -> Vec<String> {
    components(path, 0)
        .into_iter()
        .map(|c| c.text.to_string())
        .collect()
}

fn route(base: &[String], target: &[String]) -> String {
    let mut common = 0;
    while common < base.len() && common < target.len() && base[common] == target[common] {
        common += 1;
    }
    let mut parts = Vec::new();
    for _ in common..base.len() {
        parts.push("..".to_string());
    }
    parts.extend(target[common..].iter().cloned());
    if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
    }
}

fn candidate_with_suffix(route: String, suffix: &[String]) -> String {
    if suffix.is_empty() {
        return route;
    }
    if route == "." {
        suffix.join("/")
    } else {
        format!("{}/{}", route, suffix.join("/"))
    }
}

fn protect_tilde(candidate: String) -> String {
    if candidate.starts_with('~') {
        format!("./{}", candidate)
    } else {
        candidate
    }
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
    let target_directory = resolved_target.ends_with('/');
    let base_components = absolute_components(&resolved_base);
    let target_components = absolute_components(&resolved_target);

    let mut candidates = Vec::<String>::new();
    candidates.push(route(&base_components, &target_components));

    for link in links {
        let probe = resolve_links(&link.path, "/", None, links, max_expansions);
        let Ok(probe) = probe else {
            continue;
        };
        let alias_components = absolute_components(&probe);
        if alias_components.len() <= target_components.len()
            && alias_components
                .iter()
                .zip(target_components.iter())
                .all(|(a, b)| a == b)
        {
            let suffix = &target_components[alias_components.len()..];
            let shortcut = candidate_with_suffix(
                route(&base_components, &absolute_components(&link.path)),
                suffix,
            );
            candidates.push(shortcut);
        }
    }

    let mut valid = Vec::<String>::new();
    for candidate in candidates {
        let mut candidate = protect_tilde(candidate);
        if target_directory && candidate != "." {
            candidate.push('/');
        }
        let Ok(resolved) = resolve_links(&candidate, &resolved_base, home, links, max_expansions)
        else {
            continue;
        };
        if absolute_components(&resolved) == target_components {
            valid.push(candidate);
        }
    }

    valid.sort_by(|a, b| {
        a.len()
            .cmp(&b.len())
            .then_with(|| a.as_bytes().cmp(b.as_bytes()))
    });
    valid.into_iter().next().ok_or(PathError::Escape { at: 0 })
}