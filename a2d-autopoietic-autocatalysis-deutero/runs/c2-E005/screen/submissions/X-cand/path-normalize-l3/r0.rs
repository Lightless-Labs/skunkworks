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
    path.rsplit('/').find(|part| !part.is_empty()) == Some(".")
        || path.rsplit('/').find(|part| !part.is_empty()) == Some("..")
}

fn apply_component(
    stack: &mut Vec<String>,
    component: &str,
    absolute: bool,
    at: usize,
) -> Result<(), PathError> {
    match component {
        "" | "." => Ok(()),
        ".." => {
            if stack.last().is_some_and(|last| last != "..") {
                stack.pop();
            } else if absolute {
                return Err(PathError::Escape { at });
            } else {
                stack.push("..".to_string());
            }
            Ok(())
        }
        other => {
            stack.push(other.to_string());
            Ok(())
        }
    }
}

fn component_offsets(path: &str, start: usize) -> Vec<(&str, usize)> {
    let bytes = path.as_bytes();
    let mut result = Vec::new();
    let mut i = start;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i] == b'/' {
            i += 1;
        }
        if i == bytes.len() {
            break;
        }
        let begin = i;
        while i < bytes.len() && bytes[i] != b'/' {
            i += 1;
        }
        result.push((&path[begin..i], begin));
    }
    result
}

fn format_path(stack: &[String], absolute: bool, intent: bool) -> String {
    let mut result = if absolute {
        if stack.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", stack.join("/"))
        }
    } else if stack.is_empty() {
        ".".to_string()
    } else {
        stack.join("/")
    };

    if intent && result != "/" && result != "." {
        result.push('/');
    }
    result
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;
    let absolute = path.as_bytes()[0] == b'/';
    let intent = directory_intent(path);
    let mut stack = Vec::new();

    for (component, at) in component_offsets(path, 0) {
        apply_component(&mut stack, component, absolute, at)?;
    }

    Ok(format_path(&stack, absolute, intent))
}

fn normalized_context(path: &str) -> Result<Vec<String>, PathError> {
    if !path.starts_with('/') {
        return Err(PathError::InvalidCwd);
    }
    let normalized = normalize(path).map_err(|_| PathError::InvalidCwd)?;
    Ok(component_offsets(&normalized, 1)
        .into_iter()
        .map(|(component, _)| component.to_string())
        .collect())
}

fn validated_home(home: Option<&str>) -> Result<Option<Vec<String>>, PathError> {
    match home {
        None => Ok(None),
        Some(path) => {
            if !path.starts_with('/') {
                return Err(PathError::InvalidHome);
            }
            let normalized = normalize(path).map_err(|_| PathError::InvalidHome)?;
            Ok(Some(
                component_offsets(&normalized, 1)
                    .into_iter()
                    .map(|(component, _)| component.to_string())
                    .collect(),
            ))
        }
    }
}

fn tilde_request(path: &str) -> Result<(bool, usize), PathError> {
    if !path.starts_with('~') {
        return Ok((false, 0));
    }
    if path == "~" || path.starts_with("~/") {
        Ok((true, if path == "~" { path.len() } else { 2 }))
    } else {
        Err(PathError::UnsupportedTilde { at: 1 })
    }
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_stack = normalized_context(cwd)?;
    let home_stack = validated_home(home)?;

    validate_raw(path)?;
    let (tilde, request_start) = tilde_request(path)?;

    let (mut stack, start) = if tilde {
        (
            home_stack.ok_or(PathError::MissingHome)?,
            request_start,
        )
    } else if path.starts_with('/') {
        (Vec::new(), 0)
    } else {
        (cwd_stack, 0)
    };

    for (component, at) in component_offsets(path, start) {
        apply_component(&mut stack, component, true, at)?;
    }

    Ok(format_path(&stack, true, directory_intent(path)))
}

struct Traversal<'a> {
    links: &'a [Link],
    active: Vec<usize>,
    expansions: usize,
    max_expansions: usize,
}

