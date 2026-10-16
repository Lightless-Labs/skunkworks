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

#[derive(Clone, Copy, Debug)]
struct Component<'a> {
    text: &'a str,
    at: usize,
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

fn components(path: &str) -> Vec<Component<'_>> {
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
        out.push(Component {
            text: &path[start..i],
            at: start,
        });
    }
    out
}

fn directory_intent(path: &str) -> bool {
    path.ends_with('/')
        || components(path)
            .last()
            .is_some_and(|c| c.text == "." || c.text == "..")
}

fn render_absolute(stack: &[String], trailing: bool) -> String {
    if stack.is_empty() {
        return "/".to_string();
    }
    let mut out = String::from("/");
    out.push_str(&stack.join("/"));
    if trailing {
        out.push('/');
    }
    out
}

fn render_normalized(stack: &[String], absolute: bool, trailing: bool) -> String {
    if absolute {
        return render_absolute(stack, trailing && !stack.is_empty());
    }
    if stack.is_empty() {
        return ".".to_string();
    }
    let mut out = stack.join("/");
    if trailing {
        out.push('/');
    }
    out
}

fn reduce_component(
    stack: &mut Vec<String>,
    component: &str,
    absolute: bool,
    at: usize,
) -> Result<(), PathError> {
    match component {
        "." => {}
        ".." => {
            if stack.last().is_some_and(|s| s != "..") {
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

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;
    let absolute = path.starts_with('/');
    let trailing = directory_intent(path);
    let mut stack = Vec::new();
    for c in components(path) {
        reduce_component(&mut stack, c.text, absolute, c.at)?;
    }
    Ok(render_normalized(&stack, absolute, trailing))
}

fn validate_context(path: &str) -> Result<Vec<String>, ()> {
    if !path.starts_with('/') {
        return Err(());
    }
    let normalized = normalize(path).map_err(|_| ())?;
    Ok(components(&normalized)
        .into_iter()
        .map(|c| c.text.to_string())
        .collect())
}

fn validate_contexts(
    cwd: &str,
    home: Option<&str>,
) -> Result<(Vec<String>, Option<Vec<String>>), PathError> {
    let cwd_components = validate_context(cwd).map_err(|_| PathError::InvalidCwd)?;
    let home_components = match home {
        Some(h) => Some(validate_context(h).map_err(|_| PathError::InvalidHome)?),
        None => None,
    };
    Ok((cwd_components, home_components))
}

fn request_start<'a>(
    path: &'a str,
    home: Option<&[String]>,
) -> Result<(&'a str, usize, bool, bool), PathError> {
    if !path.starts_with('~') {
        return Ok((path, 0, path.starts_with('/'), false));
    }
    let bytes = path.as_bytes();
    if bytes.len() > 1 && bytes[1] != b'/' {
        return Err(PathError::UnsupportedTilde { at: 1 });
    }
    if home.is_none() {
        return Err(PathError::MissingHome);
    }
    let consumed = if bytes.len() > 1 { 2 } else { 1 };
    Ok((&path[consumed..], consumed, false, true))
}

fn resolve_impl(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let (cwd_components, home_components) = validate_contexts(cwd, home)?;
    validate_raw(path)?;
    let (request, offset, absolute, expanded) =
        request_start(path, home_components.as_deref())?;

    let mut stack = if absolute {
        Vec::new()
    } else if expanded {
        home_components.unwrap_or_default()
    } else {
        cwd_components
    };

    for c in components(request) {
        reduce_component(&mut stack, c.text, true, c.at + offset)?;
    }
    Ok(render_absolute(&stack, directory_intent(path)))
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    resolve_impl(path, cwd, home)
}

fn validate_links(links: &[Link]) -> Result<(), PathError> {
    let mut seen: Vec<(&str, usize)> = Vec::new();
    for (index, link) in links.iter().enumerate() {
        let valid_path = link.path.starts_with('/')
            && link.path != "/"
            && !link.path.ends_with('/')
            && normalize(&link.path).is_ok_and(|n| n == link.path);
        if !valid_path || validate_raw(&link.target).is_err() {
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
struct WorkComponent {
    text: String,
    at: usize,
}

#[derive(Clone)]
enum Event {
    Component(WorkComponent),
    EndTarget(usize),
}

fn work_components(path: &str, offset: usize) -> Vec<WorkComponent> {
    components(path)
        .into_iter()
        .map(|c| WorkComponent {
            text: c.text.to_string(),
            at: c.at + offset,
        })
        .collect()
}

fn stack_key(stack: &[String]) -> String {
    render_absolute(stack, false)
}

fn reduce_absolute(stack: &mut Vec<String>, component: &WorkComponent) -> Result<(), PathError> {
    match component.text.as_str() {
        "." => {}
        ".." => {
            if stack.pop().is_none() {
                return Err(PathError::Escape { at: component.at });
            }
        }
        _ => stack.push(component.text.clone()),
    }
    Ok(())
}

fn resolve_links_components(
    path: &str,
    cwd_components: &[String],
    home_components: Option<&[String]>,
    links: &[Link],
    max_expansions: usize,
) -> Result<(Vec<String>, bool), PathError> {
    let (request, request_offset, absolute, expanded) =
        request_start(path, home_components)?;

    let mut stack = if absolute {
        Vec::new()
    } else if expanded {
        home_components.unwrap_or_default().to_vec()
    } else {
        cwd_components.to_vec()
    };

    let mut events: Vec<Event> = work_components(request, request_offset)
        .into_iter()
        .rev()
        .map(Event::Component)
        .collect();
    let trailing = directory_intent(path);
    let mut active = Vec::<usize>::new();
    let mut expansions = 0usize;

    while let Some(event) = events.pop() {
        match event {
            Event::EndTarget(index) => {
                if active.last() == Some(&index) {
                    active.pop();
                }
            }
            Event::Component(component) => {
                if component.text == "." || component.text == ".." {
                    reduce_absolute(&mut stack, &component)?;
                    continue;
                }

                stack.push(component.text.clone());
                let key = stack_key(&stack);
                if let Some((index, link)) = links
                    .iter()
                    .enumerate()
                    .find(|(_, link)| link.path == key)
                {
                    if active.contains(&index) {
                        return Err(PathError::Loop { index });
                    }
                    if expansions >= max_expansions {
                        return Err(PathError::Limit { index });
                    }
                    expansions += 1;

                    // The matched key is replaced by its target. A relative
                    // target begins at the key's parent; an absolute target
                    // begins at root.
                    stack.pop();
                    if link.target.starts_with('/') {
                        stack.clear();
                    }
                    active.push(index);

                    let pending = std::mem::take(&mut events);
                    let mut target_events: Vec<Event> =
                        work_components(&link.target, 0).into_iter().map(Event::Component).collect();
                    target_events.reverse();

                    // Target events must run before the pending request, and
                    // the link remains active through nested target expansion.
                    events = target_events;
                    events.push(Event::EndTarget(index));
                    events.extend(pending);
                }
            }
        }
    }
    Ok((stack, trailing))
}

pub fn resolve_links(
    path: &str,
    cwd: &str,
    home: Option<&str>,
    links: &[Link],
    max_expansions: usize,
) -> Result<String, PathError> {
    let (cwd_components, home_components) = validate_contexts(cwd, home)?;
    validate_links(links)?;
    validate_raw(path)?;
    let (stack, trailing) = resolve_links_components(
        path,
        &cwd_components,
        home_components.as_deref(),
        links,
        max_expansions,
    )?;
    Ok(render_absolute(&stack, trailing))
}

fn absolute_components(path: &str) -> Vec<String> {
    components(path)
        .into_iter()
        .map(|c| c.text.to_string())
        .collect()
}

fn route(base: &[String], target: &[String]) -> String {
    let mut common = 0;
    while common < base.len() && common < target.len() && base[common] == target[common] {
        common += 1;
    }
    let mut parts = Vec::new();
    parts.extend((common..base.len()).map(|_| "..".to_string()));
    parts.extend(target[common..].iter().cloned());
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
    let base_resolved = resolve_links(base, cwd, home, links, max_expansions)?;
    let target_resolved = resolve_links(target, cwd, home, links, max_expansions)?;

    let base_components = absolute_components(&base_resolved);
    let target_components = absolute_components(&target_resolved);
    let target_trailing = target_resolved.ends_with('/');

    let mut candidates = vec![route(&base_components, &target_components)];

    for link in links {
        let Ok(probe) = resolve_links(&link.path, "/", None, links, max_expansions) else {
            continue;
        };
        let probe_components = absolute_components(&probe);
        if probe_components.len() > target_components.len()
            || probe_components
                .iter()
                .zip(&target_components)
                .any(|(a, b)| a != b)
        {
            continue;
        }

        let key_components = absolute_components(&link.path);
        let mut candidate = route(&base_components, &key_components);
        let suffix = &target_components[probe_components.len()..];
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
            candidate.insert_str(0, "./");
        }
        if target_trailing && candidate != "." && !candidate.ends_with('/') {
            candidate.push('/');
        }

        if let Ok(resolved) =
            resolve_links(&candidate, &base_resolved, home, links, max_expansions)
        {
            if absolute_components(&resolved) == target_components {
                valid.push(candidate);
            }
        }
    }

    valid.sort_by(|a, b| {
        a.len()
            .cmp(&b.len())
            .then_with(|| a.as_bytes().cmp(b.as_bytes()))
    });
    valid.into_iter().next().ok_or(PathError::Escape { at: 0 })
}