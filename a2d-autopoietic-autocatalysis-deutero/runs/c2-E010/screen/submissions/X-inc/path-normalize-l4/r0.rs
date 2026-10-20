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
    let bytes = path.as_bytes();

    if bytes.is_empty() {
        return Err(PathError::Empty);
    }
    if let Some(at) = bytes.iter().position(|&b| b == 0) {
        return Err(PathError::Nul { at });
    }
    if let Some(at) = bytes.iter().position(|&b| b == b'\\') {
        return Err(PathError::Backslash { at });
    }
    if let Some(at) = bytes.iter().position(|&b| (1..=31).contains(&b) || b == 127) {
        return Err(PathError::Control { at });
    }
    Ok(())
}

fn components_with_offsets(path: &str) -> Vec<(&str, usize)> {
    path.split('/')
        .scan(0usize, |offset, part| {
            let start = *offset;
            *offset += part.len() + 1;
            Some((part, start))
        })
        .filter(|(part, _)| !part.is_empty())
        .collect()
}

fn directory_intent(path: &str) -> bool {
    path.ends_with('/')
        || components_with_offsets(path)
            .last()
            .map(|(part, _)| *part == "." || *part == "..")
            .unwrap_or(false)
}

fn reduce_components(
    stack: &mut Vec<String>,
    components: &[(&str, usize)],
    absolute: bool,
) -> Result<(), PathError> {
    for &(component, at) in components {
        match component {
            "." => {}
            ".." => {
                if stack.last().map(|s| s.as_str()) != Some("..")
                    && !stack.is_empty()
                {
                    stack.pop();
                } else if absolute {
                    return Err(PathError::Escape { at });
                } else {
                    stack.push("..".to_owned());
                }
            }
            name => stack.push(name.to_owned()),
        }
    }
    Ok(())
}

fn render(stack: &[String], absolute: bool, intent: bool) -> String {
    let mut result = if absolute {
        format!("/{}", stack.join("/"))
    } else if stack.is_empty() {
        ".".to_owned()
    } else {
        stack.join("/")
    };

    if absolute && stack.is_empty() {
        return "/".to_owned();
    }
    if !absolute && stack.is_empty() {
        return ".".to_owned();
    }
    if intent {
        result.push('/');
    }
    result
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;
    let absolute = path.as_bytes().first() == Some(&b'/');
    let intent = directory_intent(path);
    let components = components_with_offsets(path);
    let mut stack = Vec::new();
    reduce_components(&mut stack, &components, absolute)?;
    Ok(render(&stack, absolute, intent))
}

fn validate_context(value: &str, cwd: bool) -> Result<Vec<String>, PathError> {
    let normalized = normalize(value).map_err(|_| {
        if cwd {
            PathError::InvalidCwd
        } else {
            PathError::InvalidHome
        }
    })?;
    if !value.starts_with('/') {
        return Err(if cwd {
            PathError::InvalidCwd
        } else {
            PathError::InvalidHome
        });
    }
    Ok(components_with_offsets(&normalized)
        .into_iter()
        .map(|(part, _)| part.to_owned())
        .collect())
}

