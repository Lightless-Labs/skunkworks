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
    if path.as_bytes().last() == Some(&b'/') {
        return true;
    }
    path.rsplit('/')
        .find(|component| !component.is_empty())
        .is_some_and(|component| component == "." || component == "..")
}

fn components(path: &str) -> Vec<(usize, &str)> {
    let mut result = Vec::new();
    let mut start = None;
    for (i, byte) in path.bytes().enumerate() {
        if byte == b'/' {
            if let Some(s) = start.take() {
                result.push((s, &path[s..i]));
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        result.push((s, &path[s..]));
    }
    result
}

fn render_absolute(parts: &[String], trailing: bool) -> String {
    if parts.is_empty() {
        return "/".to_string();
    }
    let mut result = String::from("/");
    result.push_str(&parts.join("/"));
    if trailing {
        result.push('/');
    }
    result
}

fn render_relative(parts: &[String], trailing: bool) -> String {
    if parts.is_empty() {
        return ".".to_string();
    }
    let mut result = parts.join("/");
    if trailing {
        result.push('/');
    }
    result
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;
    let absolute = path.starts_with('/');
    let want_dir = directory_intent(path);
    let mut stack: Vec<String> = Vec::new();

    for (at, component) in components(path) {
        match component {
            "." => {}
            ".." => {
                if stack.last().is_some_and(|part| part != "..") {
                    stack.pop();
                } else if absolute {
                    return Err(PathError::Escape { at });
                } else {
                    stack.push("..".to_string());
                }
            }
            _ => stack.push(component.to_string()),
        }
    }

    if absolute {
        Ok(render_absolute(&stack, want_dir && !stack.is_empty()))
    } else if stack.is_empty() {
        Ok(".".to_string())
    } else {
        Ok(render_relative(&stack, want_dir))
    }
}

fn context_components(path: &str) -> Vec<String> {
    components(path)
        .into_iter()
        .filter_map(|(_, component)| match component {
            "." | "" => None,
            ".." => None,
            _ => Some(component.to_string()),
        })
        .collect()
}

fn validate_contexts(cwd: &str, home: Option<&str>) -> Result<(Vec<String>, Option<Vec<String>>), PathError> {
    let normalized_cwd = if cwd.starts_with('/') {
        normalize(cwd).map_err(|_| PathError::InvalidCwd)?
    } else {
        return Err(PathError::InvalidCwd);
    };
    let cwd_parts = context_components(&normalized_cwd);

    let home_parts = if let Some(home) = home {
        if !home.starts_with('/') {
            return Err(PathError::InvalidHome);
        }
        let normalized = normalize(home).map_err(|_| PathError::InvalidHome)?;
        Some(context_components(&normalized))
    } else {
        None
    };
    Ok((cwd_parts, home_parts))
}

fn expand_request<'a>(
    path: &'a str,
    home_parts: Option<&[String]>,
) -> Result<(bool, &'a str, usize), PathError> {
    if path.starts_with('~') {
        if path == "~" || path.starts_with("~/") {
            if home_parts.is_none() {
                return Err(PathError::MissingHome);
            }
            let consumed = if path == "~" { 1 } else { 2 };
            Ok((false, &path[consumed..], consumed))
        } else {
            Err(PathError::UnsupportedTilde { at: 1 })
        }
    } else {
        Ok((path.starts_with('/'), path, 0))
    }
}

fn reduce_plain(
    stack: &mut Vec<String>,
    raw: &str,
    offset_base: usize,
    absolute: bool,
) -> Result<(), PathError> {
    for (at, component) in components(raw) {
        match component {
            "." => {}
            ".." => {
                if stack.last().is_some_and(|part| part != "..") {
                    stack.pop();
                } else if absolute {
                    return Err(PathError::Escape {
                        at: offset_base + at,
                    });
                } else {
                    stack.push("..".to_string());
                }
            }
            _ => stack.push(component.to_string()),
        }
    }
    Ok(())
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let (cwd_parts, home_parts) = validate_contexts(cwd, home)?;
    validate_raw(path)?;
    let want_dir = directory_intent(path);
    let (absolute, remainder, offset) = expand_request(path, home_parts.as_deref())?;

    let mut stack = if absolute {
        Vec::new()
    } else if path.starts_with('~') {
        home_parts.unwrap_or_default()
    } else {
        cwd_parts
    };

    reduce_plain(&mut stack, remainder, offset, true)?;
    Ok(render_absolute(&stack, want_dir && !stack.is_empty()))
}

fn canonical_link_path(path: &str) -> bool {
    path.starts_with('/')
        && !path.ends_with('/')
        && normalize(path).is_ok_and(|normalized| normalized == path)
        && path != "/"
}

fn validate_links(links: &[Link]) -> Result<HashMap<String, usize>, PathError> {
    let mut keys = HashMap::new();
    for (index, link) in links.iter().enumerate() {
        if !canonical_link_path(&link.path) || validate_raw(&link.target).is_err() {
            return Err(PathError::InvalidLink { index });
        }
        if let Some(&first) = keys.get(&link.path) {
            return Err(PathError::DuplicateLink {
                first,
                second: index,
            });
        }
        keys.insert(link.path.clone(), index);
    }
    Ok(keys)
}

#[derive(Clone, Copy)]
struct Segment<'a> {
    raw: &'a str,
    offset: usize,
}

