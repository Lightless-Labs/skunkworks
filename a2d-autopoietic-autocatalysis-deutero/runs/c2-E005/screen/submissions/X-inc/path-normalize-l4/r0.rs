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
    path.split('/').filter(|part| !part.is_empty()).last()
        .is_some_and(|part| part == "." || part == "..")
}

fn finish_absolute(parts: &[String], intent: bool) -> String {
    if parts.is_empty() {
        return "/".to_string();
    }
    let mut result = String::from("/");
    result.push_str(&parts.join("/"));
    if intent {
        result.push('/');
    }
    result
}

fn finish_normalized(parts: &[String], absolute: bool, intent: bool) -> String {
    if absolute {
        return finish_absolute(parts, intent);
    }
    let mut result = if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
    };
    if intent && result != "." {
        result.push('/');
    }
    result
}

fn reduce_component(
    stack: &mut Vec<String>,
    component: &str,
    absolute: bool,
    at: usize,
) -> Result<(), PathError> {
    if component.is_empty() || component == "." {
        return Ok(());
    }
    if component == ".." {
        if stack.last().is_some_and(|last| last != "..") {
            stack.pop();
        } else if absolute {
            return Err(PathError::Escape { at });
        } else {
            stack.push("..".to_string());
        }
    } else {
        stack.push(component.to_string());
    }
    Ok(())
}

fn reduce_raw(
    stack: &mut Vec<String>,
    raw: &str,
    absolute: bool,
    base_offset: usize,
) -> Result<(), PathError> {
    for (at, component) in component_offsets(raw, base_offset) {
        reduce_component(stack, component, absolute, at)?;
    }
    Ok(())
}

fn component_offsets(raw: &str, base_offset: usize) -> Vec<(usize, &str)> {
    let bytes = raw.as_bytes();
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
        result.push((base_offset + start, &raw[start..i]));
    }
    result
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;
    let absolute = path.starts_with('/');
    let intent = directory_intent(path);
    let mut stack = Vec::new();
    reduce_raw(&mut stack, path, absolute, 0)?;
    Ok(finish_normalized(&stack, absolute, intent))
}

fn normalized_absolute_context(path: &str, invalid: PathError) -> Result<Vec<String>, PathError> {
    if !path.starts_with('/') {
        return Err(invalid);
    }
    let normalized = normalize(path).map_err(|_| invalid.clone())?;
    Ok(components_of_absolute(&normalized))
}

fn components_of_absolute(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn expand_tilde<'a>(
    path: &'a str,
    home: Option<&'a str>,
) -> Result<(Option<&'a str>, usize), PathError> {
    if !path.starts_with('~') {
        return Ok((None, 0));
    }
    let bytes = path.as_bytes();
    if bytes.len() == 1 || bytes[1] == b'/' {
        if home.is_none() {
            return Err(PathError::MissingHome);
        }
        let offset = if bytes.len() > 1 { 2 } else { 1 };
        return Ok((Some(&path[offset..]), offset));
    }
    Err(PathError::UnsupportedTilde { at: 1 })
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_parts = normalized_absolute_context(cwd, PathError::InvalidCwd)?;
    let home_parts = match home {
        Some(h) => Some(normalized_absolute_context(h, PathError::InvalidHome)?),
        None => None,
    };

    validate_raw(path)?;
    let intent = directory_intent(path);
    let (expanded, offset) = expand_tilde(path, home)?;
    let is_tilde = path.starts_with('~');
    let mut stack;
    let raw;
    let raw_offset;
    let absolute;

    if is_tilde {
        stack = home_parts.unwrap();
        raw = expanded.unwrap_or("");
        raw_offset = offset;
        absolute = true;
    } else if path.starts_with('/') {
        stack = Vec::new();
        raw = path;
        raw_offset = 0;
        absolute = true;
    } else {
        stack = cwd_parts;
        raw = path;
        raw_offset = 0;
        absolute = true;
    }

    reduce_raw(&mut stack, raw, absolute, raw_offset)?;
    Ok(finish_absolute(&stack, intent))
}