impl Traversal<'_> {
    fn process(
        &mut self,
        raw: &str,
        start: usize,
        stack: &mut Vec<String>,
        base_offset: usize,
    ) -> Result<(), PathError> {
        for (component, local_at) in component_offsets(raw, start) {
            let at = base_offset + local_at;
            match component {
                "." => {}
                ".." => apply_component(stack, component, true, at)?,
                _ => {
                    stack.push(component.to_string());
                    let matched = {
                        let current = format!("/{}", stack.join("/"));
                        self.links.iter().position(|link| link.path == current)
                    };

                    if let Some(index) = matched {
                        if self.active.contains(&index) {
                            return Err(PathError::Loop { index });
                        }
                        if self.expansions >= self.max_expansions {
                            return Err(PathError::Limit { index });
                        }
                        self.expansions += 1;

                        let link = &self.links[index];
                        if !link.target.starts_with('/') {
                            stack.pop();
                        } else {
                            stack.clear();
                        }

                        self.active.push(index);
                        let target_absolute = link.target.starts_with('/');
                        let target_parts = component_offsets(
                            &link.target,
                            if target_absolute { 1 } else { 0 },
                        );
                        let target_result = (|| {
                            for (target_component, target_at) in target_parts {
                                match target_component {
                                    "." => {}
                                    ".." => apply_component(
                                        stack,
                                        target_component,
                                        true,
                                        target_at,
                                    )?,
                                    _ => {
                                        stack.push(target_component.to_string());
                                        let current = format!("/{}", stack.join("/"));
                                        if let Some(nested) =
                                            self.links.iter().position(|entry| entry.path == current)
                                        {
                                            if self.active.contains(&nested) {
                                                return Err(PathError::Loop { index: nested });
                                            }
                                            if self.expansions >= self.max_expansions {
                                                return Err(PathError::Limit { index: nested });
                                            }
                                            self.expand_matched(nested, stack)?;
                                        }
                                    }
                                }
                            }
                            Ok(())
                        })();
                        self.active.pop();
                        target_result?;
                    }
                }
            }
        }
        Ok(())
    }

    fn expand_matched(
        &mut self,
        index: usize,
        stack: &mut Vec<String>,
    ) -> Result<(), PathError> {
        if self.active.contains(&index) {
            return Err(PathError::Loop { index });
        }
        if self.expansions >= self.max_expansions {
            return Err(PathError::Limit { index });
        }
        self.expansions += 1;

        let target = self.links[index].target.clone();
        if target.starts_with('/') {
            stack.clear();
        } else {
            stack.pop();
        }

        self.active.push(index);
        let result = self.process(&target, 0, stack, 0);
        self.active.pop();
        result
    }
}

pub fn resolve_links(
    path: &str,
    cwd: &str,
    home: Option<&str>,
    links: &[Link],
    max_expansions: usize,
) -> Result<String, PathError> {
    let cwd_stack = normalized_context(cwd)?;
    let home_stack = validated_home(home)?;

    for (index, link) in links.iter().enumerate() {
        let valid_path = link.path.starts_with('/')
            && !link.path.ends_with('/')
            && normalize(&link.path)
                .map(|normalized| normalized == link.path)
                .unwrap_or(false)
            && link.path != "/";
        if !valid_path || validate_raw(&link.target).is_err() {
            return Err(PathError::InvalidLink { index });
        }

        if let Some(first) = links[..index]
            .iter()
            .position(|previous| previous.path == link.path)
        {
            return Err(PathError::DuplicateLink {
                first,
                second: index,
            });
        }
    }

    validate_raw(path)?;
    let (tilde, request_start) = tilde_request(path)?;

    let (mut stack, start) = if tilde {
        (
            home_stack.ok_or(PathError::MissingHome)?,
            request_start,
        )
    } else if path.starts_with('/') {
        (Vec::new(), 0)
    } else {
        (cwd_stack, 0)
    };

    let mut traversal = Traversal {
        links,
        active: Vec::new(),
        expansions: 0,
        max_expansions,
    };

    traversal.process(path, start, &mut stack, 0)?;

    Ok(format_path(
        &stack,
        true,
        directory_intent(path),
    ))
}