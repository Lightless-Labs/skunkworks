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

#[derive(Clone, Copy)]
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
        result.push(Component {
            text: &path[start..i],
            at: start,
        });
    }

    result
}

fn directory_intent(path: &str) -> bool {
    if path.as_bytes().last() == Some(&b'/') {
        return true;
    }
    components(path)
        .last()
        .map(|c| c.text == "." || c.text == "..")
        .unwrap_or(false)
}

fn render_absolute(stack: &[String], intent: bool) -> String {
    let mut result = String::from("/");
    result.push_str(&stack.join("/"));
    if intent && !stack.is_empty() {
        result.push('/');
    }
    result
}

fn render_normalized(stack: &[String], absolute: bool, intent: bool) -> String {
    if absolute {
        return render_absolute(stack, intent);
    }

    if stack.is_empty() {
        return ".".to_string();
    }

    let mut result = stack.join("/");
    if intent {
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
    match component {
        "." => {}
        ".." => {
            if stack.last().map(String::as_str).is_some_and(|s| s != "..") {
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
    let absolute = path.as_bytes().first() == Some(&b'/');
    let intent = directory_intent(path);
    let mut stack = Vec::new();

    for component in components(path) {
        reduce_component(&mut stack, component.text, absolute, component.at)?;
    }

    Ok(render_normalized(&stack, absolute, intent))
}

fn validated_context(path: &str, error: PathError) -> Result<Vec<String>, PathError> {
    if path.as_bytes().first() != Some(&b'/') {
        return Err(error);
    }
    match normalize(path) {
        Ok(value) => Ok(components(&value)
            .into_iter()
            .map(|c| c.text.to_string())
            .collect()),
        Err(_) => Err(error),
    }
}

#[derive(Clone, Copy)]
enum Source<'a> {
    Request(&'a str),
    Target(&'a str),
}

impl<'a> Source<'a> {
    fn path(self) -> &'a str {
        match self {
            Source::Request(path) | Source::Target(path) => path,
        }
    }

    fn request(self) -> bool {
        matches!(self, Source::Request(_))
    }
}

fn resolve_internal(
    path: &str,
    cwd: &str,
    home: Option<&str>,
    links: &[Link],
    max_expansions: usize,
) -> Result<String, PathError> {
    let cwd_stack = validated_context(cwd, PathError::InvalidCwd)?;
    let home_stack = match home {
        Some(h) => Some(validated_context(h, PathError::InvalidHome)?),
        None => None,
    };

    let mut lookup = HashMap::<&str, usize>::new();
    for (index, link) in links.iter().enumerate() {
        let valid_path = link.path.starts_with('/')
            && !link.path.ends_with('/')
            && normalize(&link.path)
                .map(|n| n == link.path && link.path != "/")
                .unwrap_or(false);
        let valid_target = validate_raw(&link.target).is_ok();

        if !valid_path || !valid_target {
            return Err(PathError::InvalidLink { index });
        }
        if let Some(&first) = lookup.get(link.path.as_str()) {
            return Err(PathError::DuplicateLink {
                first,
                second: index,
            });
        }
        lookup.insert(link.path.as_str(), index);
    }

    validate_raw(path)?;

    let mut request_start = 0;
    let mut stack;
    let bytes = path.as_bytes();

    if bytes.first() == Some(&b'~') {
        if bytes.len() == 1 || bytes.get(1) == Some(&b'/') {
            stack = home_stack.ok_or(PathError::MissingHome)?;
            request_start = if bytes.len() > 1 { 2 } else { 1 };
        } else {
            return Err(PathError::UnsupportedTilde { at: 1 });
        }
    } else if bytes.first() == Some(&b'/') {
        stack = Vec::new();
    } else {
        stack = cwd_stack;
    }

    let intent = directory_intent(path);
    let request_components = components(&path[request_start..])
        .into_iter()
        .map(|c| Component {
            text: c.text,
            at: c.at + request_start,
        })
        .collect::<Vec<_>>();

    let mut frames = vec![(Source::Request(path), request_components, 0usize, None)];
    let mut active = Vec::<usize>::new();
    let mut expansions = 0usize;

    while !frames.is_empty() {
        let last = frames.len() - 1;
        if frames[last].2 >= frames[last].1.len() {
            let (_, _, _, completed) = frames.pop().unwrap();
            if let Some(index) = completed {
                if active.last() == Some(&index) {
                    active.pop();
                } else if let Some(position) = active.iter().rposition(|&x| x == index) {
                    active.remove(position);
                }
            }
            continue;
        }

        let component = frames[last].1[frames[last].2];
        frames[last].2 += 1;
        let source = frames[last].0;

        match component.text {
            "." => {}
            ".." => {
                if stack.last().is_some_and(|s| s != "..") {
                    stack.pop();
                } else {
                    return Err(PathError::Escape { at: component.at });
                }
            }
            name => {
                stack.push(name.to_string());

                let key = {
                    let mut value = String::from("/");
                    value.push_str(&stack.join("/"));
                    value
                };

                if let Some(&index) = lookup.get(key.as_str()) {
                    if active.contains(&index) {
                        return Err(PathError::Loop { index });
                    }
                    if expansions >= max_expansions {
                        return Err(PathError::Limit { index });
                    }
                    expansions += 1;
                    active.push(index);

                    let target = &links[index].target;
                    let target_components = components(target);
                    let target_absolute = target.starts_with('/');
                    if target_absolute {
                        stack.clear();
                    } else {
                        stack.pop();
                    }

                    frames.push((
                        Source::Target(target),
                        target_components,
                        0,
                        Some(index),
                    ));
                }
            }
        }

        let _ = source;
    }

    Ok(render_absolute(&stack, intent))
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    resolve_internal(path, cwd, home, &[], 0)
}

pub fn resolve_links(
    path: &str,
    cwd: &str,
    home: Option<&str>,
    links: &[Link],
    max_expansions: usize,
) -> Result<String, PathError> {
    resolve_internal(path, cwd, home, links, max_expansions)
}

fn absolute_components(path: &str) -> Vec<String> {
    components(path)
        .into_iter()
        .map(|c| c.text.to_string())
        .collect()
}

fn route(base: &[String], target: &[String]) -> String {
    let common = base
        .iter()
        .zip(target.iter())
        .take_while(|(a, b)| a == b)
        .count();

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
    let resolved_base = resolve_links(base, cwd, home, links, max_expansions)?;
    let resolved_target = resolve_links(target, cwd, home, links, max_expansions)?;
    let base_components = absolute_components(&resolved_base);
    let target_components = absolute_components(&resolved_target);
    let target_is_directory = resolved_target.ends_with('/');

    let mut candidates = vec![route(&base_components, &target_components)];

    for link in links {
        let probe = resolve_links(
            &link.path,
            "/",
            None,
            links,
            max_expansions,
        );
        let Ok(probe) = probe else {
            continue;
        };

        let prefix = absolute_components(&probe);
        if prefix.len() > target_components.len()
            || prefix
                .iter()
                .zip(target_components.iter())
                .any(|(a, b)| a != b)
        {
            continue;
        }

        let mut candidate = route(&base_components, &absolute_components(&link.path));
        let suffix = &target_components[prefix.len()..];
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

    let mut valid = Vec::<String>::new();
    for mut candidate in candidates {
        if candidate.starts_with('~') {
            candidate.insert_str(0, "./");
        }
        if target_is_directory && candidate != "." && !candidate.ends_with('/') {
            candidate.push('/');
        }

        if let Ok(result) = resolve_links(
            &candidate,
            &resolved_base,
            home,
            links,
            max_expansions,
        ) {
            if absolute_components(&result) == target_components {
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

fn main() {}