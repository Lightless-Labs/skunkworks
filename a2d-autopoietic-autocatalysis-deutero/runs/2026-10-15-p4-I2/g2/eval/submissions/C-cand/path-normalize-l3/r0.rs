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

fn is_absolute(path: &str) -> bool {
    path.as_bytes().first() == Some(&b'/')
}

fn directory_intent(path: &str) -> bool {
    path.ends_with('/')
        || path
            .split('/')
            .filter(|component| !component.is_empty())
            .last()
            .is_some_and(|component| component == "." || component == "..")
}

fn components(path: &str) -> Vec<(usize, &str)> {
    path.split('/')
        .scan(0usize, |offset, component| {
            let start = *offset;
            *offset += component.len() + 1;
            Some((start, component))
        })
        .filter(|(_, component)| !component.is_empty())
        .collect()
}

fn apply_component(
    stack: &mut Vec<String>,
    component: &str,
    at: usize,
    absolute: bool,
) -> Result<(), PathError> {
    match component {
        "." => {}
        ".." => {
            if stack.last().is_some_and(|last| last != "..") {
                stack.pop();
            } else if absolute {
                return Err(PathError::Escape { at });
            } else {
                stack.push("..".to_string());
            }
        }
        _ => stack.push(component.to_string()),
    }
    Ok(())
}

fn render(stack: &[String], absolute: bool, intent: bool) -> String {
    let mut result = stack.join("/");
    if absolute {
        result.insert(0, '/');
        if result.is_empty() {
            result.push('/');
        }
    } else if result.is_empty() {
        result.push('.');
    }
    if intent && result != "/" && result != "." {
        result.push('/');
    }
    result
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;
    let absolute = is_absolute(path);
    let intent = directory_intent(path);
    let mut stack = Vec::new();
    for (at, component) in components(path) {
        apply_component(&mut stack, component, at, absolute)?;
    }
    Ok(render(&stack, absolute, intent))
}

fn validate_context(path: &str, error: PathError) -> Result<Vec<String>, PathError> {
    if !is_absolute(path) {
        return Err(error);
    }
    let normalized = normalize(path).map_err(|_| error)?;
    Ok(components(&normalized)
        .into_iter()
        .map(|(_, component)| component.to_string())
        .collect())
}

fn tilde_remainder<'a>(
    path: &'a str,
    home: Option<&'a [String]>,
) -> Result<(bool, Vec<String>, &'a str, usize), PathError> {
    if path.starts_with('~') {
        let bytes = path.as_bytes();
        if bytes.len() == 1 || bytes[1] == b'/' {
            let home = home.ok_or(PathError::MissingHome)?;
            let consumed = if bytes.len() > 1 { 2 } else { 1 };
            return Ok((
                true,
                home.to_vec(),
                &path[consumed..],
                consumed,
            ));
        }
        return Err(PathError::UnsupportedTilde { at: 1 });
    }
    Ok((false, Vec::new(), path, 0))
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_components = validate_context(cwd, PathError::InvalidCwd)?;
    let home_components = match home {
        Some(value) => Some(validate_context(value, PathError::InvalidHome)?),
        None => None,
    };

    validate_raw(path)?;
    let intent = directory_intent(path);

    let (expanded, tilde_components, remainder, consumed) =
        tilde_remainder(path, home_components.as_deref())?;

    let absolute_request = !expanded && is_absolute(path);
    let mut stack = if absolute_request {
        Vec::new()
    } else if expanded {
        tilde_components
    } else {
        cwd_components
    };

    let source = if absolute_request {
        path
    } else if expanded {
        remainder
    } else {
        path
    };
    let base_offset = if expanded { consumed } else { 0 };

    for (at, component) in components(source) {
        apply_component(
            &mut stack,
            component,
            at + base_offset,
            true,
        )?;
    }
    Ok(render(&stack, true, intent))
}

#[derive(Clone, Copy)]
struct WorkItem<'a> {
    raw: &'a str,
    base_offset: usize,
    target_link: Option<usize>,
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

    let mut seen: Vec<(&str, usize)> = Vec::new();
    for (index, link) in links.iter().enumerate() {
        let valid_path = is_absolute(&link.path)
            && !link.path.ends_with('/')
            && link.path != "/"
            && normalize(&link.path)
                .is_ok_and(|normalized| normalized == link.path);
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

    validate_raw(path)?;
    let intent = directory_intent(path);
    let (expanded, tilde_components, remainder, consumed) =
        tilde_remainder(path, home_components.as_deref())?;

    let absolute_request = !expanded && is_absolute(path);
    let mut stack = if absolute_request {
        Vec::new()
    } else if expanded {
        tilde_components
    } else {
        cwd_components
    };

    let (source, base_offset) = if absolute_request {
        (path, 0)
    } else if expanded {
        (remainder, consumed)
    } else {
        (path, 0)
    };

    let mut work = vec![WorkItem {
        raw: source,
        base_offset,
        target_link: None,
    }];
    let mut active = Vec::<usize>::new();
    let mut expansions = 0usize;

    while let Some(item) = work.pop() {
        if let Some(link_index) = item.target_link {
            if active.last() == Some(&link_index) {
                active.pop();
            } else if let Some(position) = active.iter().rposition(|&i| i == link_index) {
                active.remove(position);
            }
        }

        let parts = components(item.raw);
        for (position, (at, component)) in parts.iter().enumerate() {
            let original_at = at + item.base_offset;
            match *component {
                "." => {}
                ".." => {
                    if stack.last().is_some_and(|last| last != "..") {
                        stack.pop();
                    } else {
                        return Err(PathError::Escape { at: original_at });
                    }
                }
                ordinary => {
                    stack.push(ordinary.to_string());
                    let current_path = format!("/{}", stack.join("/"));
                    if let Some((link_index, link)) = links
                        .iter()
                        .enumerate()
                        .find(|(_, link)| link.path == current_path)
                    {
                        if active.contains(&link_index) {
                            return Err(PathError::Loop { index: link_index });
                        }
                        if expansions >= max_expansions {
                            return Err(PathError::Limit { index: link_index });
                        }
                        expansions += 1;

                        // Relative targets begin at the link's parent.
                        stack.pop();
                        if is_absolute(&link.target) {
                            stack.clear();
                        }
                        active.push(link_index);

                        let remaining = parts[position + 1..]
                            .first()
                            .map(|(next_at, _)| &item.raw[*next_at..])
                            .unwrap_or("");
                        let mut target_work = Vec::new();
                        if !remaining.is_empty() {
                            target_work.push(WorkItem {
                                raw: remaining,
                                base_offset: item.base_offset
                                    + parts[position + 1].0,
                                target_link: None,
                            });
                        }
                        target_work.push(WorkItem {
                            raw: &link.target,
                            base_offset: 0,
                            target_link: Some(link_index),
                        });
                        work.extend(target_work);
                        break;
                    }
                }
            }
        }
    }

    Ok(render(&stack, true, intent))
}

fn main() {}