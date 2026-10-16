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
        out.push(Component {
            text: &path[start..i],
            at: start,
        });
    }
    out
}

fn directory_intent(path: &str) -> bool {
    if path.ends_with('/') {
        return true;
    }
    components(path)
        .last()
        .map(|c| c.text == "." || c.text == "..")
        .unwrap_or(false)
}

fn render(parts: &[String], absolute: bool, dir: bool) -> String {
    let mut result = if absolute {
        format!("/{}", parts.join("/"))
    } else if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
    };

    if absolute && parts.is_empty() {
        result = "/".to_string();
    } else if dir {
        result.push('/');
    }
    result
}

fn reduce(
    parts: &mut Vec<String>,
    comps: &[Component<'_>],
    absolute: bool,
) -> Result<(), PathError> {
    for c in comps {
        match c.text {
            "." => {}
            ".." => {
                if parts.last().map(|s| s.as_str()) != Some("..") && !parts.is_empty() {
                    parts.pop();
                } else if absolute {
                    return Err(PathError::Escape { at: c.at });
                } else {
                    parts.push("..".to_string());
                }
            }
            _ => parts.push(c.text.to_string()),
        }
    }
    Ok(())
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;
    let absolute = path.as_bytes()[0] == b'/';
    let dir = directory_intent(path);
    let mut parts = Vec::new();
    reduce(&mut parts, &components(path), absolute)?;
    Ok(render(&parts, absolute, dir))
}

fn validate_context(cwd: &str, home: Option<&str>) -> Result<(Vec<String>, Option<Vec<String>>), PathError> {
    let cwd_norm = normalize(cwd).map_err(|_| PathError::InvalidCwd)?;
    if !cwd.starts_with('/') {
        return Err(PathError::InvalidCwd);
    }
    let cwd_parts = components(&cwd_norm)
        .into_iter()
        .map(|c| c.text.to_string())
        .collect();

    let home_parts = match home {
        Some(h) => {
            let norm = normalize(h).map_err(|_| PathError::InvalidHome)?;
            if !h.starts_with('/') {
                return Err(PathError::InvalidHome);
            }
            Some(
                components(&norm)
                    .into_iter()
                    .map(|c| c.text.to_string())
                    .collect(),
            )
        }
        None => None,
    };
    Ok((cwd_parts, home_parts))
}

fn expand_request<'a>(path: &'a str, home: Option<&[String]>) -> Result<(bool, Vec<String>, Vec<Component<'a>>), PathError> {
    validate_raw(path)?;
    if path.starts_with('~') {
        if path == "~" || path.starts_with("~/") {
            let home = home.ok_or(PathError::MissingHome)?;
            let remainder = if path == "~" { "" } else { &path[2..] };
            return Ok((
                false,
                home.to_vec(),
                components(remainder),
            ));
        } else {
            return Err(PathError::UnsupportedTilde { at: 1 });
        }
    }
    let absolute = path.starts_with('/');
    Ok((absolute, Vec::new(), components(path)))
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let (cwd_parts, home_parts) = validate_context(cwd, home)?;
    let dir = directory_intent(path);
    let (absolute, expanded_home, req) =
        expand_request(path, home_parts.as_deref())?;

    let mut parts = if absolute {
        Vec::new()
    } else if path.starts_with('~') {
        expanded_home
    } else {
        cwd_parts
    };

    // Request offsets after a tilde are retained from the original string.
    let req = if path.starts_with("~/") {
        components_from_offset(path, 2)
    } else if path == "~" {
        Vec::new()
    } else {
        req
    };
    reduce(&mut parts, &req, true)?;
    Ok(render(&parts, true, dir))
}

fn components_from_offset(path: &str, offset: usize) -> Vec<Component<'_>> {
    let mut out = Vec::new();
    let bytes = path.as_bytes();
    let mut i = offset;
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

fn validate_links(links: &[Link]) -> Result<HashMap<String, usize>, PathError> {
    let mut map = HashMap::new();
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
        if let Some(&first) = map.get(&link.path) {
            return Err(PathError::DuplicateLink {
                first,
                second: index,
            });
        }
        map.insert(link.path.clone(), index);
    }
    Ok(map)
}

