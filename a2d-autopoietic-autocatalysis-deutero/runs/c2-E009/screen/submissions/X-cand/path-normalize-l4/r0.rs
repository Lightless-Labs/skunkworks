use std::collections::{HashMap, HashSet};

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

fn components(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

fn directory_intent(path: &str) -> bool {
    path.ends_with('/')
        || path
            .split('/')
            .filter(|part| !part.is_empty())
            .last()
            .is_some_and(|part| part == "." || part == "..")
}

fn join_absolute(parts: &[String], intent: bool) -> String {
    if parts.is_empty() {
        return "/".to_owned();
    }
    let mut result = String::from("/");
    result.push_str(&parts.join("/"));
    if intent {
        result.push('/');
    }
    result
}

fn reduce_into(
    stack: &mut Vec<String>,
    raw: &str,
    offset_base: usize,
    absolute: bool,
) -> Result<(), PathError> {
    let bytes = raw.as_bytes();
    let mut start = 0usize;

    while start < bytes.len() {
        while start < bytes.len() && bytes[start] == b'/' {
            start += 1;
        }
        if start >= bytes.len() {
            break;
        }

        let end = bytes[start..]
            .iter()
            .position(|&b| b == b'/')
            .map_or(bytes.len(), |n| start + n);
        let part = &raw[start..end];

        if part == "." {
            // Nothing to do.
        } else if part == ".." {
            if stack.last().is_some_and(|last| last != "..") {
                stack.pop();
            } else if absolute {
                return Err(PathError::Escape {
                    at: offset_base + start,
                });
            } else {
                stack.push("..".to_owned());
            }
        } else {
            stack.push(part.to_owned());
        }

        start = end;
    }

    Ok(())
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;
    let absolute = path.as_bytes()[0] == b'/';
    let intent = directory_intent(path);
    let mut stack = Vec::new();
    reduce_into(&mut stack, path, 0, absolute)?;

    if absolute {
        Ok(join_absolute(&stack, intent))
    } else {
        let mut result = if stack.is_empty() {
            ".".to_owned()
        } else {
            stack.join("/")
        };
        if intent && result != "." {
            result.push('/');
        }
        Ok(result)
    }
}

fn validated_absolute(path: &str) -> bool {
    path.starts_with('/') && normalize(path).is_ok()
}

fn expand_request<'a>(
    path: &'a str,
    home: Option<&'a str>,
) -> Result<(bool, &'a str, usize, Option<&'a str>), PathError> {
    let bytes = path.as_bytes();
    if bytes.first() != Some(&b'~') {
        return Ok((bytes.first() == Some(&b'/'), path, 0, None));
    }

    if bytes.len() == 1 || bytes.get(1) == Some(&b'/') {
        let home = home.ok_or(PathError::MissingHome)?;
        let remainder_start = if bytes.len() > 1 { 2 } else { 1 };
        return Ok((true, &path[remainder_start..], remainder_start, Some(home)));
    }

    Err(PathError::UnsupportedTilde { at: 1 })
}

fn resolve_core(
    path: &str,
    cwd: &str,
    home: Option<&str>,
) -> Result<String, PathError> {
    if !validated_absolute(cwd) {
        return Err(PathError::InvalidCwd);
    }
    if let Some(home_path) = home {
        if !validated_absolute(home_path) {
            return Err(PathError::InvalidHome);
        }
    }

    validate_raw(path)?;
    let intent = directory_intent(path);
    let (absolute, request, offset, expansion_home) = expand_request(path, home)?;

    let mut stack = if absolute {
        if expansion_home.is_some() {
            components(expansion_home.unwrap())
        } else if path.starts_with('/') {
            Vec::new()
        } else {
            components(cwd)
        }
    } else {
        components(cwd)
    };

    reduce_into(&mut stack, request, offset, true)?;
    Ok(join_absolute(&stack, intent))
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    resolve_core(path, cwd, home)
}

