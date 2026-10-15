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
    if path.as_bytes().last() == Some(&b'/') {
        return true;
    }
    path.split('/').filter(|part| !part.is_empty()).last()
        .is_some_and(|part| part == "." || part == "..")
}

fn format_absolute(parts: &[String], directory: bool) -> String {
    if parts.is_empty() {
        return "/".to_string();
    }
    let mut result = format!("/{}", parts.join("/"));
    if directory {
        result.push('/');
    }
    result
}

fn format_relative(parts: &[String], directory: bool) -> String {
    if parts.is_empty() {
        return ".".to_string();
    }
    let mut result = parts.join("/");
    if directory {
        result.push('/');
    }
    result
}

fn reduce_plain(path: &str, absolute: bool) -> Result<Vec<String>, PathError> {
    let mut parts: Vec<String> = Vec::new();
    let mut offset = 0;
    for component in path.split('/') {
        let start = offset;
        offset += component.len() + 1;
        if component.is_empty() || component == "." {
            continue;
        }
        if component == ".." {
            if parts.last().is_some_and(|p| p != "..") {
                parts.pop();
            } else if absolute {
                return Err(PathError::Escape { at: start });
            } else {
                parts.push("..".to_string());
            }
        } else {
            parts.push(component.to_string());
        }
    }
    Ok(parts)
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;
    let absolute = path.starts_with('/');
    let directory = directory_intent(path);
    let parts = reduce_plain(path, absolute)?;
    if absolute {
        Ok(format_absolute(&parts, directory))
    } else {
        Ok(format_relative(&parts, directory))
    }
}

fn normalized_context(path: &str, error: PathError) -> Result<Vec<String>, PathError> {
    if !path.starts_with('/') {
        return Err(error);
    }
    let normalized = normalize(path).map_err(|_| error)?;
    Ok(normalized
        .split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect())
}

fn expand_request<'a>(
    path: &'a str,
    home: Option<&'a [String]>,
) -> Result<(&'a str, usize, bool, bool), PathError> {
    let directory = directory_intent(path);
    if path.starts_with('~') {
        let bytes = path.as_bytes();
        if bytes.len() == 1 || bytes[1] == b'/' {
            let home = home.ok_or(PathError::MissingHome)?;
            let consumed = if bytes.len() > 1 { 2 } else { 1 };
            return Ok((&path[consumed..], consumed, false, true));
        }
        return Err(PathError::UnsupportedTilde { at: 1 });
    }
    let absolute = path.starts_with('/');
    let consumed = if absolute { 1 } else { 0 };
    Ok((&path[consumed..], consumed, absolute, false))
}

fn resolve_core(
    path: &str,
    cwd: &str,
    home: Option<&str>,
) -> Result<String, PathError> {
    let cwd_parts = normalized_context(cwd, PathError::InvalidCwd)?;
    let home_parts = match home {
        Some(h) => Some(normalized_context(h, PathError::InvalidHome)?),
        None => None,
    };
    validate_raw(path)?;
    let (remainder, _, absolute, tilde) = expand_request(path, home_parts.as_deref())?;
    let mut parts = if absolute {
        Vec::new()
    } else if tilde {
        home_parts.clone().unwrap_or_default()
    } else {
        cwd_parts
    };
    let directory = directory_intent(path);
    for component in remainder.split('/') {
        if component.is_empty() || component == "." {
            continue;
        }
        if component == ".." {
            if parts.pop().is_none() {
                return Err(PathError::Escape {
                    at: path.find("..").unwrap_or(0),
                });
            }
        } else {
            parts.push(component.to_string());
        }
    }
    Ok(format_absolute(&parts, directory))
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    resolve_core(path, cwd, home)
}

