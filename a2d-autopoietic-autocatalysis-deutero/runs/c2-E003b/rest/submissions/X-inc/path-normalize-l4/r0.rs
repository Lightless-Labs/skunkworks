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

fn directory_intent(path: &str) -> bool {
    if path.as_bytes().last() == Some(&b'/') {
        return true;
    }
    path.rsplit('/')
        .find(|component| !component.is_empty())
        .is_some_and(|component| component == "." || component == "..")
}

fn raw_components(path: &str, base_offset: usize) -> Vec<(String, usize)> {
    let bytes = path.as_bytes();
    let mut components = Vec::new();
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
        components.push((path[start..i].to_owned(), base_offset + start));
    }
    components
}

fn render(absolute: bool, components: &[String], dir: bool) -> String {
    let mut result = if absolute {
        format!("/{}", components.join("/"))
    } else if components.is_empty() {
        ".".to_owned()
    } else {
        components.join("/")
    };

    if absolute && components.is_empty() {
        return "/".to_owned();
    }
    if dir && result != "/" && result != "." {
        result.push('/');
    }
    result
}

fn reduce(
    stack: &mut Vec<String>,
    components: Vec<(String, usize)>,
    absolute: bool,
) -> Result<(), PathError> {
    for (component, at) in components {
        match component.as_str() {
            "." => {}
            ".." => {
                if stack.last().is_some_and(|last| last != "..") {
                    stack.pop();
                } else if absolute {
                    return Err(PathError::Escape { at });
                } else {
                    stack.push(component);
                }
            }
            _ => stack.push(component),
        }
    }
    Ok(())
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;
    let absolute = path.starts_with('/');
    let mut components = Vec::new();
    reduce(&mut components, raw_components(path, 0), absolute)?;
    Ok(render(absolute, &components, directory_intent(path)))
}

fn validate_context(path: &str) -> Result<Vec<String>, PathError> {
    if !path.starts_with('/') {
        return Err(PathError::InvalidCwd);
    }
    let normalized = normalize(path).map_err(|_| PathError::InvalidCwd)?;
    Ok(raw_components(&normalized, 0)
        .into_iter()
        .map(|(component, _)| component)
        .collect())
}

fn validate_home(home: Option<&str>) -> Result<Option<Vec<String>>, PathError> {
    let Some(home) = home else {
        return Ok(None);
    };
    if !home.starts_with('/') {
        return Err(PathError::InvalidHome);
    }
    let normalized = normalize(home).map_err(|_| PathError::InvalidHome)?;
    Ok(Some(
        raw_components(&normalized, 0)
            .into_iter()
            .map(|(component, _)| component)
            .collect(),
    ))
}

fn expand_tilde<'a>(
    path: &'a str,
    home: Option<&[String]>,
) -> Result<(bool, Vec<String>, &'a str, usize), PathError> {
    if !path.starts_with('~') {
        return Ok((path.starts_with('/'), Vec::new(), path, 0));
    }
    let bytes = path.as_bytes();
    if bytes.len() > 1 && bytes[1] != b'/' {
        return Err(PathError::UnsupportedTilde { at: 1 });
    }
    let Some(home) = home else {
        return Err(PathError::MissingHome);
    };
    let skip = if bytes.len() > 1 { 2 } else { 1 };
    Ok((true, home.to_vec(), &path[skip..], skip))
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_components = validate_context(cwd)?;
    let home_components = validate_home(home)?;
    validate_raw(path)?;

    let intent = directory_intent(path);
    let (absolute, mut stack, remainder, offset) =
        expand_tilde(path, home_components.as_deref())?;
    if !absolute {
        stack = cwd_components;
    }
    reduce(&mut stack, raw_components(remainder, offset), true)?;
    Ok(render(true, &stack, intent))
}