fn validate_links(links: &[Link]) -> Result<HashMap<String, usize>, PathError> {
    let mut keys = HashMap::new();

    for (index, link) in links.iter().enumerate() {
        let valid_path = link.path.starts_with('/')
            && !link.path.ends_with('/')
            && normalize(&link.path).is_ok_and(|normalized| normalized == link.path)
            && link.path != "/";
        let valid_target = validate_raw(&link.target).is_ok();

        if !valid_path || !valid_target {
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

fn absolute_stack(path: &str) -> Vec<String> {
    components(path)
}

fn stack_path(stack: &[String]) -> String {
    if stack.is_empty() {
        "/".to_owned()
    } else {
        format!("/{}", stack.join("/"))
    }
}

struct Traversal<'a> {
    links: &'a [Link],
    keys: &'a HashMap<String, usize>,
    max_expansions: usize,
    expansions: usize,
    active: HashSet<usize>,
    stack: Vec<String>,
}

impl<'a> Traversal<'a> {
    fn process(
        &mut self,
        raw: &str,
        offset_base: usize,
        absolute: bool,
    ) -> Result<(), PathError> {
        validate_raw(raw)?;
        let bytes = raw.as_bytes();
        let mut start = 0usize;

        while start < bytes.len() {
            while start < bytes.len() && bytes[start] == b'/' {
                start += 1;
            }
            if start >= bytes.len() {
                break;
            }
            let end = bytes[start..]
                .iter()
                .position(|&b| b == b'/')
                .map_or(bytes.len(), |n| start + n);
            let part = &raw[start..end];

            if part == "." {
                // Dot components do not trigger lookups.
            } else if part == ".." {
                if self.stack.last().is_some_and(|last| last != "..") {
                    self.stack.pop();
                } else if absolute {
                    return Err(PathError::Escape {
                        at: offset_base + start,
                    });
                } else {
                    self.stack.push("..".to_owned());
                }
            } else {
                self.stack.push(part.to_owned());
                let current = stack_path(&self.stack);
                if let Some(&index) = self.keys.get(&current) {
                    if self.active.contains(&index) {
                        return Err(PathError::Loop { index });
                    }
                    if self.expansions >= self.max_expansions {
                        return Err(PathError::Limit { index });
                    }
                    self.expansions += 1;
                    self.active.insert(index);

                    let target = &self.links[index].target;
                    let target_absolute = target.starts_with('/');
                    if target_absolute {
                        self.stack.clear();
                    } else {
                        self.stack.pop();
                    }
                    let target_result = self.process(target, 0, true);
                    self.active.remove(&index);
                    target_result?;
                }
            }
            start = end;
        }
        Ok(())
    }
}

fn resolve_links_core(
    path: &str,
    cwd: &str,
    home: Option<&str>,
    links: &[Link],
    max_expansions: usize,
) -> Result<String, PathError> {
    if !validated_absolute(cwd) {
        return Err(PathError::InvalidCwd);
    }
    if let Some(home_path) = home {
        if !validated_absolute(home_path) {
            return Err(PathError::InvalidHome);
        }
    }
    let keys = validate_links(links)?;
    validate_raw(path)?;

    let intent = directory_intent(path);
    let (absolute, request, offset, expansion_home) = expand_request(path, home)?;

    let start_path = if expansion_home.is_some() {
        expansion_home.unwrap()
    } else if absolute {
        "/"
    } else {
        cwd
    };
    let mut traversal = Traversal {
        links,
        keys: &keys,
        max_expansions,
        expansions: 0,
        active: HashSet::new(),
        stack: absolute_stack(start_path),
    };

    traversal.process(request, offset, true)?;
    Ok(join_absolute(&traversal.stack, intent))
}

pub fn resolve_links(
    path: &str,
    cwd: &str,
    home: Option<&str>,
    links: &[Link],
    max_expansions: usize,
) -> Result<String, PathError> {
    resolve_links_core(path, cwd, home, links, max_expansions)
}

fn relative_route(base: &[String], target: &[String]) -> String {
    let mut common = 0usize;
    while common < base.len()
        && common < target.len()
        && base[common] == target[common]
    {
        common += 1;
    }

    let mut parts = Vec::new();
    for _ in common..base.len() {
        parts.push("..".to_owned());
    }
    parts.extend(target[common..].iter().cloned());

    if parts.is_empty() {
        ".".to_owned()
    } else {
        parts.join("/")
    }
}

fn components_absolute(path: &str) -> Vec<String> {
    components(path)
}

pub fn relative_to(
    base: &str,
    target: &str,
    cwd: &str,
    home: Option<&str>,
    links: &[Link],
    max_expansions: usize,
) -> Result<String, PathError> {
    let resolved_base = resolve_links_core(base, cwd, home, links, max_expansions)?;
    let resolved_target = resolve_links_core(target, cwd, home, links, max_expansions)?;
    let base_parts = components_absolute(&resolved_base);
    let target_parts = components_absolute(&resolved_target);
    let target_directory = resolved_target.ends_with('/');

    let mut candidates = Vec::new();
    candidates.push(relative_route(&base_parts, &target_parts));

    for link in links {
        let probe = resolve_links_core(&link.path, "/", None, links, max_expansions);
        let Ok(probe) = probe else {
            continue;
        };
        let alias_parts = components_absolute(&probe);

        if alias_parts.len() > target_parts.len()
            || alias_parts
                .iter()
                .zip(target_parts.iter())
                .any(|(a, t)| a != t)
        {
            continue;
        }

        let mut candidate = relative_route(&base_parts, &components_absolute(&link.path));
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
        if target_directory && candidate != "." && !candidate.ends_with('/') {
            candidate.push('/');
        }

        let Ok(resolved) = resolve_links_core(
            &candidate,
            &resolved_base,
            home,
            links,
            max_expansions,
        ) else {
            continue;
        };
        if components_absolute(&resolved) == target_parts {
            valid.push(candidate);
        }
    }

    valid.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes())));
    valid.dedup();

    valid.into_iter().next().ok_or(PathError::Empty)
}