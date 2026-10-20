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

fn components(path: &str) -> Vec<(String, usize)> {
    let mut result = Vec::new();
    let bytes = path.as_bytes();
    let mut start = 0;
    while start < bytes.len() {
        if bytes[start] == b'/' {
            start += 1;
            continue;
        }
        let mut end = start;
        while end < bytes.len() && bytes[end] != b'/' {
            end += 1;
        }
        result.push((path[start..end].to_owned(), start));
        start = end;
    }
    result
}

fn render_relative(stack: &[String]) -> String {
    if stack.is_empty() {
        ".".to_owned()
    } else {
        stack.join("/")
    }
}

fn render_absolute(stack: &[String], intent: bool) -> String {
    if stack.is_empty() {
        return "/".to_owned();
    }
    let mut result = format!("/{}", stack.join("/"));
    if intent {
        result.push('/');
    }
    result
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;
    let absolute = path.as_bytes()[0] == b'/';
    let intent = directory_intent(path);
    let mut stack: Vec<String> = Vec::new();

    for (component, at) in components(path) {
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

    if absolute {
        Ok(render_absolute(&stack, intent))
    } else {
        let mut result = render_relative(&stack);
        if intent && result != "." {
            result.push('/');
        }
        Ok(result)
    }
}

fn validate_context(path: &str, invalid: PathError) -> Result<Vec<String>, PathError> {
    if !path.starts_with('/') {
        return Err(invalid);
    }
    let normalized = normalize(path).map_err(|_| invalid)?;
    Ok(components(&normalized)
        .into_iter()
        .map(|(component, _)| component)
        .collect())
}

fn request_start<'a>(
    path: &'a str,
    home: Option<&'a str>,
) -> Result<(bool, usize, Option<&'a str>), PathError> {
    if path.starts_with('/') {
        return Ok((true, 0, None));
    }
    if path.starts_with('~') {
        if path.len() == 1 || path.as_bytes().get(1) == Some(&b'/') {
            let home = home.ok_or(PathError::MissingHome)?;
            let consumed = if path.len() > 1 { 2 } else { 1 };
            return Ok((false, consumed, Some(home)));
        }
        return Err(PathError::UnsupportedTilde { at: 1 });
    }
    Ok((false, 0, None))
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_components = validate_context(cwd, PathError::InvalidCwd)?;
    let home_components = match home {
        Some(home) => Some(validate_context(home, PathError::InvalidHome)?),
        None => None,
    };

    validate_raw(path)?;
    let (absolute, consumed, _) = request_start(path, home)?;
    let intent = directory_intent(path);

    let mut stack = if absolute {
        Vec::new()
    } else if path.starts_with('~') {
        home_components.unwrap_or_default()
    } else {
        cwd_components
    };

    let request = if absolute { path } else { &path[consumed..] };
    for (component, at) in components(request) {
        let original_at = if absolute { at } else { at + consumed };
        match component.as_str() {
            "." => {}
            ".." => {
                if stack.pop().is_none() {
                    return Err(PathError::Escape { at: original_at });
                }
            }
            _ => stack.push(component),
        }
    }
    Ok(render_absolute(&stack, intent))
}

fn validate_links(links: &[Link]) -> Result<(), PathError> {
    let mut seen: Vec<(&str, usize)> = Vec::new();
    for (index, link) in links.iter().enumerate() {
        let valid_path = link.path.starts_with('/')
            && !link.path.ends_with('/')
            && normalize(&link.path).is_ok_and(|normalized| normalized == link.path)
            && link.path != "/";
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

struct LinkTraversal<'a> {
    links: &'a [Link],
    max_expansions: usize,
    expansions: usize,
    active: Vec<usize>,
}

impl LinkTraversal<'_> {
    fn push_component(
        &mut self,
        stack: &mut Vec<String>,
        component: String,
        at: usize,
    ) -> Result<(), PathError> {
        stack.push(component);
        let current = format!("/{}", stack.join("/"));
        let Some(index) = self.links.iter().position(|link| link.path == current) else {
            return Ok(());
        };

        if self.active.contains(&index) {
            return Err(PathError::Loop { index });
        }
        if self.expansions >= self.max_expansions {
            return Err(PathError::Limit { index });
        }
        self.expansions += 1;
        self.active.push(index);

        // The pushed name is the matched link. Relative targets begin at its parent.
        stack.pop();
        let target = &self.links[index].target;
        if target.starts_with('/') {
            stack.clear();
        }
        self.traverse_raw(stack, target, 0)?;

        self.active.pop();
        let _ = at;
        Ok(())
    }

    fn traverse_raw(
        &mut self,
        stack: &mut Vec<String>,
        raw: &str,
        offset_base: usize,
    ) -> Result<(), PathError> {
        for (component, local_at) in components(raw) {
            let at = local_at + offset_base;
            match component.as_str() {
                "." => {}
                ".." => {
                    if stack.pop().is_none() {
                        return Err(PathError::Escape { at });
                    }
                }
                _ => self.push_component(stack, component, at)?,
            }
        }
        Ok(())
    }
}

fn resolve_links_impl(
    path: &str,
    cwd: &str,
    home: Option<&str>,
    links: &[Link],
    max_expansions: usize,
) -> Result<String, PathError> {
    let cwd_components = validate_context(cwd, PathError::InvalidCwd)?;
    let home_components = match home {
        Some(home) => Some(validate_context(home, PathError::InvalidHome)?),
        None => None,
    };
    validate_links(links)?;
    validate_raw(path)?;

    let (absolute, consumed, _) = request_start(path, home)?;
    let intent = directory_intent(path);
    let mut stack = if absolute {
        Vec::new()
    } else if path.starts_with('~') {
        home_components.unwrap_or_default()
    } else {
        cwd_components
    };

    let mut traversal = LinkTraversal {
        links,
        max_expansions,
        expansions: 0,
        active: Vec::new(),
    };

    // Context paths are already normalized, but their components still participate
    // in link lookup as they are pushed from the root.
    if !absolute {
        let context = if path.starts_with('~') {
            home.unwrap_or("")
        } else {
            cwd
        };
        let normalized = normalize(context)?;
        let context_components = components(&normalized);
        stack.clear();
        for (component, _) in context_components {
            match component.as_str() {
                "." => {}
                ".." => {
                    if stack.pop().is_none() {
                        return Err(PathError::Escape { at: 0 });
                    }
                }
                _ => traversal.push_component(&mut stack, component, 0)?,
            }
        }
    }

    let request = if absolute { path } else { &path[consumed..] };
    let base = if absolute { 0 } else { consumed };
    traversal.traverse_raw(&mut stack, request, base)?;
    Ok(render_absolute(&stack, intent))
}

pub fn resolve_links(
    path: &str,
    cwd: &str,
    home: Option<&str>,
    links: &[Link],
    max_expansions: usize,
) -> Result<String, PathError> {
    resolve_links_impl(path, cwd, home, links, max_expansions)
}