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

#[derive(Debug, Clone)]
struct Component {
    value: String,
    at: usize,
}

#[derive(Debug, Clone)]
struct ParsedPath {
    absolute: bool,
    components: Vec<Component>,
    directory_intent: bool,
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

fn raw_components(path: &str, offset: usize) -> Vec<Component> {
    let bytes = path.as_bytes();
    let mut result = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        while i < bytes.len() && bytes[i] == b'/' {
            i += 1;
        }
        if i == bytes.len() {
            break;
        }

        let start = i;
        while i < bytes.len() && bytes[i] != b'/' {
            i += 1;
        }
        result.push(Component {
            value: path[start..i].to_owned(),
            at: offset + start,
        });
    }
    result
}

fn parse_path(path: &str) -> Result<ParsedPath, PathError> {
    validate_raw(path)?;
    let components = raw_components(path, 0);
    let directory_intent = path.ends_with('/')
        || components
            .last()
            .map(|c| c.value == "." || c.value == "..")
            .unwrap_or(false);

    Ok(ParsedPath {
        absolute: path.as_bytes()[0] == b'/',
        components,
        directory_intent,
    })
}

fn render_relative(components: &[String], directory_intent: bool) -> String {
    let mut result = if components.is_empty() {
        ".".to_owned()
    } else {
        components.join("/")
    };

    if directory_intent && result != "." {
        result.push('/');
    }
    result
}

fn render_absolute(components: &[String], directory_intent: bool) -> String {
    if components.is_empty() {
        return "/".to_owned();
    }

    let mut result = format!("/{}", components.join("/"));
    if directory_intent {
        result.push('/');
    }
    result
}

fn reduce_relative(components: &[Component]) -> Result<Vec<String>, PathError> {
    let mut stack: Vec<String> = Vec::new();

    for component in components {
        match component.value.as_str() {
            "." => {}
            ".." => {
                if stack.last().is_some_and(|last| last != "..") {
                    stack.pop();
                } else {
                    stack.push("..".to_owned());
                }
            }
            name => stack.push(name.to_owned()),
        }
    }

    Ok(stack)
}

fn reduce_absolute(components: &[Component]) -> Result<Vec<String>, PathError> {
    let mut stack: Vec<String> = Vec::new();

    for component in components {
        match component.value.as_str() {
            "." => {}
            ".." => {
                if stack.pop().is_none() {
                    return Err(PathError::Escape { at: component.at });
                }
            }
            name => stack.push(name.to_owned()),
        }
    }

    Ok(stack)
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    let parsed = parse_path(path)?;

    if parsed.absolute {
        let components = reduce_absolute(&parsed.components)?;
        Ok(render_absolute(&components, parsed.directory_intent))
    } else {
        let components = reduce_relative(&parsed.components)?;
        Ok(render_relative(&components, parsed.directory_intent))
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
        .map(str::to_owned)
        .collect())
}

fn validate_home(home: Option<&str>) -> Result<Option<Vec<String>>, PathError> {
    match home {
        None => Ok(None),
        Some(path) => {
            if !path.starts_with('/') {
                return Err(PathError::InvalidHome);
            }
            let normalized = normalize(path).map_err(|_| PathError::InvalidHome)?;
            Ok(Some(
                normalized
                    .split('/')
                    .filter(|part| !part.is_empty())
                    .map(str::to_owned)
                    .collect(),
            ))
        }
    }
}

fn expand_request_tilde(
    path: &str,
    components: Vec<Component>,
    home: Option<&[String]>,
) -> Result<(bool, Vec<String>, Vec<Component>), PathError> {
    if !path.starts_with('~') {
        return Ok((false, Vec::new(), components));
    }

    let bytes = path.as_bytes();
    if bytes.len() > 1 && bytes[1] != b'/' {
        return Err(PathError::UnsupportedTilde { at: 1 });
    }

    let home = home.ok_or(PathError::MissingHome)?;
    let remainder_start = if bytes.len() > 1 { 2 } else { 1 };
    let remainder = raw_components(&path[remainder_start..], remainder_start);

    Ok((true, home.to_vec(), remainder))
}

fn run_request(
    path: &str,
    cwd: &[String],
    home: Option<&[String]>,
) -> Result<String, PathError> {
    let parsed = parse_path(path)?;
    let (expanded, tilde_base, components) =
        expand_request_tilde(path, parsed.components, home)?;

    let (absolute, mut stack) = if expanded {
        (true, tilde_base)
    } else if parsed.absolute {
        (true, Vec::new())
    } else {
        (true, cwd.to_vec())
    };

    let reduced = if absolute {
        reduce_absolute(&components)?
    } else {
        reduce_relative(&components)?
    };

    if expanded || parsed.absolute {
        stack.extend(reduced);
    } else {
        stack.extend(reduced);
    }

    Ok(render_absolute(&stack, parsed.directory_intent))
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_components = validate_context(cwd)?;
    let home_components = validate_home(home)?;
    validate_raw(path)?;

    let home_slice = home_components.as_deref();
    run_request(path, &cwd_components, home_slice)
}

