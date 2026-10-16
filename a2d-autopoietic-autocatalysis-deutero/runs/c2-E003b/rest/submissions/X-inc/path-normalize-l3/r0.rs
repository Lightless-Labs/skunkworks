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
    path.ends_with('/')
        || path
            .split('/')
            .filter(|part| !part.is_empty())
            .last()
            .is_some_and(|part| part == "." || part == "..")
}

fn components(path: &str) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    let bytes = path.as_bytes();
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
        out.push((path[start..i].to_string(), start));
    }
    out
}

fn finish_absolute(stack: &[String], dir: bool) -> String {
    let mut result = String::from("/");
    result.push_str(&stack.join("/"));
    if dir && result != "/" {
        result.push('/');
    }
    result
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;
    let absolute = path.starts_with('/');
    let dir = directory_intent(path);
    let mut stack: Vec<String> = Vec::new();

    for (part, at) in components(path) {
        match part.as_str() {
            "." => {}
            ".." => {
                if stack.last().is_some_and(|s| s != "..") {
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

    if absolute {
        Ok(finish_absolute(&stack, dir))
    } else if stack.is_empty() {
        Ok(".".to_string())
    } else {
        let mut result = stack.join("/");
        if dir && result != "." {
            result.push('/');
        }
        Ok(result)
    }
}

fn valid_context(path: &str) -> Result<Vec<String>, ()> {
    if !path.starts_with('/') {
        return Err(());
    }
    let normalized = normalize(path).map_err(|_| ())?;
    Ok(normalized
        .split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect())
}

fn tilde_remainder<'a>(
    path: &'a str,
    home: Option<&str>,
) -> Result<(&'a str, bool), PathError> {
    if path == "~" {
        return home
            .map(|_| ("", true))
            .ok_or(PathError::MissingHome);
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return home
            .map(|_| (rest, true))
            .ok_or(PathError::MissingHome);
    }
    if path.starts_with('~') {
        return Err(PathError::UnsupportedTilde { at: 1 });
    }
    Ok((path, false))
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_parts = valid_context(cwd).map_err(|_| PathError::InvalidCwd)?;
    let home_parts = match home {
        Some(h) => Some(valid_context(h).map_err(|_| PathError::InvalidHome)?),
        None => None,
    };

    validate_raw(path)?;
    let dir = directory_intent(path);
    let (request, tilde_home) = tilde_remainder(path, home)?;

    let mut stack = if path.starts_with('/') {
        Vec::new()
    } else if tilde_home {
        home_parts.unwrap_or_default()
    } else {
        cwd_parts
    };

    let request_base = path.len() - request.len();
    for (part, at) in components(request) {
        match part.as_str() {
            "." => {}
            ".." => {
                if stack.pop().is_none() {
                    return Err(PathError::Escape {
                        at: request_base + at,
                    });
                }
            }
            _ => stack.push(part),
        }
    }

    Ok(finish_absolute(&stack, dir))
}

fn validate_links(links: &[Link]) -> Result<(), PathError> {
    let mut seen: Vec<(&str, usize)> = Vec::new();

    for (index, link) in links.iter().enumerate() {
        let path_valid = link.path.starts_with('/')
            && !link.path.ends_with('/')
            && link.path != "/"
            && normalize(&link.path).is_ok_and(|normalized| normalized == link.path);
        let target_valid = validate_raw(&link.target).is_ok();

        if !path_valid || !target_valid {
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

#[derive(Clone)]
struct WorkItem {
    text: String,
    offset: usize,
    finish_link: Option<usize>,
}

pub fn resolve_links(
    path: &str,
    cwd: &str,
    home: Option<&str>,
    links: &[Link],
    max_expansions: usize,
) -> Result<String, PathError> {
    let cwd_parts = valid_context(cwd).map_err(|_| PathError::InvalidCwd)?;
    let home_parts = match home {
        Some(h) => Some(valid_context(h).map_err(|_| PathError::InvalidHome)?),
        None => None,
    };

    validate_links(links)?;
    validate_raw(path)?;
    let dir = directory_intent(path);
    let (request, tilde_home) = tilde_remainder(path, home)?;

    let absolute = path.starts_with('/');
    let starting = if absolute {
        Vec::new()
    } else if tilde_home {
        home_parts.unwrap_or_default()
    } else {
        cwd_parts
    };
    let mut stack = starting;

    let request_base = path.len() - request.len();
    let mut queue: Vec<WorkItem> = components(request)
        .into_iter()
        .map(|(text, at)| WorkItem {
            text,
            offset: request_base + at,
            finish_link: None,
        })
        .collect();

    let mut active: Vec<usize> = Vec::new();
    let mut expansions = 0usize;
    let mut cursor = 0usize;

    while cursor < queue.len() {
        let item = queue[cursor].clone();
        cursor += 1;

        if let Some(index) = item.finish_link {
            if let Some(pos) = active.iter().rposition(|&value| value == index) {
                active.remove(pos);
            }
            continue;
        }

        match item.text.as_str() {
            "." => {}
            ".." => {
                if stack.pop().is_none() {
                    return Err(PathError::Escape { at: item.offset });
                }
            }
            _ => {
                stack.push(item.text.clone());
                let current = finish_absolute(&stack, false);
                if let Some((index, link)) = links
                    .iter()
                    .enumerate()
                    .find(|(_, link)| link.path == current)
                {
                    if active.contains(&index) {
                        return Err(PathError::Loop { index });
                    }
                    if expansions >= max_expansions {
                        return Err(PathError::Limit { index });
                    }
                    expansions += 1;
                    active.push(index);

                    stack.pop();
                    if link.target.starts_with('/') {
                        stack.clear();
                    }

                    let mut replacement: Vec<WorkItem> = components(&link.target)
                        .into_iter()
                        .map(|(text, offset)| WorkItem {
                            text,
                            offset,
                            finish_link: None,
                        })
                        .collect();
                    replacement.push(WorkItem {
                        text: String::new(),
                        offset: 0,
                        finish_link: Some(index),
                    });

                    let remaining = queue.split_off(cursor);
                    queue.truncate(cursor - 1);
                    queue.extend(replacement);
                    queue.extend(remaining);
                    cursor -= 1;
                }
            }
        }
    }

    Ok(finish_absolute(&stack, dir))
}