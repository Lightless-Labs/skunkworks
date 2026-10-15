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

fn is_directory_intent(path: &str) -> bool {
    if path.ends_with('/') {
        return true;
    }
    path.rsplit('/').find(|part| !part.is_empty()).is_some_and(|part| part == "." || part == "..")
}

fn split_components(path: &str, offset: usize) -> Vec<(String, usize)> {
    let mut result = Vec::new();
    let mut start = 0;
    for (i, byte) in path.bytes().enumerate() {
        if byte == b'/' {
            if start < i {
                result.push((path[start..i].to_string(), offset + start));
            }
            start = i + 1;
        }
    }
    if start < path.len() {
        result.push((path[start..].to_string(), offset + start));
    }
    result
}

fn render_absolute(parts: &[String], trailing: bool) -> String {
    if parts.is_empty() {
        return "/".to_string();
    }
    let mut out = format!("/{}", parts.join("/"));
    if trailing {
        out.push('/');
    }
    out
}

fn normalize_parts(path: &str) -> Result<Vec<String>, PathError> {
    validate_raw(path)?;
    let absolute = path.starts_with('/');
    let mut parts: Vec<String> = Vec::new();
    for (component, at) in split_components(path, 0) {
        match component.as_str() {
            "." => {}
            ".." => {
                if parts.last().is_some_and(|p| p != "..") {
                    parts.pop();
                } else if absolute {
                    return Err(PathError::Escape { at });
                } else {
                    parts.push(component);
                }
            }
            _ => parts.push(component),
        }
    }
    Ok(parts)
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    let parts = normalize_parts(path)?;
    let absolute = path.starts_with('/');
    let intent = is_directory_intent(path);
    if absolute {
        if parts.is_empty() {
            Ok("/".to_string())
        } else {
            Ok(render_absolute(&parts, intent))
        }
    } else if parts.is_empty() {
        Ok(".".to_string())
    } else {
        let mut out = parts.join("/");
        if intent {
            out.push('/');
        }
        Ok(out)
    }
}

fn validated_context(path: &str, invalid: PathError) -> Result<Vec<String>, PathError> {
    if !path.starts_with('/') {
        return Err(invalid);
    }
    normalize_parts(path).map_err(|_| invalid)
}

fn request_start<'a>(
    path: &'a str,
    home_parts: Option<&'a [String]>,
) -> Result<(bool, &'a str, usize, Option<&'a [String]>), PathError> {
    if path.starts_with('~') {
        if path.len() == 1 || path.as_bytes().get(1) == Some(&b'/') {
            let home = home_parts.ok_or(PathError::MissingHome)?;
            let skip = if path.len() > 1 { 2 } else { 1 };
            Ok((true, &path[skip..], skip, Some(home)))
        } else {
            Err(PathError::UnsupportedTilde { at: 1 })
        }
    } else if path.starts_with('/') {
        Ok((true, path, 0, None))
    } else {
        Ok((false, path, 0, None))
    }
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_parts = validated_context(cwd, PathError::InvalidCwd)?;
    let home_parts = match home {
        Some(h) => Some(validated_context(h, PathError::InvalidHome)?),
        None => None,
    };

    validate_raw(path)?;
    let (absolute, remainder, offset, tilde_home) =
        request_start(path, home_parts.as_deref())?;

    let mut parts = if let Some(home) = tilde_home {
        home.to_vec()
    } else if absolute {
        Vec::new()
    } else {
        cwd_parts
    };

    for (component, at) in split_components(remainder, offset) {
        match component.as_str() {
            "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(PathError::Escape { at });
                }
            }
            _ => parts.push(component),
        }
    }
    Ok(render_absolute(&parts, is_directory_intent(path)))
}

fn validate_links(links: &[Link]) -> Result<HashMap<String, usize>, PathError> {
    let mut keys = HashMap::new();
    for (index, link) in links.iter().enumerate() {
        let valid_path = link.path.starts_with('/')
            && !link.path.ends_with('/')
            && link.path != "/"
            && normalize(&link.path).is_ok_and(|n| n == link.path);
        if !valid_path || validate_raw(&link.target).is_err() {
            return Err(PathError::InvalidLink { index });
        }
        if let Some(&first) = keys.get(&link.path) {
            return Err(PathError::DuplicateLink { first, second: index });
        }
        keys.insert(link.path.clone(), index);
    }
    Ok(keys)
}