fn validate_links(links: &[Link]) -> Result<HashMap<String, usize>, PathError> {
    let mut indices = HashMap::new();
    for (index, link) in links.iter().enumerate() {
        let path_valid = link.path.starts_with('/')
            && !link.path.ends_with('/')
            && link.path != "/"
            && normalize(&link.path)
                .is_ok_and(|normalized| normalized == link.path);
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

struct Walker<'a> {
    links: &'a [Link],
    keys: &'a HashMap<String, usize>,
    active: Vec<usize>,
    expansions: usize,
    max_expansions: usize,
}

impl<'a> Walker<'a> {
    fn absolute_name(stack: &[String]) -> String {
        format!("/{}", stack.join("/"))
    }

    fn process(
        &mut self,
        stack: &mut Vec<String>,
        components: Vec<(String, usize)>,
        absolute: bool,
    ) -> Result<(), PathError> {
        for (component, at) in components {
            match component.as_str() {
                "." => continue,
                ".." => {
                    if stack.last().is_some_and(|last| last != "..") {
                        stack.pop();
                    } else if absolute {
                        return Err(PathError::Escape { at });
                    } else {
                        stack.push(component);
                    }
                }
                _ => {
                    stack.push(component);
                    let name = Self::absolute_name(stack);
                    if let Some(&index) = self.keys.get(&name) {
                        self.expand(index, stack)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn expand(&mut self, index: usize, stack: &mut Vec<String>) -> Result<(), PathError> {
        if self.active.contains(&index) {
            return Err(PathError::Loop { index });
        }
        if self.expansions >= self.max_expansions {
            return Err(PathError::Limit { index });
        }
        self.expansions += 1;
        self.active.push(index);

        let link = &self.links[index];
        let target = link.target.clone();
        let absolute = target.starts_with('/');
        if absolute {
            stack.clear();
        } else {
            stack.pop();
        }
        let result = self.process(stack, raw_components(&target, 0), absolute || true);
        self.active.pop();
        result
    }
}

fn resolve_links_internal(
    path: &str,
    cwd_components: &[String],
    home_components: Option<&[String]>,
    links: &[Link],
    keys: &HashMap<String, usize>,
    max_expansions: usize,
) -> Result<String, PathError> {
    validate_raw(path)?;
    let intent = directory_intent(path);
    let (is_absolute, mut stack, remainder, offset) = expand_tilde(path, home_components)?;
    if is_absolute && !path.starts_with('~') {
        stack.clear();
    } else if !is_absolute {
        stack = cwd_components.to_vec();
    }

    let mut walker = Walker {
        links,
        keys,
        active: Vec::new(),
        expansions: 0,
        max_expansions,
    };
    walker.process(&mut stack, raw_components(remainder, offset), true)?;
    Ok(render(true, &stack, intent))
}

pub fn resolve_links(
    path: &str,
    cwd: &str,
    home: Option<&str>,
    links: &[Link],
    max_expansions: usize,
) -> Result<String, PathError> {
    let cwd_components = validate_context(cwd)?;
    let home_components = validate_home(home)?;
    let keys = validate_links(links)?;
    resolve_links_internal(
        path,
        &cwd_components,
        home_components.as_deref(),
        links,
        &keys,
        max_expansions,
    )
}

fn components_of_absolute(path: &str) -> Vec<String> {
    raw_components(path, 0)
        .into_iter()
        .map(|(component, _)| component)
        .collect()
}

fn route(base: &[String], target: &[String]) -> String {
    let common = base
        .iter()
        .zip(target.iter())
        .take_while(|(left, right)| left == right)
        .count();
    let mut result = vec!["..".to_owned(); base.len() - common];
    result.extend(target[common..].iter().cloned());
    if result.is_empty() {
        ".".to_owned()
    } else {
        result.join("/")
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
    let base_components = components_of_absolute(&resolved_base);
    let target_components = components_of_absolute(&resolved_target);
    let keys = validate_links(links)?;

    let mut candidates = vec![route(&base_components, &target_components)];
    for link in links {
        let Ok(probe) = resolve_links_internal(
            &link.path,
            &[],
            None,
            links,
            &keys,
            max_expansions,
        ) else {
            continue;
        };
        let alias_components = components_of_absolute(&probe);
        if alias_components.len() <= target_components.len()
            && alias_components
                .iter()
                .zip(target_components.iter())
                .all(|(a, b)| a == b)
        {
            let mut candidate = route(&base_components, &components_of_absolute(&link.path));
            let suffix = &target_components[alias_components.len()..];
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

    let target_is_dir = resolved_target.ends_with('/');
    let mut valid = Vec::new();
    for mut candidate in candidates {
        if candidate.starts_with('~') {
            candidate = format!("./{candidate}");
        }
        if target_is_dir && candidate != "." && !candidate.ends_with('/') {
            candidate.push('/');
        }
        let Ok(result) = resolve_links(
            &candidate,
            &resolved_base,
            home,
            links,
            max_expansions,
        ) else {
            continue;
        };
        if components_of_absolute(&result) == target_components {
            valid.push(candidate);
        }
    }

    valid.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes())));
    valid.into_iter().next().ok_or(PathError::Empty)
}