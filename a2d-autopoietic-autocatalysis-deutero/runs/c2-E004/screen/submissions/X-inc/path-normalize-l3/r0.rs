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
    path.as_bytes().last() == Some(&b'/')
        || path
            .rsplit('/')
            .find(|component| !component.is_empty())
            .is_some_and(|component| component == "." || component == "..")
}

fn raw_components(path: &str, offset_base: usize) -> Vec<(String, usize)> {
    let bytes = path.as_bytes();
    let mut components = Vec::new();
    let mut start = 0;

    while start < bytes.len() {
        while start < bytes.len() && bytes[start] == b'/' {
            start += 1;
        }
        if start == bytes.len() {
            break;
        }

        let component_start = start;
        while start < bytes.len() && bytes[start] != b'/' {
            start += 1;
        }
        components.push((
            path[component_start..start].to_string(),
            offset_base + component_start,
        ));
    }

    components
}

fn append_trailing_slash(mut result: String, intent: bool) -> String {
    if intent && result != "/" && result != "." {
        result.push('/');
    }
    result
}

fn render_absolute(components: &[String]) -> String {
    if components.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", components.join("/"))
    }
}

fn reduce_relative(
    components: impl IntoIterator<Item = (String, usize)>,
) -> Result<Vec<String>, PathError> {
    let mut stack: Vec<String> = Vec::new();

    for (component, at) in components {
        match component.as_str() {
            "." => {}
            ".." => {
                if stack.last().is_some_and(|last| last != "..") {
                    stack.pop();
                } else {
                    stack.push(component);
                }
            }
            _ => stack.push(component),
        }
    }

    let _ = at_placeholder();
    Ok(stack)
}

fn at_placeholder() {}

fn reduce_absolute(
    stack: &mut Vec<String>,
    components: impl IntoIterator<Item = (String, usize)>,
) -> Result<(), PathError> {
    for (component, at) in components {
        match component.as_str() {
            "." => {}
            ".." => {
                if stack.pop().is_none() {
                    return Err(PathError::Escape { at });
                }
            }
            _ => stack.push(component),
        }
    }
    Ok(())
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;

    let absolute = path.as_bytes().first() == Some(&b'/');
    let intent = directory_intent(path);
    let components = raw_components(path, 0);

    let result = if absolute {
        let mut stack = Vec::new();
        reduce_absolute(&mut stack, components)?;
        render_absolute(&stack)
    } else {
        let stack = reduce_relative(components)?;
        if stack.is_empty() {
            ".".to_string()
        } else {
            stack.join("/")
        }
    };

    Ok(append_trailing_slash(result, intent))
}

fn validated_context(path: &str, error: PathError) -> Result<Vec<String>, PathError> {
    if path.as_bytes().first() != Some(&b'/') {
        return Err(error);
    }
    match normalize(path) {
        Ok(normalized) => Ok(normalized
            .split('/')
            .filter(|component| !component.is_empty())
            .map(str::to_string)
            .collect()),
        Err(_) => Err(error),
    }
}

fn expand_request_tilde<'a>(
    path: &'a str,
    home: Option<&'a [String]>,
) -> Result<(bool, &'a str, usize, Option<&'a [String]>), PathError> {
    if !path.starts_with('~') {
        return Ok((path.starts_with('/'), path, 0, None));
    }

    let bytes = path.as_bytes();
    if bytes.len() == 1 {
        return match home {
            Some(home) => Ok((false, "", path.len(), Some(home))),
            None => Err(PathError::MissingHome),
        };
    }
    if bytes[1] != b'/' {
        return Err(PathError::UnsupportedTilde { at: 1 });
    }

    let Some(home) = home else {
        return Err(PathError::MissingHome);
    };

    let mut offset = 2;
    while offset < bytes.len() && bytes[offset] == b'/' {
        offset += 1;
    }
    Ok((false, &path[offset..], offset, Some(home)))
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_components = validated_context(cwd, PathError::InvalidCwd)?;
    let home_components = match home {
        Some(home) => Some(validated_context(home, PathError::InvalidHome)?),
        None => None,
    };

    validate_raw(path)?;

    let intent = directory_intent(path);
    let (is_absolute, remainder, offset, tilde_home) =
        expand_request_tilde(path, home_components.as_deref())?;

    let mut stack = if is_absolute {
        Vec::new()
    } else if let Some(home) = tilde_home {
        home.to_vec()
    } else {
        cwd_components
    };

    reduce_absolute(&mut stack, raw_components(remainder, offset))?;
    let result = render_absolute(&stack);
    Ok(append_trailing_slash(result, intent))
}