fn parse_segments(raw: &str, offset: usize) -> Vec<Segment<'_>> {
    components(raw)
        .into_iter()
        .map(|(at, component)| Segment {
            raw: component,
            offset: offset + at,
        })
        .collect()
}

fn stack_key(stack: &[String]) -> String {
    render_absolute(stack, false)
}

fn traverse_segments(
    stack: &mut Vec<String>,
    segments: &[Segment<'_>],
    links: &[Link],
    key_map: &HashMap<String, usize>,
    active: &mut Vec<usize>,
    expansions: &mut usize,
    max_expansions: usize,
) -> Result<(), PathError> {
    let mut pos = 0;
    while pos < segments.len() {
        let segment = segments[pos];
        match segment.raw {
            "." => {}
            ".." => {
                if stack.is_empty() {
                    return Err(PathError::Escape { at: segment.offset });
                }
                stack.pop();
            }
            ordinary => {
                stack.push(ordinary.to_string());
                let key = stack_key(stack);
                if let Some(&index) = key_map.get(&key) {
                    if active.contains(&index) {
                        return Err(PathError::Loop { index });
                    }
                    if *expansions >= max_expansions {
                        return Err(PathError::Limit { index });
                    }
                    *expansions += 1;

                    let link = &links[index];
                    let target_absolute = link.target.starts_with('/');
                    let target_segments = parse_segments(&link.target, 0);
                    if target_absolute {
                        stack.clear();
                    } else {
                        stack.pop();
                    }
                    active.push(index);
                    let result = traverse_segments(
                        stack,
                        &target_segments,
                        links,
                        key_map,
                        active,
                        expansions,
                        max_expansions,
                    );
                    active.pop();
                    result?;
                }
            }
        }
        pos += 1;
    }
    Ok(())
}

fn resolve_links_inner(
    path: &str,
    cwd_parts: &[String],
    home_parts: Option<&[String]>,
    links: &[Link],
    key_map: &HashMap<String, usize>,
    max_expansions: usize,
) -> Result<String, PathError> {
    validate_raw(path)?;
    let want_dir = directory_intent(path);
    let (absolute, remainder, offset) = expand_request(path, home_parts)?;

    let mut stack = if absolute {
        Vec::new()
    } else if path.starts_with('~') {
        home_parts.unwrap_or(&[]).to_vec()
    } else {
        cwd_parts.to_vec()
    };

    let mut active = Vec::new();
    let mut expansions = 0;

    // Context components are already normalized and do not carry request offsets.
    // Links in context paths are traversed before request components.
    if !absolute && !path.starts_with('~') {
        let context = cwd_parts
            .iter()
            .map(|part| Segment { raw: part, offset: 0 })
            .collect::<Vec<_>>();
        stack.clear();
        traverse_segments(
            &mut stack,
            &context,
            links,
            key_map,
            &mut active,
            &mut expansions,
            max_expansions,
        )?;
    } else if !absolute && path.starts_with('~') {
        let context = home_parts.unwrap_or(&[])
            .iter()
            .map(|part| Segment { raw: part, offset: 0 })
            .collect::<Vec<_>>();
        stack.clear();
        traverse_segments(
            &mut stack,
            &context,
            links,
            key_map,
            &mut active,
            &mut expansions,
            max_expansions,
        )?;
    }

    let request_segments = parse_segments(remainder, offset);
    traverse_segments(
        &mut stack,
        &request_segments,
        links,
        key_map,
        &mut active,
        &mut expansions,
        max_expansions,
    )?;
    Ok(render_absolute(&stack, want_dir && !stack.is_empty()))
}

pub fn resolve_links(
    path: &str,
    cwd: &str,
    home: Option<&str>,
    links: &[Link],
    max_expansions: usize,
) -> Result<String, PathError> {
    let (cwd_parts, home_parts) = validate_contexts(cwd, home)?;
    let key_map = validate_links(links)?;
    resolve_links_inner(
        path,
        &cwd_parts,
        home_parts.as_deref(),
        links,
        &key_map,
        max_expansions,
    )
}

fn absolute_components(path: &str) -> Vec<String> {
    components(path)
        .into_iter()
        .map(|(_, component)| component.to_string())
        .collect()
}

fn route(base: &[String], target: &[String]) -> String {
    let common = base
        .iter()
        .zip(target.iter())
        .take_while(|(a, b)| a == b)
        .count();
    let mut parts = vec!["..".to_string(); base.len() - common];
    parts.extend(target[common..].iter().cloned());
    render_relative(&parts, false)
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
    let target_dir = resolved_target.ends_with('/');

    let mut candidates = vec![route(&base_parts, &target_parts)];
    for link in links {
        let probe = resolve_links(&link.path, "/", None, links, max_expansions);
        let Ok(probe) = probe else {
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

        let mut candidate = route(&base_parts, &absolute_components(&link.path));
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

    let mut valid = Vec::new();
    for mut candidate in candidates {
        if candidate.starts_with('~') {
            candidate = format!("./{candidate}");
        }
        if target_dir && candidate != "." {
            candidate.push('/');
        }
        if let Ok(resolved) = resolve_links(
            &candidate,
            &resolved_base,
            home,
            links,
            max_expansions,
        ) {
            if absolute_components(&resolved) == target_parts {
                valid.push(candidate);
            }
        }
    }

    valid.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes())));
    valid.into_iter().next().ok_or(PathError::Empty)
}

fn main() {}