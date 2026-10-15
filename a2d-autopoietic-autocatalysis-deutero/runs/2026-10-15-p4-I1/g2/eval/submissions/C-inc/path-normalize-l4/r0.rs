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

fn raw_components(path: &str, base_offset: usize) -> Vec<(&str, usize)> {
    let mut result = Vec::new();
    let bytes = path.as_bytes();
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
        result.push((&path[start..i], base_offset + start));
    }
    result
}

fn directory_intent(path: &str) -> bool {
    path.ends_with('/')
        || raw_components(path, 0)
            .last()
            .is_some_and(|(part, _)| *part == "." || *part == "..")
}

fn normalize_components(
    components: impl IntoIterator<Item = (String, usize)>,
    absolute: bool,
) -> Result<Vec<String>, PathError> {
    let mut stack: Vec<String> = Vec::new();
    for (component, at) in components {
        match component.as_str() {
            "." => {}
            ".." => {
                if stack.last().is_some_and(|last| last != "..") {
                    stack.pop();
                } else if absolute {
                    return Err(PathError::Escape { at });
                } else {
                    stack.push(component);
                }
            }
            _ => stack.push(component),
        }
    }
    Ok(stack)
}

fn render(components: &[String], absolute: bool, intent: bool) -> String {
    let mut result = components.join("/");
    if absolute {
        result.insert(0, '/');
        if components.is_empty() {
            return result;
        }
    } else if components.is_empty() {
        return ".".to_owned();
    }

    if intent && !result.ends_with('/') {
        result.push('/');
    }
    result
}

fn normalized(path: &str) -> Result<(Vec<String>, bool, bool), PathError> {
    validate_raw(path)?;
    let absolute = path.as_bytes()[0] == b'/';
    let intent = directory_intent(path);
    let components = normalize_components(
        raw_components(path, 0)
            .into_iter()
            .map(|(part, at)| (part.to_owned(), at)),
        absolute,
    )?;
    Ok((components, absolute, intent))
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    let (components, absolute, intent) = normalized(path)?;
    Ok(render(&components, absolute, intent))
}

fn validate_context(path: &str, error: PathError) -> Result<Vec<String>, PathError> {
    if !path.starts_with('/') {
        return Err(error);
    }
    match normalized(path) {
        Ok((components, true, _)) => Ok(components),
        _ => Err(error),
    }
}

fn expand_tilde<'a>(path: &'a str, home: Option<&'a [String]>) -> Result<(bool, usize), PathError> {
    if !path.starts_with('~') {
        return Ok((false, 0));
    }
    if path == "~" || path.starts_with("~/") {
        if home.is_none() {
            return Err(PathError::MissingHome);
        }
        let consumed = if path == "~" { 1 } else { 2 };
        return Ok((true, consumed));
    }
    Err(PathError::UnsupportedTilde { at: 1 })
}

fn prepare_request<'a>(
    path: &'a str,
    home: Option<&'a [String]>,
) -> Result<(bool, usize, bool), PathError> {
    validate_raw(path)?;
    let (expanded, consumed) = expand_tilde(path, home)?;
    Ok((expanded, consumed, directory_intent(path)))
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_components = validate_context(cwd, PathError::InvalidCwd)?;
    let home_components = match home {
        Some(value) => Some(validate_context(value, PathError::InvalidHome)?),
        None => None,
    };
    let (expanded, consumed, intent) = prepare_request(path, home_components.as_deref())?;

    let absolute = !expanded && path.starts_with('/');
    let mut stack = if absolute {
        Vec::new()
    } else if expanded {
        home_components.unwrap_or_default()
    } else {
        cwd_components
    };

    let remainder = &path[consumed..];
    for (part, at) in raw_components(remainder, consumed) {
        match part {
            "." => {}
            ".." => {
                if stack.pop().is_none() {
                    return Err(PathError::Escape { at });
                }
            }
            _ => stack.push(part.to_owned()),
        }
    }
    Ok(render(&stack, true, intent))
}

fn validate_links(links: &[Link]) -> Result<HashMap<String, usize>, PathError> {
    let mut by_path = HashMap::new();
    for (index, link) in links.iter().enumerate() {
        let valid_path = link.path.starts_with('/')
            && !link.path.ends_with('/')
            && normalize(&link.path).is_ok_and(|value| value == link.path)
            && link.path != "/";
        if !valid_path || validate_raw(&link.target).is_err() {
            return Err(PathError::InvalidLink { index });
        }
        if let Some(&first) = by_path.get(&link.path) {
            return Err(PathError::DuplicateLink {
                first,
                second: index,
            });
        }
        by_path.insert(link.path.clone(), index);
    }
    Ok(by_path)
}

struct ExpansionState<'a> {
    links: &'a [Link],
    by_path: &'a HashMap<String, usize>,
    active: Vec<usize>,
    count: usize,
    limit: usize,
}

