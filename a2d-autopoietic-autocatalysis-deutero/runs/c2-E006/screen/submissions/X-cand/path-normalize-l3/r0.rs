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

fn components(path: &str, base: usize) -> Vec<(String, usize)> {
    let mut result = Vec::new();
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
        result.push((path[start..i].to_owned(), base + start));
    }
    result
}

fn directory_intent(path: &str) -> bool {
    if path.as_bytes().last() == Some(&b'/') {
        return true;
    }
    components(path, 0)
        .last()
        .map(|(c, _)| c == "." || c == "..")
        .unwrap_or(false)
}

fn render(parts: &[String], absolute: bool, intent: bool) -> String {
    let mut out = if absolute {
        format!("/{}", parts.join("/"))
    } else {
        parts.join("/")
    };
    if absolute && parts.is_empty() {
        return "/".to_owned();
    }
    if !absolute && parts.is_empty() {
        return ".".to_owned();
    }
    if intent {
        out.push('/');
    }
    out
}

fn reduce(path: &str, absolute: bool) -> Result<Vec<String>, PathError> {
    let mut stack: Vec<String> = Vec::new();
    for (part, at) in components(path, 0) {
        match part.as_str() {
            "." => {}
            ".." => {
                if stack.last().is_some_and(|p| p != "..") {
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
    Ok(stack)
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;
    let absolute = path.starts_with('/');
    let intent = directory_intent(path);
    let parts = reduce(path, absolute)?;
    Ok(render(&parts, absolute, intent))
}

fn validate_context(path: &str, error: PathError) -> Result<Vec<String>, PathError> {
    if !path.starts_with('/') {
        return Err(error);
    }
    match normalize(path) {
        Ok(normalized) => Ok(components(&normalized, 0)
            .into_iter()
            .map(|(part, _)| part)
            .collect()),
        Err(_) => Err(error),
    }
}

fn validate_links(links: &[Link]) -> Result<(), PathError> {
    let mut seen: Vec<(&str, usize)> = Vec::new();
    for (index, link) in links.iter().enumerate() {
        let valid_path = link.path.starts_with('/')
            && !link.path.ends_with('/')
            && link.path != "/"
            && normalize(&link.path)
                .map(|n| n == link.path)
                .unwrap_or(false);
        if !valid_path || validate_raw(&link.target).is_err() {
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

fn expand_request<'a>(
    path: &'a str,
    home: Option<&'a [String]>,
) -> Result<(&'a str, usize, Option<&'a [String]>), PathError> {
    if !path.starts_with('~') {
        return Ok((path, 0, None));
    }
    let bytes = path.as_bytes();
    if bytes.len() > 1 && bytes[1] != b'/' {
        return Err(PathError::UnsupportedTilde { at: 1 });
    }
    let home = home.ok_or(PathError::MissingHome)?;
    let skip = if bytes.len() > 1 { 2 } else { 1 };
    Ok((&path[skip..], skip, Some(home)))
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_parts = validate_context(cwd, PathError::InvalidCwd)?;
    let home_parts = match home {
        Some(h) => Some(validate_context(h, PathError::InvalidHome)?),
        None => None,
    };
    validate_raw(path)?;

    let (request, offset, tilde_home) = expand_request(path, home_parts.as_deref())?;
    let absolute = path.starts_with('/');
    let intent = directory_intent(path);
    let mut stack = if absolute {
        Vec::new()
    } else if let Some(h) = tilde_home {
        h.to_vec()
    } else {
        cwd_parts
    };

    for (part, at) in components(request, offset) {
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
    Ok(render(&stack, true, intent))
}

#[derive(Clone)]
struct Item {
    text: String,
    at: usize,
}

pub fn resolve_links(
    path: &str,
    cwd: &str,
    home: Option<&str>,
    links: &[Link],
    max_expansions: usize,
) -> Result<String, PathError> {
    let cwd_parts = validate_context(cwd, PathError::InvalidCwd)?;
    let home_parts = match home {
        Some(h) => Some(validate_context(h, PathError::InvalidHome)?),
        None => None,
    };
    validate_links(links)?;
    validate_raw(path)?;

    let (request, offset, tilde_home) = expand_request(path, home_parts.as_deref())?;
    let absolute_request = path.starts_with('/');
    let intent = directory_intent(path);

    let mut stack: Vec<String> = Vec::new();
    let mut count = 0usize;
    let mut active: Vec<usize> = Vec::new();

    let mut initial = if absolute_request {
        Vec::new()
    } else if let Some(h) = tilde_home {
        h.to_vec()
    } else {
        cwd_parts
    };

    // Context components are traversed from root and can themselves match links.
    let mut work: Vec<Item> = initial
        .drain(..)
        .map(|text| Item { text, at: 0 })
        .collect();
    work.extend(
        components(request, offset)
            .into_iter()
            .map(|(text, at)| Item { text, at }),
    );

    // Frames retain the suspended traversal and the link active until its target ends.
    let mut frames: Vec<(Vec<Item>, usize)> = Vec::new();
    let mut cursor = 0usize;

    loop {
        if cursor >= work.len() {
            if let Some((saved, link_index)) = frames.pop() {
                active.pop();
                work = saved;
                cursor = 0;
                continue;
            }
            break;
        }

        let item = work[cursor].clone();
        cursor += 1;

        match item.text.as_str() {
            "." => continue,
            ".." => {
                if stack.last().is_some_and(|p| p != "..") {
                    stack.pop();
                } else {
                    return Err(PathError::Escape { at: item.at });
                }
                continue;
            }
            _ => stack.push(item.text.clone()),
        }

        let key = format!("/{}", stack.join("/"));
        if let Some((index, link)) = links.iter().enumerate().find(|(_, link)| link.path == key) {
            if active.contains(&index) {
                return Err(PathError::Loop { index });
            }
            if count >= max_expansions {
                return Err(PathError::Limit { index });
            }
            count += 1;

            // Remove the matched link name before processing a relative target.
            stack.pop();
            if link.target.starts_with('/') {
                stack.clear();
            }
            let target_items: Vec<Item> = components(&link.target, 0)
                .into_iter()
                .map(|(text, at)| Item { text, at })
                .collect();

            let saved = work[cursor..].to_vec();
            frames.push((saved, index));
            active.push(index);
            work = target_items;
            cursor = 0;
        }
    }

    Ok(render(&stack, true, intent))
}