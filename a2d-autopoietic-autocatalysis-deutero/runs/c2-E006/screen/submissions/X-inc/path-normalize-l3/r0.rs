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

fn render(stack: &[String], absolute: bool, intent: bool) -> String {
    let mut result = stack.join("/");
    if absolute {
        result.insert(0, '/');
    }
    if result.is_empty() {
        result = if absolute { "/".into() } else { ".".into() };
    }
    if intent && result != "/" && result != "." {
        result.push('/');
    }
    result
}

fn reduce_plain(
    path: &str,
    absolute: bool,
    initial: Vec<String>,
) -> Result<String, PathError> {
    validate_raw(path)?;
    let intent = directory_intent(path);
    let mut stack = initial;
    for (part, at) in components(path) {
        match part.as_str() {
            "." => {}
            ".." => {
                if stack.last().is_some_and(|last| last != "..") {
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
    Ok(render(&stack, absolute, intent))
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    let absolute = path.as_bytes().first() == Some(&b'/');
    reduce_plain(path, absolute, Vec::new())
}

fn validate_context(value: &str, error: PathError) -> Result<Vec<String>, PathError> {
    if !value.starts_with('/') {
        return Err(error);
    }
    match normalize(value) {
        Ok(normalized) => Ok(components(&normalized)
            .into_iter()
            .map(|(part, _)| part)
            .collect()),
        Err(_) => Err(error),
    }
}

fn expand_request<'a>(
    path: &'a str,
    home: Option<&'a [String]>,
) -> Result<(bool, Vec<String>, usize), PathError> {
    if path.starts_with('~') {
        if path.len() == 1 || path.as_bytes().get(1) == Some(&b'/') {
            let base = home.ok_or(PathError::MissingHome)?.to_vec();
            let consumed = if path.len() > 1 { 2 } else { 1 };
            Ok((true, base, consumed))
        } else {
            Err(PathError::UnsupportedTilde { at: 1 })
        }
    } else if path.starts_with('/') {
        Ok((true, Vec::new(), 0))
    } else {
        Ok((false, Vec::new(), 0))
    }
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_parts = validate_context(cwd, PathError::InvalidCwd)?;
    let home_parts = match home {
        Some(value) => Some(validate_context(value, PathError::InvalidHome)?),
        None => None,
    };
    validate_raw(path)?;
    let (absolute, base, consumed) = expand_request(path, home_parts.as_deref())?;
    let initial = if path.starts_with('~') || absolute {
        base
    } else {
        cwd_parts
    };
    let remainder = &path[consumed..];
    let mut stack = initial;
    for (part, at) in components(remainder) {
        match part.as_str() {
            "." => {}
            ".." => {
                if stack.pop().is_none() {
                    return Err(PathError::Escape { at: at + consumed });
                }
            }
            _ => stack.push(part),
        }
    }
    Ok(render(&stack, true, directory_intent(path)))
}

#[derive(Clone)]
struct WorkComponent {
    value: String,
    at: usize,
}

#[derive(Clone)]
struct Frame {
    parts: Vec<WorkComponent>,
    pos: usize,
    source: String,
    link_index: Option<usize>,
}

fn make_parts(path: &str, offset: usize) -> Vec<WorkComponent> {
    components(path)
        .into_iter()
        .map(|(value, at)| WorkComponent {
            value,
            at: at + offset,
        })
        .collect()
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
        Some(value) => Some(validate_context(value, PathError::InvalidHome)?),
        None => None,
    };

    let mut keys = Vec::<(String, usize, String)>::new();
    for (index, link) in links.iter().enumerate() {
        let valid_path = link.path.starts_with('/')
            && !link.path.ends_with('/')
            && link.path != "/"
            && normalize(&link.path)
                .is_ok_and(|normalized| normalized == link.path);
        if !valid_path || validate_raw(&link.target).is_err() {
            return Err(PathError::InvalidLink { index });
        }
        if let Some((_, first, _)) = keys.iter().find(|(key, _, _)| key == &link.path) {
            return Err(PathError::DuplicateLink {
                first: *first,
                second: index,
            });
        }
        keys.push((link.path.clone(), index, link.target.clone()));
    }

    validate_raw(path)?;
    let intent = directory_intent(path);
    let (request_absolute, request_base, consumed) =
        expand_request(path, home_parts.as_deref())?;

    let mut stack = if path.starts_with('~') {
        request_base
    } else if request_absolute {
        Vec::new()
    } else {
        cwd_parts
    };

    let mut frames = Vec::new();
    frames.push(Frame {
        parts: make_parts(&path[consumed..], consumed),
        pos: 0,
        source: path.to_string(),
        link_index: None,
    });

    let mut active = Vec::<usize>::new();
    let mut expansions = 0usize;

    loop {
        let Some(frame) = frames.last_mut() else {
            break;
        };
        if frame.pos >= frame.parts.len() {
            let done = frames.pop().unwrap();
            if let Some(index) = done.link_index {
                if let Some(pos) = active.iter().rposition(|&x| x == index) {
                    active.remove(pos);
                }
            }
            continue;
        }

        let item = frame.parts[frame.pos].clone();
        frame.pos += 1;

        match item.value.as_str() {
            "." => continue,
            ".." => {
                if stack.pop().is_none() {
                    return Err(PathError::Escape { at: item.at });
                }
                continue;
            }
            _ => stack.push(item.value.clone()),
        }

        let current = render(&stack, true, false);
        if let Some((_, index, target)) = keys.iter().find(|(key, _, _)| key == &current) {
            if active.contains(index) {
                return Err(PathError::Loop { index: *index });
            }
            if expansions >= max_expansions {
                return Err(PathError::Limit { index: *index });
            }
            expansions += 1;
            active.push(*index);
            stack.pop();

            let absolute_target = target.starts_with('/');
            if absolute_target {
                stack.clear();
            }
            frames.push(Frame {
                parts: make_parts(target, 0),
                pos: 0,
                source: target.clone(),
                link_index: Some(*index),
            });
        }
    }

    Ok(render(&stack, true, intent))
}