fn validate_links(links: &[Link]) -> Result<(), PathError> {
    let mut seen: Vec<(&str, usize)> = Vec::new();

    for (index, link) in links.iter().enumerate() {
        let path_valid = link.path.starts_with('/')
            && !link.path.ends_with('/')
            && normalize(&link.path)
                .map(|normalized| normalized == link.path)
                .unwrap_or(false);
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

fn link_key_components(link: &Link) -> Vec<String> {
    link.path
        .split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

#[derive(Clone)]
struct WorkItem {
    component: Component,
    active_end: Option<usize>,
}

fn components_equal_rendered(left: &[String], right_path: &str) -> bool {
    let right: Vec<&str> = right_path
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    left.len() == right.len()
        && left
            .iter()
            .zip(right.iter())
            .all(|(a, b)| a.as_str() == *b)
}

fn traverse_links(
    initial_stack: Vec<String>,
    items: Vec<WorkItem>,
    links: &[Link],
    max_expansions: usize,
) -> Result<Vec<String>, PathError> {
    let mut stack = initial_stack;
    let mut queue = items;
    let mut active: Vec<usize> = Vec::new();
    let mut expansion_count = 0usize;

    while let Some(item) = queue.first().cloned() {
        queue.remove(0);

        if let Some(index) = item.active_end {
            if active.last() == Some(&index) {
                active.pop();
            } else if let Some(position) = active.iter().rposition(|&x| x == index) {
                active.remove(position);
            }
            continue;
        }

        match item.component.value.as_str() {
            "." => {}
            ".." => {
                if stack.pop().is_none() {
                    return Err(PathError::Escape {
                        at: item.component.at,
                    });
                }
            }
            name => {
                stack.push(name.to_owned());

                let matched = links.iter().enumerate().find(|(_, link)| {
                    link_key_components(link) == stack
                });

                if let Some((index, link)) = matched {
                    if active.contains(&index) {
                        return Err(PathError::Loop { index });
                    }
                    if expansion_count >= max_expansions {
                        return Err(PathError::Limit { index });
                    }
                    expansion_count += 1;
                    active.push(index);

                    stack.pop();
                    let target_absolute = link.target.starts_with('/');
                    if target_absolute {
                        stack.clear();
                    }
                    let target_components = raw_components(&link.target, 0);
                    let mut replacement = Vec::with_capacity(target_components.len() + 1 + queue.len());
                    for component in target_components {
                        replacement.push(WorkItem {
                            component,
                            active_end: None,
                        });
                    }
                    replacement.push(WorkItem {
                        component: Component {
                            value: String::new(),
                            at: 0,
                        },
                        active_end: Some(index),
                    });
                    replacement.extend(queue);
                    queue = replacement;
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
    let cwd_components = validate_context(cwd)?;
    let home_components = validate_home(home)?;
    validate_links(links)?;
    let parsed = parse_path(path)?;
    let (expanded, tilde_base, components) =
        expand_request_tilde(path, parsed.components, home_components.as_deref())?;

    let initial_stack = if expanded {
        tilde_base
    } else if parsed.absolute {
        Vec::new()
    } else {
        cwd_components
    };

    let mut items = Vec::new();
    for component in components {
        items.push(WorkItem {
            component,
            active_end: None,
        });
    }

    let result = traverse_links(initial_stack, items, links, max_expansions)?;
    Ok(render_absolute(&result, parsed.directory_intent))
}

fn route(base: &[String], target: &[String]) -> String {
    let mut common = 0;
    while common < base.len() && common < target.len() && base[common] == target[common] {
        common += 1;
    }

    let mut parts = vec!["..".to_owned(); base.len() - common];
    parts.extend(target[common..].iter().cloned());

    if parts.is_empty() {
        ".".to_owned()
    } else {
        parts.join("/")
    }
}

fn absolute_components(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

fn target_has_directory_intent(path: &str) -> bool {
    path.ends_with('/')
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
    let base_components = absolute_components(&resolved_base);
    let target_components = absolute_components(&resolved_target);
    let directory_intent = target_has_directory_intent(&resolved_target);

    let mut candidates = vec![route(&base_components, &target_components)];

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
            let mut candidate = route(&base_components, &link_key_components(link));
            let suffix = &target_components[alias_components.len()..];

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

    let mut valid: Vec<String> = Vec::new();

    for mut candidate in candidates {
        if candidate.starts_with('~') {
            candidate.insert_str(0, "./");
        }

        if directory_intent && candidate != "." && !candidate.ends_with('/') {
            candidate.push('/');
        }

        if let Ok(resolved) =
            resolve_links(&candidate, &resolved_base, home, links, max_expansions)
        {
            if components_equal_rendered(&target_components, &resolved) {
                valid.push(candidate);
            }
        }
    }

    valid.sort_by(|a, b| {
        a.len()
            .cmp(&b.len())
            .then_with(|| a.as_bytes().cmp(b.as_bytes()))
    });
    valid.dedup();

    valid
        .into_iter()
        .next()
        .ok_or(PathError::Escape { at: 0 })
}