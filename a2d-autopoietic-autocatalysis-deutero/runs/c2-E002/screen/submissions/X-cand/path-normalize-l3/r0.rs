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

fn components(path: &str, base: usize) -> Vec<Component<'_>> {
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
            at: base + start,
        });
    }

    result
}

fn directory_intent(path: &str) -> bool {
    path.ends_with('/')
        || components(path, 0)
            .last()
            .is_some_and(|component| component.text == "." || component.text == "..")
}

fn render_absolute(stack: &[String], intent: bool) -> String {
    let mut result = String::from("/");
    result.push_str(&stack.join("/"));
    if intent && !stack.is_empty() {
        result.push('/');
    }
    result
}

fn render_relative(stack: &[String], intent: bool) -> String {
    if stack.is_empty() {
        return ".".to_string();
    }

    let mut result = stack.join("/");
    if intent {
        result.push('/');
    }
    result
}

fn reduce(
    stack: &mut Vec<String>,
    component: Component<'_>,
    absolute: bool,
) -> Result<(), PathError> {
    match component.text {
        "." => {}
        ".." => {
            if stack.last().is_some_and(|last| last != "..") {
                stack.pop();
            } else if absolute {
                return Err(PathError::Escape { at: component.at });
            } else {
                stack.push("..".to_string());
            }
        }
        ordinary => stack.push(ordinary.to_string()),
    }
    Ok(())
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;
    let absolute = path.starts_with('/');
    let mut stack = Vec::new();

    for component in components(path, 0) {
        reduce(&mut stack, component, absolute)?;
    }

    let intent = directory_intent(path);
    if absolute {
        Ok(render_absolute(&stack, intent))
    } else {
        Ok(render_relative(&stack, intent))
    }
}

fn validated_context(path: &str, invalid: PathError) -> Result<Vec<String>, PathError> {
    if !path.starts_with('/') {
        return Err(invalid);
    }
    match normalize(path) {
        Ok(normalized) => Ok(components(&normalized, 0)
            .into_iter()
            .map(|component| component.text.to_string())
            .collect()),
        Err(_) => Err(invalid),
    }
}

fn tilde_remainder<'a>(path: &'a str, home: Option<&str>) -> Result<(&'a str, bool), PathError> {
    if !path.starts_with('~') {
        return Ok((path, false));
    }

    if path == "~" || path.starts_with("~/") {
        if home.is_none() {
            return Err(PathError::MissingHome);
        }
        let remainder = if path == "~" { "" } else { &path[2..] };
        return Ok((remainder, true));
    }

    Err(PathError::UnsupportedTilde { at: 1 })
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_components = validated_context(cwd, PathError::InvalidCwd)?;
    let home_components = match home {
        Some(value) => Some(validated_context(value, PathError::InvalidHome)?),
        None => None,
    };

    validate_raw(path)?;
    let (request, expanded_tilde) = tilde_remainder(path, home)?;
    let absolute = path.starts_with('/');
    let mut stack = if absolute {
        Vec::new()
    } else if expanded_tilde {
        home_components.unwrap_or_default()
    } else {
        cwd_components
    };

    for component in components(request, if expanded_tilde { 2 } else { 0 }) {
        reduce(&mut stack, component, true)?;
    }

    Ok(render_absolute(&stack, directory_intent(path)))
}

fn validate_links(links: &[Link]) -> Result<(), PathError> {
    for (index, link) in links.iter().enumerate() {
        let canonical_path = link.path.starts_with('/')
            && !link.path.ends_with('/')
            && link.path != "/"
            && normalize(&link.path).is_ok_and(|normalized| normalized == link.path);
        let valid_target = validate_raw(&link.target).is_ok();

        if !canonical_path || !valid_target {
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
    Ok(())
}

struct LinkTraversal<'a> {
    links: &'a [Link],
    max_expansions: usize,
    expansions: usize,
    active: Vec<usize>,
    stack: Vec<String>,
}

impl LinkTraversal<'_> {
    fn process_component(
        &mut self,
        component: Component<'_>,
        absolute: bool,
    ) -> Result<(), PathError> {
        match component.text {
            "." => Ok(()),
            ".." => {
                if self.stack.pop().is_none() {
                    if absolute {
                        Err(PathError::Escape { at: component.at })
                    } else {
                        self.stack.push("..".to_string());
                        Ok(())
                    }
                } else {
                    Ok(())
                }
            }
            ordinary => {
                self.stack.push(ordinary.to_string());
                self.expand_match()
            }
        }
    }

    fn expand_match(&mut self) -> Result<(), PathError> {
        let matched = {
            let current = render_absolute(&self.stack, false);
            self.links.iter().position(|link| link.path == current)
        };

        let Some(index) = matched else {
            return Ok(());
        };

        if self.active.contains(&index) {
            return Err(PathError::Loop { index });
        }
        if self.expansions >= self.max_expansions {
            return Err(PathError::Limit { index });
        }
        self.expansions += 1;

        let target = &self.links[index].target;
        let target_absolute = target.starts_with('/');
        if target_absolute {
            self.stack.clear();
        } else {
            self.stack.pop();
        }

        self.active.push(index);
        let result = self.process_path(target, 0, true);
        self.active.pop();
        result
    }

    fn process_path(
        &mut self,
        path: &str,
        base: usize,
        absolute: bool,
    ) -> Result<(), PathError> {
        for component in components(path, base) {
            self.process_component(component, absolute)?;
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
        Some(value) => Some(validated_context(value, PathError::InvalidHome)?),
        None => None,
    };

    validate_links(links)?;
    validate_raw(path)?;
    let (request, expanded_tilde) = tilde_remainder(path, home)?;

    let absolute = path.starts_with('/');
    let mut traversal = LinkTraversal {
        links,
        max_expansions,
        expansions: 0,
        active: Vec::new(),
        stack: if absolute {
            Vec::new()
        } else if expanded_tilde {
            home_components.unwrap_or_default()
        } else {
            cwd_components
        },
    };

    if !absolute {
        let starting = traversal.stack.clone();
        traversal.stack.clear();
        for component in starting {
            traversal.stack.push(component);
            traversal.expand_match()?;
        }
    }

    traversal.process_path(
        request,
        if expanded_tilde { 2 } else { 0 },
        true,
    )?;

    Ok(render_absolute(
        &traversal.stack,
        directory_intent(path),
    ))
}