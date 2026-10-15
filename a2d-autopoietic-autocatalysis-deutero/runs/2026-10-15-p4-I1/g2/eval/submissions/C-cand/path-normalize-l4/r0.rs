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

fn components(path: &str) -> Vec<Component> {
    path.split('/')
        .scan(0usize, |offset, part| {
            let at = *offset;
            *offset += part.len() + 1;
            Some((part, at))
        })
        .filter(|(part, _)| !part.is_empty())
        .map(|(part, at)| Component {
            text: part.to_owned(),
            at,
        })
        .collect()
}

fn directory_intent(path: &str) -> bool {
    path.ends_with('/')
        || components(path)
            .last()
            .is_some_and(|c| c.text == "." || c.text == "..")
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

fn reduce_plain(path: &str, absolute: bool) -> Result<Vec<String>, PathError> {
    let mut result: Vec<String> = Vec::new();
    for component in components(path) {
        match component.text.as_str() {
            "." => {}
            ".." => {
                if result.last().is_some_and(|last| last != "..") {
                    result.pop();
                } else if absolute {
                    return Err(PathError::Escape { at: component.at });
                } else {
                    result.push("..".to_owned());
                }
            }
            _ => result.push(component.text),
        }
    }
    Ok(result)
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;
    let absolute = path.starts_with('/');
    let parts = reduce_plain(path, absolute)?;
    Ok(render(&parts, absolute, directory_intent(path)))
}

fn validate_context(path: &str) -> Result<Vec<String>, PathError> {
    if !path.starts_with('/') {
        return Err(PathError::InvalidCwd);
    }
    normalize(path)
        .map_err(|_| PathError::InvalidCwd)
        .map(|normalized| {
            normalized
                .split('/')
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect()
        })
}

fn validate_home(home: Option<&str>) -> Result<Option<Vec<String>>, PathError> {
    match home {
        None => Ok(None),
        Some(path) => {
            if !path.starts_with('/') {
                return Err(PathError::InvalidHome);
            }
            normalize(path)
                .map_err(|_| PathError::InvalidHome)
                .map(|normalized| {
                    Some(
                        normalized
                            .split('/')
                            .filter(|s| !s.is_empty())
                            .map(str::to_owned)
                            .collect(),
                    )
                })
        }
    }
}

fn tilde_start(path: &str, home: Option<&[String]>) -> Result<(bool, usize, Vec<String>), PathError> {
    if !path.starts_with('~') {
        return Ok((path.starts_with('/'), 0, Vec::new()));
    }
    let bytes = path.as_bytes();
    if bytes.len() > 1 && bytes[1] != b'/' {
        return Err(PathError::UnsupportedTilde { at: 1 });
    }
    let home = home.ok_or(PathError::MissingHome)?;
    let consumed = if bytes.len() > 1 { 2 } else { 1 };
    Ok((true, consumed, home.to_vec()))
}

fn resolve_core(
    path: &str,
    cwd_parts: &[String],
    home_parts: Option<&[String]>,
) -> Result<String, PathError> {
    validate_raw(path)?;
    let (absolute, consumed, start) = tilde_start(path, home_parts)?;
    let mut stack = if absolute {
        start
    } else {
        cwd_parts.to_vec()
    };

    for component in components(&path[consumed..]) {
        let at = component.at + consumed;
        match component.text.as_str() {
            "." => {}
            ".." => {
                if stack.pop().is_none() {
                    return Err(PathError::Escape { at });
                }
            }
            _ => stack.push(component.text),
        }
    }

    Ok(render(&stack, true, directory_intent(path)))
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_parts = validate_context(cwd)?;
    let home_parts = validate_home(home)?;
    resolve_core(path, &cwd_parts, home_parts.as_deref())
}

fn validate_links(links: &[Link]) -> Result<(), PathError> {
    let mut seen: Vec<(&str, usize)> = Vec::new();
    for (index, link) in links.iter().enumerate() {
        let valid_path = link.path.starts_with('/')
            && !link.path.ends_with('/')
            && link.path != "/"
            && normalize(&link.path)
                .is_ok_and(|normalized| normalized == link.path);
        let valid_target = validate_raw(&link.target).is_ok();
        if !valid_path || !valid_target {
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

fn path_key(stack: &[String]) -> String {
    format!("/{}", stack.join("/"))
}

fn link_index(links: &[Link], stack: &[String]) -> Option<usize> {
    let key = path_key(stack);
    links.iter().position(|link| link.path == key)
}

#[derive(Clone)]
struct WorkItem {
    component: Component,
    source: String,
}

fn traversal_components(path: &str, base: usize) -> Vec<WorkItem> {
    components(path)
        .into_iter()
        .map(|component| WorkItem {
            component: Component {
                text: component.text,
                at: component.at + base,
            },
            source: path.to_owned(),
        })
        .collect()
}

fn resolve_links_traverse(
    mut stack: Vec<String>,
    initial: Vec<WorkItem>,
    request_path: &str,
    links: &[Link],
    max_expansions: usize,
) -> Result<Vec<String>, PathError> {
    let mut queue = initial;
    let mut active: Vec<usize> = Vec::new();
    let mut frames: Vec<usize> = Vec::new();
    let mut expansions = 0usize;
    let mut cursor = 0usize;

    while cursor < queue.len() || !frames.is_empty() {
        if cursor >= queue.len() {
            if let Some(index) = frames.pop() {
                active.pop();
                continue;
            }
            break;
        }

        let item = queue[cursor].clone();
        cursor += 1;

        match item.component.text.as_str() {
            "." => continue,
            ".." => {
                if stack.pop().is_none() {
                    return Err(PathError::Escape {
                        at: item.component.at,
                    });
                }
            }
            _ => {
                stack.push(item.component.text);
                if let Some(index) = link_index(links, &stack) {
                    if active.contains(&index) {
                        return Err(PathError::Loop { index });
                    }
                    if expansions >= max_expansions {
                        return Err(PathError::Limit { index });
                    }
                    expansions += 1;
                    active.push(index);
                    frames.push(index);

                    let link = &links[index];
                    stack.pop();
                    let target_absolute = link.target.starts_with('/');
                    if target_absolute {
                        stack.clear();
                    }

                    let target_items = traversal_components(&link.target, 0);
                    let mut suffix = queue[cursor..].to_vec();
                    let mut combined = target_items;
                    combined.append(&mut suffix);
                    queue.truncate(cursor);
                    queue.append(&mut combined);
                    cursor -= 0;

                    if target_absolute {
                        // The absolute target already reset traversal to root.
                    }
                }
            }
        }
    }

    let _ = request_path;
    Ok(stack)
}

pub fn resolve_links(
    path: &str,
    cwd: &str,
    home: Option<&str>,
    links: &[Link],
    max_expansions: usize,
) -> Result<String, PathError> {
    let cwd_parts = validate_context(cwd)?;
    let home_parts = validate_home(home)?;
    validate_links(links)?;
    validate_raw(path)?;

    let (absolute, consumed, tilde_start_parts) = tilde_start(path, home_parts.as_deref())?;
    let start = if absolute {
        tilde_start_parts
    } else {
        cwd_parts
    };
    let mut initial = traversal_components(&path[consumed..], consumed);
    for item in &mut initial {
        item.component.at = item.component.at.max(consumed);
    }

    let result = resolve_links_traverse(
        start,
        initial,
        path,
        links,
        max_expansions,
    )?;
    Ok(render(&result, true, directory_intent(path)))
}

fn abs_parts(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

fn route(base: &[String], target: &[String]) -> String {
    let common = base
        .iter()
        .zip(target.iter())
        .take_while(|(a, b)| a == b)
        .count();
    let mut parts = vec!["..".to_owned(); base.len() - common];
    parts.extend(target[common..].iter().cloned());
    if parts.is_empty() {
        ".".to_owned()
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
    let base_parts = abs_parts(&resolved_base);
    let target_parts = abs_parts(&resolved_target);
    let target_directory = resolved_target.ends_with('/');

    let mut candidates = vec![route(&base_parts, &target_parts)];
    for link in links {
        let probe = resolve_links(
            &link.path,
            "/",
            None,
            links,
            max_expansions,
        );
        let Ok(probe) = probe else {
            continue;
        };
        let matched = abs_parts(&probe);
        if matched.len() <= target_parts.len()
            && matched
                .iter()
                .zip(target_parts.iter())
                .all(|(a, b)| a == b)
        {
            let key = abs_parts(&link.path);
            let mut candidate = route(&base_parts, &key);
            let suffix = &target_parts[matched.len()..];
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
            candidate.insert_str(0, "./");
        }
        if target_directory && candidate != "." && !candidate.ends_with('/') {
            candidate.push('/');
        }
        let resolved = resolve_links(
            &candidate,
            &resolved_base,
            home,
            links,
            max_expansions,
        );
        if let Ok(resolved) = resolved {
            if abs_parts(&resolved) == target_parts {
                valid.push(candidate);
            }
        }
    }

    valid.sort_by(|a, b| {
        a.len()
            .cmp(&b.len())
            .then_with(|| a.as_bytes().cmp(b.as_bytes()))
    });
    valid.into_iter().next().ok_or(PathError::Empty)
}

fn main() {}