fn validate_links(links: &[Link]) -> Result<HashMap<String, usize>, PathError> {
    let mut map = HashMap::new();
    for (index, link) in links.iter().enumerate() {
        let valid_path = link.path.starts_with('/')
            && !link.path.ends_with('/')
            && normalize(&link.path).is_ok_and(|n| n == link.path);
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

#[derive(Clone, Copy)]
enum Source<'a> {
    Request(&'a str, usize),
    Target(&'a str),
    Context(&'a [String]),
}

struct Traversal<'a> {
    links: &'a [Link],
    link_map: &'a HashMap<String, usize>,
    max_expansions: usize,
    expansions: usize,
    active: Vec<usize>,
    stack: Vec<String>,
}

impl<'a> Traversal<'a> {
    fn absolute_key(&self) -> String {
        if self.stack.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", self.stack.join("/"))
        }
    }

    fn push_component(&mut self, component: &str, absolute: bool, at: usize) -> Result<(), PathError> {
        if component.is_empty() || component == "." {
            return Ok(());
        }
        if component == ".." {
            if self.stack.last().is_some_and(|last| last != "..") {
                self.stack.pop();
            } else if absolute {
                return Err(PathError::Escape { at });
            } else {
                self.stack.push("..".to_string());
            }
            return Ok(());
        }

        self.stack.push(component.to_string());
        let key = self.absolute_key();
        if let Some(&index) = self.link_map.get(&key) {
            if self.active.contains(&index) {
                return Err(PathError::Loop { index });
            }
            if self.expansions >= self.max_expansions {
                return Err(PathError::Limit { index });
            }
            self.expansions += 1;
            self.active.push(index);

            let link = &self.links[index];
            self.stack.pop();
            let target_absolute = link.target.starts_with('/');
            if target_absolute {
                self.stack.clear();
            }
            self.process_raw(&link.target, Source::Target(&link.target), true)?;
            self.active.pop();
        }
        Ok(())
    }

    fn process_raw(
        &mut self,
        raw: &str,
        source: Source<'_>,
        absolute: bool,
    ) -> Result<(), PathError> {
        let base = match source {
            Source::Request(_, base) => base,
            Source::Target(_) | Source::Context(_) => 0,
        };
        for (at, component) in component_offsets(raw, base) {
            self.push_component(component, absolute, at)?;
        }
        Ok(())
    }

    fn process_context(&mut self, parts: &[String]) -> Result<(), PathError> {
        for component in parts {
            self.push_component(component, true, 0)?;
        }
        Ok(())
    }
}

fn resolve_links_internal(
    path: &str,
    cwd: &str,
    home: Option<&str>,
    links: &[Link],
    max_expansions: usize,
) -> Result<(String, bool), PathError> {
    let cwd_parts = normalized_absolute_context(cwd, PathError::InvalidCwd)?;
    let home_parts = match home {
        Some(h) => Some(normalized_absolute_context(h, PathError::InvalidHome)?),
        None => None,
    };
    let link_map = validate_links(links)?;
    validate_raw(path)?;

    let intent = directory_intent(path);
    let (expanded, offset) = expand_tilde(path, home)?;
    let tilde = path.starts_with('~');
    let absolute_request = !tilde && path.starts_with('/');

    let mut traversal = Traversal {
        links,
        link_map: &link_map,
        max_expansions,
        expansions: 0,
        active: Vec::new(),
        stack: Vec::new(),
    };

    let raw;
    let raw_offset;
    if absolute_request {
        raw = path;
        raw_offset = 0;
    } else if tilde {
        traversal.process_context(home_parts.as_ref().unwrap())?;
        raw = expanded.unwrap_or("");
        raw_offset = offset;
    } else {
        traversal.process_context(&cwd_parts)?;
        raw = path;
        raw_offset = 0;
    }

    traversal.process_raw(raw, Source::Request(raw, raw_offset), true)?;
    Ok((finish_absolute(&traversal.stack, intent), intent))
}

pub fn resolve_links(
    path: &str,
    cwd: &str,
    home: Option<&str>,
    links: &[Link],
    max_expansions: usize,
) -> Result<String, PathError> {
    resolve_links_internal(path, cwd, home, links, max_expansions).map(|r| r.0)
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

fn candidate_with_suffix(mut first: String, suffix: &[String]) -> String {
    if suffix.is_empty() {
        return first;
    }
    if first == "." {
        return suffix.join("/");
    }
    if !first.is_empty() {
        first.push('/');
    }
    first.push_str(&suffix.join("/"));
    first
}

fn protect_leading_tilde(candidate: String) -> String {
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
    let base_parts = components_of_absolute(&resolved_base);
    let target_parts = components_of_absolute(&resolved_target);
    let target_directory = resolved_target.ends_with('/');

    let mut candidates = vec![route(&base_parts, &target_parts)];
    for link in links {
        if let Ok(probe) = resolve_links(&link.path, "/", None, links, max_expansions) {
            let probe_parts = components_of_absolute(&probe);
            if probe_parts.len() <= target_parts.len()
                && probe_parts.iter().zip(target_parts.iter()).all(|(a, b)| a == b)
            {
                let key_parts = components_of_absolute(&link.path);
                let candidate = candidate_with_suffix(
                    route(&base_parts, &key_parts),
                    &target_parts[probe_parts.len()..],
                );
                candidates.push(candidate);
            }
        }
    }

    let mut best: Option<String> = None;
    for candidate in candidates {
        let mut candidate = protect_leading_tilde(candidate);
        if target_directory && candidate != "." && !candidate.ends_with('/') {
            candidate.push('/');
        }
        let Ok(validated) = resolve_links(
            &candidate,
            &resolved_base,
            home,
            links,
            max_expansions,
        ) else {
            continue;
        };
        if components_of_absolute(&validated) != target_parts {
            continue;
        }
        let replace = match &best {
            None => true,
            Some(current) => {
                candidate.len() < current.len()
                    || (candidate.len() == current.len() && candidate.as_bytes() < current.as_bytes())
            }
        };
        if replace {
            best = Some(candidate);
        }
    }
    best.ok_or(PathError::Escape { at: 0 })
}