#[derive(Clone)]
struct Component {
    name: String,
    at: usize,
}

fn stack_key(parts: &[String]) -> String {
    render_absolute(parts, false)
}

fn traverse_items(
    mut stack: Vec<String>,
    items: Vec<Component>,
    links: &[Link],
    keys: &HashMap<String, usize>,
    active: &mut Vec<usize>,
    count: &mut usize,
    max_expansions: usize,
) -> Result<Vec<String>, PathError> {
    let mut todo = items;
    let mut cursor = 0;
    while cursor < todo.len() {
        let item = todo[cursor].clone();
        cursor += 1;
        match item.name.as_str() {
            "." => continue,
            ".." => {
                if stack.pop().is_none() {
                    return Err(PathError::Escape { at: item.at });
                }
            }
            _ => {
                stack.push(item.name);
                let key = stack_key(&stack);
                if let Some(&index) = keys.get(&key) {
                    if active.contains(&index) {
                        return Err(PathError::Loop { index });
                    }
                    if *count >= max_expansions {
                        return Err(PathError::Limit { index });
                    }
                    *count += 1;
                    active.push(index);

                    let link = &links[index];
                    let target_absolute = link.target.starts_with('/');
                    let target_parts = split_components(&link.target, 0)
                        .into_iter()
                        .map(|(name, at)| Component { name, at })
                        .collect();
                    let target_start = if target_absolute {
                        Vec::new()
                    } else {
                        let mut parent = stack.clone();
                        parent.pop();
                        parent
                    };
                    let expanded = traverse_items(
                        target_start,
                        target_parts,
                        links,
                        keys,
                        active,
                        count,
                        max_expansions,
                    )?;
                    active.pop();
                    stack = expanded;
                }
            }
        }
    }
    Ok(stack)
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
        Some(h) => Some(validated_context(h, PathError::InvalidHome)?),
        None => None,
    };
    let keys = validate_links(links)?;
    validate_raw(path)?;

    let (absolute, remainder, offset, tilde_home) =
        request_start(path, home_parts.as_deref())?;
    let starting = if let Some(home) = tilde_home {
        home.to_vec()
    } else if absolute {
        Vec::new()
    } else {
        cwd_parts
    };

    let items = split_components(remainder, offset)
        .into_iter()
        .map(|(name, at)| Component { name, at })
        .collect();
    let mut active = Vec::new();
    let mut count = 0;
    let parts = traverse_items(
        starting,
        items,
        links,
        &keys,
        &mut active,
        &mut count,
        max_expansions,
    )?;
    Ok(render_absolute(&parts, is_directory_intent(path)))
}

fn components_of_absolute(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn route(base: &[String], target: &[String]) -> String {
    let common = base
        .iter()
        .zip(target.iter())
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
    let b = components_of_absolute(&resolved_base);
    let t = components_of_absolute(&resolved_target);
    let target_directory = resolved_target.ends_with('/');

    let mut candidates = vec![route(&b, &t)];
    for link in links {
        if let Ok(probe) = resolve_links(&link.path, "/", None, links, max_expansions) {
            let a = components_of_absolute(&probe);
            if a.len() <= t.len() && a.iter().zip(&t).all(|(x, y)| x == y) {
                let suffix = &t[a.len()..];
                candidates.push(candidate_with_suffix(
                    route(&b, &components_of_absolute(&link.path)),
                    suffix,
                ));
            }
        }
    }

    let mut valid: Vec<String> = Vec::new();
    for mut candidate in candidates {
        if candidate.starts_with('~') {
            candidate = format!("./{}", candidate);
        }
        if target_directory && candidate != "." {
            candidate.push('/');
        }
        if let Ok(result) = resolve_links(
            &candidate,
            &resolved_base,
            home,
            links,
            max_expansions,
        ) {
            if components_of_absolute(&result) == t {
                valid.push(candidate);
            }
        }
    }

    valid.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes())));
    valid.into_iter().next().ok_or(PathError::Escape { at: 0 })
}

fn main() {}