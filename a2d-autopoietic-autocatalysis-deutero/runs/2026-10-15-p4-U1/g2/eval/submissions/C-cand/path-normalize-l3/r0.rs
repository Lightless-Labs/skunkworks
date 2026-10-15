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

fn components(path: &str, base_offset: usize) -> Vec<(String, usize)> {
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
        result.push((path[start..i].to_owned(), base_offset + start));
    }
    result
}

fn directory_intent(path: &str) -> bool {
    path.as_bytes().last() == Some(&b'/')
        || components(path, 0)
            .last()
            .is_some_and(|(component, _)| component == "." || component == "..")
}

fn render_absolute(parts: &[String], intent: bool) -> String {
    if parts.is_empty() {
        return "/".to_owned();
    }
    let mut result = format!("/{}", parts.join("/"));
    if intent {
        result.push('/');
    }
    result
}

fn render_normalized(parts: &[String], absolute: bool, intent: bool) -> String {
    if absolute {
        return render_absolute(parts, intent);
    }
    if parts.is_empty() {
        return ".".to_owned();
    }
    let mut result = parts.join("/");
    if intent {
        result.push('/');
    }
    result
}

fn reduce_component(
    parts: &mut Vec<String>,
    component: &str,
    absolute: bool,
    at: usize,
) -> Result<(), PathError> {
    match component {
        "." => {}
        ".." => {
            if parts.last().is_some_and(|part| part != "..") {
                parts.pop();
            } else if absolute {
                return Err(PathError::Escape { at });
            } else {
                parts.push("..".to_owned());
            }
        }
        _ => parts.push(component.to_owned()),
    }
    Ok(())
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;
    let absolute = path.as_bytes()[0] == b'/';
    let intent = directory_intent(path);
    let mut parts = Vec::new();

    for (component, at) in components(path, 0) {
        reduce_component(&mut parts, &component, absolute, at)?;
    }
    Ok(render_normalized(&parts, absolute, intent))
}

fn validated_context(path: &str) -> Result<Vec<String>, ()> {
    if !path.starts_with('/') {
        return Err(());
    }
    let normalized = normalize(path).map_err(|_| ())?;
    Ok(components(&normalized, 0)
        .into_iter()
        .map(|(component, _)| component)
        .collect())
}

fn validate_contexts(
    cwd: &str,
    home: Option<&str>,
) -> Result<(Vec<String>, Option<Vec<String>>), PathError> {
    let cwd_parts = validated_context(cwd).map_err(|_| PathError::InvalidCwd)?;
    let home_parts = match home {
        Some(value) => Some(validated_context(value).map_err(|_| PathError::InvalidHome)?),
        None => None,
    };
    Ok((cwd_parts, home_parts))
}

struct PreparedRequest<'a> {
    absolute: bool,
    intent: bool,
    remainder: &'a str,
    offset: usize,
    tilde: bool,
}

fn prepare_request<'a>(path: &'a str) -> Result<PreparedRequest<'a>, PathError> {
    validate_raw(path)?;
    let absolute = path.as_bytes()[0] == b'/';
    let intent = directory_intent(path);

    if path.as_bytes()[0] == b'~' {
        if path.len() > 1 && path.as_bytes()[1] != b'/' {
            return Err(PathError::UnsupportedTilde { at: 1 });
        }
        if path.len() == 1 {
            return Ok(PreparedRequest {
                absolute: false,
                intent,
                remainder: "",
                offset: 1,
                tilde: true,
            });
        }
        return Ok(PreparedRequest {
            absolute: false,
            intent,
            remainder: &path[2..],
            offset: 2,
            tilde: true,
        });
    }

    Ok(PreparedRequest {
        absolute,
        intent,
        remainder: path,
        offset: 0,
        tilde: false,
    })
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let (cwd_parts, home_parts) = validate_contexts(cwd, home)?;
    let request = prepare_request(path)?;

    let mut parts = if request.absolute {
        Vec::new()
    } else if request.tilde {
        home_parts.ok_or(PathError::MissingHome)?
    } else {
        cwd_parts
    };

    for (component, at) in components(request.remainder, request.offset) {
        reduce_component(&mut parts, &component, true, at)?;
    }
    Ok(render_absolute(&parts, request.intent))
}

