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

#[derive(Clone)]
struct Component {
    text: String,
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

fn components(path: &str, base: usize) -> Vec<Component> {
    let mut result = Vec::new();
    let mut start = 0;
    for (i, b) in path.bytes().enumerate() {
        if b == b'/' {
            if start < i {
                result.push(Component {
                    text: path[start..i].to_string(),
                    at: base + start,
                });
            }
            start = i + 1;
        }
    }
    if start < path.len() {
        result.push(Component {
            text: path[start..].to_string(),
            at: base + start,
        });
    }
    result
}

fn directory_intent(path: &str) -> bool {
    if path.ends_with('/') {
        return true;
    }
    components(path, 0)
        .last()
        .is_some_and(|c| c.text == "." || c.text == "..")
}

fn format_relative(parts: &[String], intent: bool) -> String {
    let mut value = if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
    };
    if intent && value != "." {
        value.push('/');
    }
    value
}

fn format_absolute(parts: &[String], intent: bool) -> String {
    let mut value = if parts.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", parts.join("/"))
    };
    if intent && value != "/" {
        value.push('/');
    }
    value
}

fn reduce(
    stack: &mut Vec<String>,
    items: &[Component],
    absolute: bool,
) -> Result<(), PathError> {
    for item in items {
        match item.text.as_str() {
            "." => {}
            ".." => {
                if stack.last().is_some_and(|p| p != "..") {
                    stack.pop();
                } else if absolute {
                    return Err(PathError::Escape { at: item.at });
                } else {
                    stack.push("..".to_string());
                }
            }
            _ => stack.push(item.text.clone()),
        }
    }
    Ok(())
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;
    let absolute = path.starts_with('/');
    let intent = directory_intent(path);
    let mut stack = Vec::new();
    reduce(&mut stack, &components(path, 0), absolute)?;
    if absolute {
        Ok(format_absolute(&stack, intent))
    } else {
        Ok(format_relative(&stack, intent))
    }
}

fn normalized_context(value: &str) -> Result<Vec<String>, ()> {
    if !value.starts_with('/') {
        return Err(());
    }
    let normalized = normalize(value).map_err(|_| ())?;
    Ok(components(&normalized, 0)
        .into_iter()
        .map(|c| c.text)
        .collect())
}

fn validate_contexts(cwd: &str, home: Option<&str>) -> Result<(Vec<String>, Option<Vec<String>>), PathError> {
    let cwd_parts = normalized_context(cwd).map_err(|_| PathError::InvalidCwd)?;
    let home_parts = match home {
        Some(h) => Some(normalized_context(h).map_err(|_| PathError::InvalidHome)?),
        None => None,
    };
    Ok((cwd_parts, home_parts))
}

fn request_start(
    path: &str,
    home_parts: &Option<Vec<String>>,
) -> Result<(Vec<String>, bool, usize, bool), PathError> {
    validate_raw(path)?;
    let intent = directory_intent(path);
    if path.starts_with('~') {
        if path == "~" || path.starts_with("~/") {
            let parts = home_parts.as_ref().ok_or(PathError::MissingHome)?.clone();
            let skip = if path == "~" { path.len() } else { 2 };
            return Ok((parts, true, skip, false));
        }
        return Err(PathError::UnsupportedTilde { at: 1 });
    }
    if path.starts_with('/') {
        Ok((Vec::new(), true, 0, true))
    } else {
        Ok((Vec::new(), false, 0, false))
    }
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let (cwd_parts, home_parts) = validate_contexts(cwd, home)?;
    validate_raw(path)?;
    let intent = directory_intent(path);

    let (mut stack, absolute, skip, _) = if path.starts_with('~') {
        if path == "~" || path.starts_with("~/") {
            (
                home_parts.clone().ok_or(PathError::MissingHome)?,
                true,
                if path == "~" { path.len() } else { 2 },
                false,
            )
        } else {
            return Err(PathError::UnsupportedTilde { at: 1 });
        }
    } else if path.starts_with('/') {
        (Vec::new(), true, 0, true)
    } else {
        (cwd_parts, true, 0, false)
    };

    let remainder = &path[skip..];
    let offset = skip;
    reduce(&mut stack, &components(remainder, offset), absolute)?;
    Ok(format_absolute(&stack, intent))
}