fn key_for(parts: &[String]) -> String {
    if parts.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", parts.join("/"))
    }
}

struct Walker<'a> {
    links: &'a [Link],
    keys: &'a HashMap<String, usize>,
    active: Vec<usize>,
    expansions: usize,
    max_expansions: usize,
}

impl<'a> Walker<'a> {
    fn push_component(
        &mut self,
        stack: &mut Vec<String>,
        c: Component<'_>,
    ) -> Result<(), PathError> {
        match c.text {
            "." => return Ok(()),
            ".." => {
                if stack.is_empty() {
                    return Err(PathError::Escape { at: c.at });
                }
                stack.pop();
                return Ok(());
            }
            _ => stack.push(c.text.to_string()),
        }

        let key = key_for(stack);
        if let Some(&index) = self.keys.get(&key) {
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
            let mut target_stack = if target.starts_with('/') {
                Vec::new()
            } else {
                stack.clone()
            };
            self.walk_components(&mut target_stack, &components(target))?;
            *stack = target_stack;

            self.active.pop();
        }
        Ok(())
    }

    fn walk_components(
        &mut self,
        stack: &mut Vec<String>,
        comps: &[Component<'_>],
    ) -> Result<(), PathError> {
        for c in comps {
            self.push_component(stack, *c)?;
        }
        Ok(())
    }
}

pub fn resolve_links(
    path: &str,
    cwd: &str,
    home: Option<&str>,
    links: &[Link],
    max_expansions: usize,
) -> Result<String, PathError> {
    let (cwd_parts, home_parts) = validate_context(cwd, home)?;
    let keys = validate_links(links)?;
    validate_raw(path)?;

    let dir = directory_intent(path);
    let (absolute, expanded_home, req) =
        expand_request(path, home_parts.as_deref())?;

    let mut stack = if absolute {
        Vec::new()
    } else if path.starts_with('~') {
        expanded_home
    } else {
        cwd_parts
    };

    let req = if path.starts_with("~/") {
        components_from_offset(path, 2)
    } else if path == "~" {
        Vec::new()
    } else {
        req
    };

    let mut walker = Walker {
        links,
        keys: &keys,
        active: Vec::new(),
        expansions: 0,
        max_expansions,
    };
    walker.walk_components(&mut stack, &req)?;
    Ok(render(&stack, true, dir))
}

fn result_components(path: &str) -> Vec<String> {
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
    let mut out = Vec::new();
    for _ in common..base.len() {
        out.push("..".to_string());
    }
    out.extend(target[common..].iter().cloned());
    if out.is_empty() {
        ".".to_string()
    } else {
        out.join("/")
    }
}

fn append_suffix(candidate: String, suffix: &[String]) -> String {
    if suffix.is_empty() {
        return candidate;
    }
    if candidate == "." {
        return suffix.join("/");
    }
    if candidate.is_empty() {
        suffix.join("/")
    } else {
        format!("{}/{}", candidate, suffix.join("/"))
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
    let b = result_components(&resolved_base);
    let t = result_components(&resolved_target);
    let target_dir = resolved_target.ends_with('/');

    let mut candidates = vec![route(&b, &t)];

    for link in links {
        let probe = match resolve_links(&link.path, "/", None, links, max_expansions) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let a = result_components(&probe);
        if a.len() <= t.len() && a.iter().zip(&t).all(|(x, y)| x == y) {
            let key_parts = result_components(&link.path);
            let base_route = route(&b, &key_parts);
            candidates.push(append_suffix(base_route, &t[a.len()..]));
        }
    }

    let mut valid = Vec::new();
    for mut candidate in candidates {
        if candidate.starts_with('~') {
            candidate = format!("./{}", candidate);
        }
        if target_dir && candidate != "." {
            candidate.push('/');
        }

        let resolved = match resolve_links(
            &candidate,
            &resolved_base,
            home,
            links,
            max_expansions,
        ) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if result_components(&resolved) == t {
            valid.push(candidate);
        }
    }

    valid.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes())));
    valid.into_iter().next().ok_or(PathError::Escape { at: 0 })
}