fn absolute_key(parts: &[String]) -> String {
    if parts.is_empty() {
        "/".to_owned()
    } else {
        format!("/{}", parts.join("/"))
    }
}

enum Work {
    Component(String, usize),
    EndTarget(usize),
}

fn process_components(
    items: Vec<(String, usize)>,
    stack: &mut Vec<String>,
    links: &[Link],
    indices: &HashMap<String, usize>,
    active: &mut Vec<usize>,
    expansions: &mut usize,
    max_expansions: usize,
) -> Result<(), PathError> {
    let mut work: Vec<Work> = items
        .into_iter()
        .rev()
        .map(|(component, at)| Work::Component(component, at))
        .collect();

    while let Some(item) = work.pop() {
        match item {
            Work::EndTarget(index) => {
                if let Some(position) = active.iter().rposition(|&entry| entry == index) {
                    active.remove(position);
                }
            }
            Work::Component(component, at) => match component.as_str() {
                "." => {}
                ".." => {
                    if stack.pop().is_none() {
                        return Err(PathError::Escape { at });
                    }
                }
                _ => {
                    stack.push(component);
                    let key = absolute_key(stack);
                    if let Some(&index) = indices.get(&key) {
                        if active.contains(&index) {
                            return Err(PathError::Loop { index });
                        }
                        if *expansions >= max_expansions {
                            return Err(PathError::Limit { index });
                        }
                        *expansions += 1;
                        active.push(index);

                        let target = &links[index].target;
                        let target_absolute = target.starts_with('/');
                        if target_absolute {
                            stack.clear();
                        } else {
                            stack.pop();
                        }

                        // Keep the remaining request suspended until the target and
                        // all links reached through it have completed.
                        let remaining = std::mem::take(&mut work);
                        work.push(Work::EndTarget(index));
                        work.extend(remaining);
                        for (target_component, target_at) in
                            components(target, 0).into_iter().rev()
                        {
                            work.push(Work::Component(target_component, target_at));
                        }
                    }
                }
            },
        }
    }
    Ok(())
}

pub fn resolve_links(
    path: &str,
    cwd: &str,
    home: Option<&str>,
    links: &[Link],
    max_expansions: usize,
) -> Result<String, PathError> {
    let (cwd_parts, home_parts) = validate_contexts(cwd, home)?;

    let mut indices = HashMap::new();
    for (index, link) in links.iter().enumerate() {
        let path_valid = link.path.starts_with('/')
            && link.path != "/"
            && !link.path.ends_with('/')
            && normalize(&link.path)
                .map(|normalized| normalized == link.path)
                .unwrap_or(false);
        let target_valid = validate_raw(&link.target).is_ok();

        if !path_valid || !target_valid {
            return Err(PathError::InvalidLink { index });
        }
        if let Some(&first) = indices.get(&link.path) {
            return Err(PathError::DuplicateLink {
                first,
                second: index,
            });
        }
        indices.insert(link.path.clone(), index);
    }

    let request = prepare_request(path)?;
    let mut stack = if request.absolute {
        Vec::new()
    } else if request.tilde {
        home_parts.ok_or(PathError::MissingHome)?
    } else {
        cwd_parts
    };

    let items = components(request.remainder, request.offset);
    let mut active = Vec::new();
    let mut expansions = 0;

    process_components(
        items,
        &mut stack,
        links,
        &indices,
        &mut active,
        &mut expansions,
        max_expansions,
    )?;

    Ok(render_absolute(&stack, request.intent))
}

fn main() {}