fn validate_links(links: &[Link]) -> Result<HashMap<String, usize>, PathError> {
    let mut map = HashMap::new();
    for (index, link) in links.iter().enumerate() {
        let valid_path = link.path.starts_with('/')
            && !link.path.ends_with('/')
            && link.path != "/"
            && normalize(&link.path)
                .is_ok_and(|normalized| normalized == link.path);
        if !valid_path || validate_raw(&link.target).is_err() {
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

fn absolute_key(stack: &[String]) -> String {
    if stack.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", stack.join("/"))
    }
}

fn traverse(
    stack: &mut Vec<String>,
    input: &[Component],
    absolute: bool,
    link_map: &HashMap<String, usize>,
    links: &[Link],
    active: &mut Vec<usize>,
    expansions: &mut usize,
    max_expansions: usize,
) -> Result<(), PathError> {
    for (position, item) in input.iter().enumerate() {
        match item.text.as_str() {
            "." => continue,
            ".." => {
                if stack.pop().is_none() {
                    if absolute {
                        return Err(PathError::Escape { at: item.at });
                    }
                    stack.push("..".to_string());
                }
                continue;
            }
            _ => stack.push(item.text.clone()),
        }

        let key = absolute_key(stack);
        if let Some(&index) = link_map.get(&key) {
            if active.contains(&index) {
                return Err(PathError::Loop { index });
            }
            if *expansions >= max_expansions {
                return Err(PathError::Limit { index });
            }
            *expansions += 1;
            active.push(index);

            let link = &links[index];
            let target_absolute = link.target.starts_with('/');
            if target_absolute {
                stack.clear();
            } else {
                stack.pop();
            }
            let target_components = components(&link.target, 0);
            traverse(
                stack,
                &target_components,
                true,
                link_map,
                links,
                active,
                expansions,
                max_expansions,
            )?;
            active.pop();

            if position + 1 < input.len() {
                traverse(
                    stack,
                    &input[position + 1..],
                    absolute,
                    link_map,
                    links,
                    active,
                    expansions,
                    max_expansions,
                )?;
                return Ok(());
            }
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
    let (cwd_parts, home_parts) = validate_contexts(cwd, home)?;
    let link_map = validate_links(links)?;
    validate_raw(path)?;
    let intent = directory_intent(path);

    let (mut stack, request_start, absolute_request) = if path.starts_with('~') {
        if path == "~" || path.starts_with("~/") {
            (
                home_parts.clone().ok_or(PathError::MissingHome)?,
                if path == "~" { path.len() } else { 2 },
                false,
            )
        } else {
            return Err(PathError::UnsupportedTilde { at: 1 });
        }
    } else if path.starts_with('/') {
        (Vec::new(), 0, true)
    } else {
        (cwd_parts.clone(), 0, false)
    };

    if absolute_request {
        stack.clear();
    } else {
        let ctx_items: Vec<Component> = stack
            .iter()
            .map(|s| Component {
                text: s.clone(),
                at: 0,
            })
            .collect();
        let mut active = Vec::new();
        let mut expansions = 0;
        traverse(
            &mut Vec::new(),
            &ctx_items,
            true,
            &link_map,
            links,
            &mut active,
            &mut expansions,
            max_expansions,
        )?;
        let mut resolved_ctx = Vec::new();
        traverse(
            &mut resolved_ctx,
            &ctx_items,
            true,
            &link_map,
            links,
            &mut active,
            &mut expansions,
            max_expansions,
        )?;
        stack = resolved_ctx;
    }

    let mut active = Vec::new();
    let mut expansions = 0;
    let remainder = &path[request_start..];
    traverse(
        &mut stack,
        &components(remainder, request_start),
        true,
        &link_map,
        links,
        &mut active,
        &mut expansions,
        max_expansions,
    )?;
    Ok(format_absolute(&stack, intent))
}

fn resolved_components(path: &str) -> Vec<String> {
    components(path, 0).into_iter().map(|c| c.text).collect()
}

fn route(base: &[String], target: &[String]) -> String {
    let common = base
        .iter()
        .zip(target)
        .take_while(|(a, b)| a == b)
        .count();
    let mut parts = vec!["..".to_string(); base.len() - common];
    parts.extend_from_slice(&target[common..]);
    if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
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
    let base_parts = resolved_components(&resolved_base);
    let target_parts = resolved_components(&resolved_target);
    let target_directory = resolved_target.ends_with('/');

    let mut candidates = vec![route(&base_parts, &target_parts)];
    for link in links {
        let probe = match resolve_links(&link.path, "/", None, links, max_expansions) {
            Ok(path) => path,
            Err(_) => continue,
        };
        let alias_parts = resolved_components(&probe);
        if alias_parts.len() <= target_parts.len()
            && alias_parts
                .iter()
                .zip(&target_parts)
                .all(|(a, b)| a == b)
        {
            let mut candidate = route(&base_parts, &components(&link.path, 0).into_iter().map(|c| c.text).collect::<Vec<_>>());
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
    }

    let mut valid = Vec::new();
    for mut candidate in candidates {
        if candidate.starts_with('~') {
            candidate = format!("./{candidate}");
        }
        if target_directory && candidate != "." && !candidate.ends_with('/') {
            candidate.push('/');
        }
        if let Ok(resolved) = resolve_links(
            &candidate,
            &resolved_base,
            home,
            links,
            max_expansions,
        ) {
            if resolved_components(&resolved) == target_parts {
                valid.push(candidate);
            }
        }
    }
    valid.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes())));
    valid.dedup();
    valid.into_iter().next().ok_or(PathError::Escape { at: 0 })
}

fn main() {}