fn validate_links(links: &[Link]) -> Result<(), PathError> {
    let mut seen: Vec<(&str, usize)> = Vec::new();
    for (index, link) in links.iter().enumerate() {
        let valid_path = link.path.starts_with('/')
            && !link.path.ends_with('/')
            && link.path != "/"
            && normalize(&link.path).as_ref().is_ok_and(|n| n == &link.path);
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

fn link_index(links: &[Link], parts: &[String]) -> Option<usize> {
    let key = format_absolute(parts, false);
    links.iter().position(|link| link.path == key)
}

struct Traverser<'a> {
    links: &'a [Link],
    active: Vec<usize>,
    expansions: usize,
    max_expansions: usize,
}

impl<'a> Traverser<'a> {
    fn push_component(
        &mut self,
        parts: &mut Vec<String>,
        component: &str,
        offset: usize,
    ) -> Result<(), PathError> {
        if component.is_empty() || component == "." {
            return Ok(());
        }
        if component == ".." {
            if parts.pop().is_none() {
                return Err(PathError::Escape { at: offset });
            }
            return Ok(());
        }
        parts.push(component.to_string());
        if let Some(index) = link_index(self.links, parts) {
            if self.active.contains(&index) {
                return Err(PathError::Loop { index });
            }
            if self.expansions >= self.max_expansions {
                return Err(PathError::Limit { index });
            }
            self.expansions += 1;
            let link = &self.links[index];
            let target = link.target.as_str();
            let absolute = target.starts_with('/');
            if absolute {
                parts.clear();
            } else {
                parts.pop();
            }
            self.active.push(index);
            let mut offset = 0;
            for part in target.split('/') {
                let at = offset;
                offset += part.len() + 1;
                self.push_component(parts, part, at)?;
            }
            self.active.pop();
        }
        Ok(())
    }

    fn process(&mut self, parts: &mut Vec<String>, raw: &str, base_offset: usize) -> Result<(), PathError> {
        let mut offset = 0;
        for component in raw.split('/') {
            let at = base_offset + offset;
            offset += component.len() + 1;
            self.push_component(parts, component, at)?;
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
    let cwd_parts = normalized_context(cwd, PathError::InvalidCwd)?;
    let home_parts = match home {
        Some(h) => Some(normalized_context(h, PathError::InvalidHome)?),
        None => None,
    };
    validate_links(links)?;
    validate_raw(path)?;
    let (remainder, consumed, absolute, tilde) = expand_request(path, home_parts.as_deref())?;
    let directory = directory_intent(path);
    let mut parts = if absolute {
        Vec::new()
    } else if tilde {
        home_parts.clone().unwrap_or_default()
    } else {
        cwd_parts
    };
    let mut traverser = Traverser {
        links,
        active: Vec::new(),
        expansions: 0,
        max_expansions,
    };
    // Context components are traversed from root and have offset zero.
    if !absolute && !tilde {
        let context = parts.clone();
        parts.clear();
        for component in context {
            traverser.push_component(&mut parts, &component, 0)?;
        }
    } else if tilde {
        let context = parts.clone();
        parts.clear();
        for component in context {
            traverser.push_component(&mut parts, &component, 0)?;
        }
    }
    traverser.process(&mut parts, remainder, consumed)?;
    Ok(format_absolute(&parts, directory))
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

fn components_of_absolute(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn route(base: &[String], target: &[String]) -> String {
    let mut common = 0;
    while common < base.len()
        && common < target.len()
        && base[common] == target[common]
    {
        common += 1;
    }
    let mut parts = vec!["..".to_string(); base.len() - common];
    parts.extend_from_slice(&target[common..]);
    if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
    }
}

fn prefix_components(a: &[String], b: &[String]) -> bool {
    a.len() <= b.len() && a.iter().zip(b).all(|(x, y)| x == y)
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
    let b = components_of_absolute(&resolved_base);
    let t = components_of_absolute(&resolved_target);
    let target_directory = resolved_target.ends_with('/');

    let mut candidates = vec![route(&b, &t)];
    for link in links {
        let probe = resolve_links(&link.path, "/", None, links, max_expansions);
        let Ok(probe) = probe else {
            continue;
        };
        let a = components_of_absolute(&probe);
        if prefix_components(&a, &t) {
            let mut candidate = route(&b, &components_of_absolute(&link.path));
            let suffix = &t[a.len()..];
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
    }

    let mut valid: Vec<String> = Vec::new();
    for mut candidate in candidates {
        if candidate.starts_with('~') {
            candidate.insert_str(0, "./");
        }
        if target_directory && candidate != "." && !candidate.ends_with('/') {
            candidate.push('/');
        }
        let Ok(resolved) = resolve_links(
            &candidate,
            &resolved_base,
            home,
            links,
            max_expansions,
        ) else {
            continue;
        };
        if components_of_absolute(&resolved) == t && !valid.contains(&candidate) {
            valid.push(candidate);
        }
    }
    valid.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes())));
    valid.into_iter().next().ok_or(PathError::Empty)
}

fn main() {}