fn request_parts<'a>(path: &'a str) -> Result<(bool, bool, Vec<(&'a str, usize)>, Option<Vec<String>>), PathError> {
    validate_raw(path)?;

    let bytes = path.as_bytes();
    let intent = directory_intent(path);
    if bytes[0] == b'~' {
        if bytes.len() > 1 && bytes[1] != b'/' {
            return Err(PathError::UnsupportedTilde { at: 1 });
        }
        let remainder_start = if bytes.len() > 1 { 2 } else { 1 };
        let remainder = &path[remainder_start..];
        let parts = components_with_offsets(remainder)
            .into_iter()
            .map(|(part, at)| (part, at + remainder_start))
            .collect();
        return Ok((false, intent, parts, Some(Vec::new())));
    }

    let absolute = bytes[0] == b'/';
    Ok((absolute, intent, components_with_offsets(path), None))
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_components = validate_context(cwd, true)?;
    let home_components = match home {
        Some(value) => Some(validate_context(value, false)?),
        None => None,
    };

    let (absolute, intent, parts, tilde) = request_parts(path)?;
    let mut stack = if absolute {
        Vec::new()
    } else if tilde.is_some() {
        home_components.ok_or(PathError::MissingHome)?
    } else {
        cwd_components
    };
    reduce_components(&mut stack, &parts, true)?;
    Ok(render(&stack, true, intent))
}

fn validate_links(links: &[Link]) -> Result<HashMap<String, usize>, PathError> {
    let mut indices = HashMap::new();

    for (index, link) in links.iter().enumerate() {
        let path_valid = link.path.starts_with('/')
            && !link.path.ends_with('/')
            && link.path != "/"
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

    Ok(indices)
}

struct Traversal<'a> {
    links: &'a [Link],
    indices: &'a HashMap<String, usize>,
    active: Vec<usize>,
    expansions: usize,
    max_expansions: usize,
}

impl Traversal<'_> {
    fn process(
        &mut self,
        stack: &mut Vec<String>,
        components: &[(&str, usize)],
        absolute: bool,
    ) -> Result<(), PathError> {
        for &(component, at) in components {
            match component {
                "." => continue,
                ".." => {
                    if stack.is_empty() {
                        if absolute {
                            return Err(PathError::Escape { at });
                        }
                        stack.push("..".to_owned());
                    } else if stack.last().map(|s| s.as_str()) == Some("..") {
                        if absolute {
                            return Err(PathError::Escape { at });
                        }
                        stack.push("..".to_owned());
                    } else {
                        stack.pop();
                    }
                }
                name => {
                    stack.push(name.to_owned());
                    let key = format!("/{}", stack.join("/"));
                    if let Some(&index) = self.indices.get(&key) {
                        if self.active.contains(&index) {
                            return Err(PathError::Loop { index });
                        }
                        if self.expansions >= self.max_expansions {
                            return Err(PathError::Limit { index });
                        }
                        self.expansions += 1;
                        self.active.push(index);

                        stack.pop();
                        let target = &self.links[index].target;
                        let target_absolute = target.starts_with('/');
                        if target_absolute {
                            stack.clear();
                        }
                        let target_parts = components_with_offsets(target);
                        self.process(stack, &target_parts, true)?;

                        self.active.pop();
                    }
                }
            }
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
) -> Result<String, PathError> {
    let cwd_components = validate_context(cwd, true)?;
    let home_components = match home {
        Some(value) => Some(validate_context(value, false)?),
        None => None,
    };
    let indices = validate_links(links)?;
    let (absolute, intent, parts, tilde) = request_parts(path)?;

    let mut stack = Vec::new();
    let mut traversal = Traversal {
        links,
        indices: &indices,
        active: Vec::new(),
        expansions: 0,
        max_expansions,
    };

    if absolute {
        traversal.process(&mut stack, &[], true)?;
    } else if tilde.is_some() {
        stack = home_components.ok_or(PathError::MissingHome)?;
        let context_parts: Vec<(&str, usize)> =
            stack.iter().map(|part| (part.as_str(), 0)).collect();
        traversal.process(&mut Vec::new(), &[], true)?;
        let mut rebuilt = Vec::new();
        traversal.process(&mut rebuilt, &context_parts, true)?;
        stack = rebuilt;
    } else {
        let context_parts: Vec<(&str, usize)> =
            cwd_components.iter().map(|part| (part.as_str(), 0)).collect();
        traversal.process(&mut stack, &context_parts, true)?;
    }

    traversal.process(&mut stack, &parts, true)?;
    Ok(render(&stack, true, intent))
}

pub fn resolve_links(
    path: &str,
    cwd: &str,
    home: Option<&str>,
    links: &[Link],
    max_expansions: usize,
) -> Result<String, PathError> {
    resolve_links_internal(path, cwd, home, links, max_expansions)
}

fn absolute_components(path: &str) -> Vec<String> {
    components_with_offsets(path)
        .into_iter()
        .map(|(part, _)| part.to_owned())
        .collect()
}

fn route(base: &[String], target: &[String]) -> String {
    let common = base
        .iter()
        .zip(target.iter())
        .take_while(|(a, b)| a == b)
        .count();

    let mut parts = Vec::new();
    parts.extend((common..base.len()).map(|_| "..".to_owned()));
    parts.extend(target[common..].iter().cloned());

    if parts.is_empty() {
        ".".to_owned()
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
        let probe = resolve_links_internal(&link.path, "/", None, links, max_expansions);
        let Ok(probe) = probe else {
            continue;
        };
        let probe_components = absolute_components(&probe);
        if probe_components.len() > target_components.len()
            || probe_components
                .iter()
                .zip(target_components.iter())
                .any(|(a, b)| a != b)
        {
            continue;
        }

        let mut candidate = route(&base_components, &absolute_components(&link.path));
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

        if target_is_directory && candidate != "." {
            candidate.push('/');
        }

        if let Ok(resolved) =
            resolve_links_internal(&candidate, &resolved_base, home, links, max_expansions)
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
    valid
        .into_iter()
        .next()
        .ok_or(PathError::Escape { at: 0 })
}