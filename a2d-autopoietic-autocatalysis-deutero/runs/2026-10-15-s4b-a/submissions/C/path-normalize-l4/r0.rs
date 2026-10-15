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

fn raw_validate(s: &str) -> Result<(), PathError> {
    if s.is_empty() {
        return Err(PathError::Empty);
    }
    if let Some(at) = s.as_bytes().iter().position(|&b| b == 0) {
        return Err(PathError::Nul { at });
    }
    if let Some(at) = s.as_bytes().iter().position(|&b| b == b'\\') {
        return Err(PathError::Backslash { at });
    }
    if let Some(at) = s.as_bytes().iter().position(|&b| b <= 31 || b == 127) {
        return Err(PathError::Control { at });
    }
    Ok(())
}

fn components(s: &str) -> Vec<(String, usize)> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
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
        out.push((s[start..i].to_string(), start));
    }
    out
}

fn directory_intent(s: &str) -> bool {
    s.ends_with('/')
        || components(s)
            .last()
            .map(|(c, _)| c == "." || c == "..")
            .unwrap_or(false)
}

fn finish_absolute(stack: &[String], intent: bool) -> String {
    let mut result = if stack.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", stack.join("/"))
    };
    if intent && result != "/" {
        result.push('/');
    }
    result
}

fn finish_normalized(stack: &[String], absolute: bool, intent: bool) -> String {
    let mut result = if absolute {
        if stack.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", stack.join("/"))
        }
    } else if stack.is_empty() {
        ".".to_string()
    } else {
        stack.join("/")
    };
    if intent && result != "/" && result != "." {
        result.push('/');
    }
    result
}

fn reduce_normalized(s: &str) -> Result<String, PathError> {
    raw_validate(s)?;
    let absolute = s.as_bytes().first() == Some(&b'/');
    let intent = directory_intent(s);
    let mut stack: Vec<String> = Vec::new();
    for (part, at) in components(s) {
        match part.as_str() {
            "." => {}
            ".." => {
                if stack.last().map(|x| x != "..").unwrap_or(false) {
                    stack.pop();
                } else if absolute {
                    return Err(PathError::Escape { at });
                } else {
                    stack.push(part);
                }
            }
            _ => stack.push(part),
        }
    }
    Ok(finish_normalized(&stack, absolute, intent))
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    reduce_normalized(path)
}

fn normalized_context(s: &str, is_home: bool) -> Result<Vec<String>, PathError> {
    if !s.starts_with('/') {
        return Err(if is_home {
            PathError::InvalidHome
        } else {
            PathError::InvalidCwd
        });
    }
    match reduce_normalized(s) {
        Ok(v) => Ok(components(&v).into_iter().map(|x| x.0).collect()),
        Err(_) => Err(if is_home {
            PathError::InvalidHome
        } else {
            PathError::InvalidCwd
        }),
    }
}

fn prepare_request<'a>(
    path: &'a str,
    home: Option<&'a str>,
) -> Result<(bool, &'a str, Vec<(String, usize)>, bool), PathError> {
    raw_validate(path)?;
    let intent = directory_intent(path);
    if path.starts_with('~') {
        if path == "~" || path.starts_with("~/") {
            let h = home.ok_or(PathError::MissingHome)?;
            let rest = if path == "~" { "" } else { &path[2..] };
            let parts = components(rest)
                .into_iter()
                .map(|(c, at)| (c, at + 2))
                .collect();
            return Ok((true, h, parts, intent));
        } else {
            return Err(PathError::UnsupportedTilde { at: 1 });
        }
    }
    Ok((
        path.starts_with('/'),
        path,
        components(path),
        intent,
    ))
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_parts = normalized_context(cwd, false)?;
    let home_parts = match home {
        Some(h) => Some(normalized_context(h, true)?),
        None => None,
    };
    let (absolute, _, parts, intent) = prepare_request(path, home)?;
    let mut stack = if absolute {
        Vec::new()
    } else if path.starts_with('~') {
        home_parts.unwrap_or_default()
    } else {
        cwd_parts
    };
    for (part, at) in parts {
        match part.as_str() {
            "." => {}
            ".." => {
                if stack.pop().is_none() {
                    return Err(PathError::Escape { at });
                }
            }
            _ => stack.push(part),
        }
    }
    Ok(finish_absolute(&stack, intent))
}