fn validate_links(links: &[Link]) -> Result<Vec<(String, String, usize)>, PathError> {
    let mut entries: Vec<(String, String, usize)> = Vec::new();

    for (index, link) in links.iter().enumerate() {
        let path_valid = link.path.starts_with('/')
            && link.path != "/"
            && !link.path.ends_with('/')
            && normalize(&link.path).is_ok_and(|normalized| normalized == link.path);
        let target_valid = validate_raw(&link.target).is_ok();

        if !path_valid || !target_valid {
            return Err(PathError::InvalidLink { index });
        }

        if let Some((_, _, first)) = entries.iter().find(|(path, _, _)| path == &link.path) {
            return Err(PathError::DuplicateLink {
                first: *first,
                second: index,
            });
        }
        entries.push((link.path.clone(), link.target.clone(), index));
    }

    Ok(entries)
}

struct LinkTraversal<'a> {
    links: &'a [(String, String, usize)],
    stack: Vec<String>,
    active: Vec<usize>,
    expansions: usize,
    max_expansions: usize,
}

impl LinkTraversal<'_> {
    fn matching_link(&self) -> Option<(usize, String)> {
        let path = render_absolute(&self.stack);
        self.links
            .iter()
            .find(|(key, _, _)| key == &path)
            .map(|(_, target, index)| (*index, target.clone()))
    }

    fn process_components(
        &mut self,
        components: Vec<(String, usize)>,
    ) -> Result<(), PathError> {
        for (component, at) in components {
            match component.as_str() {
                "." => {}
                ".." => {
                    if self.stack.pop().is_none() {
                        return Err(PathError::Escape { at });
                    }
                }
                _ => {
                    self.stack.push(component);
                    if let Some((index, target)) = self.matching_link() {
                        if self.active.contains(&index) {
                            return Err(PathError::Loop { index });
                        }
                        if self.expansions >= self.max_expansions {
                            return Err(PathError::Limit { index });
                        }

                        self.expansions += 1;
                        self.stack.pop();
                        self.active.push(index);

                        if target.starts_with('/') {
                            self.stack.clear();
                        }
                        let result = self.process_components(raw_components(&target, 0));
                        self.active.pop();
                        result?;
                    }
                }
            }
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
    let cwd_components = validated_context(cwd, PathError::InvalidCwd)?;
    let home_components = match home {
        Some(home) => Some(validated_context(home, PathError::InvalidHome)?),
        None => None,
    };

    let link_entries = validate_links(links)?;
    validate_raw(path)?;

    let intent = directory_intent(path);
    let (is_absolute, remainder, offset, tilde_home) =
        expand_request_tilde(path, home_components.as_deref())?;

    let mut traversal = LinkTraversal {
        links: &link_entries,
        stack: Vec::new(),
        active: Vec::new(),
        expansions: 0,
        max_expansions,
    };

    if !is_absolute {
        let starting = if let Some(home) = tilde_home {
            home
        } else {
            &cwd_components
        };
        traversal.process_components(
            starting
                .iter()
                .cloned()
                .map(|component| (component, 0))
                .collect(),
        )?;
    }

    traversal.process_components(raw_components(remainder, offset))?;
    let result = render_absolute(&traversal.stack);
    Ok(append_trailing_slash(result, intent))
}