impl ExpansionState<'_> {
    fn process(
        &mut self,
        stack: &mut Vec<String>,
        items: &[(String, usize)],
        absolute: bool,
    ) -> Result<(), PathError> {
        for (component, at) in items {
            match component.as_str() {
                "." => continue,
                ".." => {
                    if stack.last().is_some_and(|last| last != "..") {
                        stack.pop();
                    } else if absolute {
                        return Err(PathError::Escape { at: *at });
                    } else {
                        stack.push(component.clone());
                    }
                    continue;
                }
                _ => stack.push(component.clone()),
            }

            if !absolute {
                continue;
            }
            let key = format!("/{}", stack.join("/"));
            let Some(&index) = self.by_path.get(&key) else {
                continue;
            };
            if self.active.contains(&index) {
                return Err(PathError::Loop { index });
            }
            if self.count >= self.limit {
                return Err(PathError::Limit { index });
            }
            self.count += 1;
            self.active.push(index);

            // A relative target starts at the link's parent, so remove the
            // link component that was just pushed.
            stack.pop();
            let target = &self.links[index].target;
            if target.starts_with('/') {
                stack.clear();
            }
            let target_items: Vec<(String, usize)> = raw_components(target, 0)
                .into_iter()
                .map(|(part, offset)| (part.to_owned(), offset))
                .collect();
            let target_absolute = target.starts_with('/');
            let result = self.process(stack, &target_items, target_absolute);
            self.active.pop();
            result?;
        }
        Ok(())
    }
}

pub fn resolve_links(
    path: &str,
    cwd: &str,
    home: Option<&str>,
    links: &[Link],
    max_expansions: usize,
) -> Result<String, PathError> {
    let cwd_components = validate_context(cwd, PathError::InvalidCwd)?;
    let home_components = match home {
        Some(value) => Some(validate_context(value, PathError::InvalidHome)?),
        None => None,
    };
    let by_path = validate_links(links)?;
    let (expanded, consumed, intent) = prepare_request(path, home_components.as_deref())?;

    let absolute = !expanded && path.starts_with('/');
    let mut stack = if absolute {
        Vec::new()
    } else if expanded {
        home_components.unwrap_or_default()
    } else {
        cwd_components
    };

    let request_items: Vec<(String, usize)> = raw_components(&path[consumed..], consumed)
        .into_iter()
        .map(|(part, offset)| (part.to_owned(), offset))
        .collect();

    let mut state = ExpansionState {
        links,
        by_path: &by_path,
        active: Vec::new(),
        count: 0,
        limit: max_expansions,
    };
    state.process(&mut stack, &request_items, true)?;
    Ok(render(&stack, true, intent))
}

fn components_from_absolute(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

fn route(base: &[String], target: &[String]) -> String {
    let common = base
        .iter()
        .zip(target.iter())
        .take_while(|(left, right)| left == right)
        .count();
    let mut parts = Vec::new();
    parts.extend((common..base.len()).map(|_| "..".to_owned()));
    parts.extend(target[common..].iter().cloned());
    if parts.is_empty() {
        ".".to_owned()
    } else {
        parts.join("/")
    }
}

fn candidate_with_suffix(mut prefix: String, suffix: &[String]) -> String {
    if suffix.is_empty() {
        return prefix;
    }
    if prefix == "." {
        return suffix.join("/");
    }
    if !prefix.is_empty() {
        prefix.push('/');
    }
    prefix.push_str(&suffix.join("/"));
    prefix
}

fn candidate_spelling(mut candidate: String, target_has_slash: bool) -> String {
    if candidate.starts_with('~') {
        candidate.insert_str(0, "./");
    }
    if candidate != "." && target_has_slash && !candidate.ends_with('/') {
        candidate.push('/');
    }
    candidate
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
    let base_components = components_from_absolute(&resolved_base);
    let target_components = components_from_absolute(&resolved_target);
    let target_has_slash = resolved_target.ends_with('/');

    let mut candidates = vec![route(&base_components, &target_components)];

    for link in links {
        let probe = resolve_links(&link.path, "/", None, links, max_expansions);
        let Ok(probed_path) = probe else {
            continue;
        };
        let anchor = components_from_absolute(&probed_path);
        if anchor.len() > target_components.len()
            || anchor
                .iter()
                .zip(target_components.iter())
                .any(|(a, t)| a != t)
        {
            continue;
        }
        let key_components = components_from_absolute(&link.path);
        let suffix = &target_components[anchor.len()..];
        candidates.push(candidate_with_suffix(route(&base_components, &key_components), suffix));
    }

    let mut valid: Vec<String> = Vec::new();
    for candidate in candidates {
        let candidate = candidate_spelling(candidate, target_has_slash);
        let Ok(resolved) = resolve_links(
            &candidate,
            &resolved_base,
            home,
            links,
            max_expansions,
        ) else {
            continue;
        };
        if components_from_absolute(&resolved) == target_components {
            valid.push(candidate);
        }
    }

    valid.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes())));
    valid.into_iter().next().ok_or(PathError::Escape { at: 0 })
}

fn main() {}