fn validate_links(links: &[Link]) -> Result<(), PathError> {
    let mut seen: Vec<(&str, usize)> = Vec::new();
    for (index, link) in links.iter().enumerate() {
        let path_valid = link.path.starts_with('/')
            && !link.path.ends_with('/')
            && link.path != "/"
            && normalize(&link.path)
                .map(|n| n == link.path)
                .unwrap_or(false);
        if !path_valid || raw_validate(&link.target).is_err() {
            return Err(PathError::InvalidLink { index });
        }
        if let Some((_, first)) = seen.iter().find(|(p, _)| *p == link.path) {
            return Err(PathError::DuplicateLink {
                first: *first,
                second: index,
            });
        }
        seen.push((&link.path, index));
    }
    Ok(())
}

#[derive(Clone)]
struct Work {
    text: String,
    parts: Vec<(String, usize)>,
    pos: usize,
    link_index: Option<usize>,
}

pub fn resolve_links(
    path: &str,
    cwd: &str,
    home: Option<&str>,
    links: &[Link],
    max_expansions: usize,
) -> Result<String, PathError> {
    let cwd_parts = normalized_context(cwd, false)?;
    let home_parts = match home {
        Some(h) => Some(normalized_context(h, true)?),
        None => None,
    };
    validate_links(links)?;
    let (absolute, _, request_parts, intent) = prepare_request(path, home)?;

    let mut stack: Vec<String> = if absolute {
        Vec::new()
    } else if path.starts_with('~') {
        home_parts.unwrap_or_default()
    } else {
        cwd_parts
    };

    let mut work = vec![Work {
        text: path.to_string(),
        parts: request_parts,
        pos: 0,
        link_index: None,
    }];
    let mut active = Vec::<usize>::new();
    let mut expansions = 0usize;

    while !work.is_empty() {
        let done = {
            let top = work.last().unwrap();
            top.pos >= top.parts.len()
        };
        if done {
            let completed = work.pop().unwrap();
            if let Some(idx) = completed.link_index {
                if let Some(pos) = active.iter().rposition(|&v| v == idx) {
                    active.remove(pos);
                }
            }
            continue;
        }
        let (part, at) = {
            let top = work.last_mut().unwrap();
            let value = top.parts[top.pos].clone();
            top.pos += 1;
            value
        };
        match part.as_str() {
            "." => continue,
            ".." => {
                if stack.pop().is_none() {
                    return Err(PathError::Escape { at });
                }
            }
            _ => {
                stack.push(part);
                let current = format!("/{}", stack.join("/"));
                if let Some((idx, link)) = links.iter().enumerate().find(|(_, l)| l.path == current) {
                    if active.contains(&idx) {
                        return Err(PathError::Loop { index: idx });
                    }
                    if expansions >= max_expansions {
                        return Err(PathError::Limit { index: idx });
                    }
                    expansions += 1;
                    active.push(idx);

                    stack.pop();
                    let target_abs = link.target.starts_with('/');
                    if target_abs {
                        stack.clear();
                    }
                    let target_parts = components(&link.target);
                    work.push(Work {
                        text: link.target.clone(),
                        parts: target_parts,
                        pos: 0,
                        link_index: Some(idx),
                    });
                }
            }
        }
    }
    Ok(finish_absolute(&stack, intent))
}

fn canonical_parts(s: &str) -> Vec<String> {
    components(s)
        .into_iter()
        .map(|(c, _)| c)
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
    let b = canonical_parts(&resolved_base);
    let t = canonical_parts(&resolved_target);
    let target_dir = resolved_target.ends_with('/');

    let mut candidates = vec![route(&b, &t)];
    for link in links {
        let probe = resolve_links(&link.path, "/", None, links, max_expansions);
        if let Ok(probe) = probe {
            let a = canonical_parts(&probe);
            if a.len() <= t.len() && a.iter().zip(t.iter()).all(|(x, y)| x == y) {
                let mut candidate = route(&b, &canonical_parts(&link.path));
                let suffix = &t[a.len()..];
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
    }

    let mut valid = Vec::<String>::new();
    for mut candidate in candidates {
        if candidate.starts_with('~') {
            candidate = format!("./{}", candidate);
        }
        if target_dir && candidate != "." {
            candidate.push('/');
        }
        if let Ok(roundtrip) =
            resolve_links(&candidate, &resolved_base, home, links, max_expansions)
        {
            if canonical_parts(&roundtrip) == t {
                valid.push(candidate);
            }
        }
    }
    valid.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes())));
    valid.into_iter().next().ok_or(PathError::Empty)
}

fn main() {}