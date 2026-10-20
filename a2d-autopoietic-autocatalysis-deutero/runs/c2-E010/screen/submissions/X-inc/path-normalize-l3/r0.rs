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

#[derive(Clone)]
struct Component {
    value: String,
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

fn components(path: &str) -> Vec<Component> {
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
            value: path[start..i].to_string(),
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
        .is_some_and(|c| c.value == "." || c.value == "..")
}

fn render(stack: &[String], absolute: bool, intent: bool) -> String {
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

fn reduce_component(
    stack: &mut Vec<String>,
    component: &Component,
    absolute: bool,
) -> Result<(), PathError> {
    match component.value.as_str() {
        "." => {}
        ".." => {
            if stack.last().is_some_and(|s| s != "..") {
                stack.pop();
            } else if absolute {
                return Err(PathError::Escape { at: component.at });
            } else {
                stack.push("..".to_string());
            }
        }
        name => stack.push(name.to_string()),
    }
    Ok(())
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;

    let absolute = path.as_bytes().first() == Some(&b'/');
    let intent = directory_intent(path);
    let mut stack = Vec::new();

    for component in components(path) {
        reduce_component(&mut stack, &component, absolute)?;
    }

    Ok(render(&stack, absolute, intent))
}

fn validated_context(path: &str, invalid: PathError) -> Result<Vec<String>, PathError> {
    if path.as_bytes().first() != Some(&b'/') {
        return Err(invalid);
    }
    let normalized = normalize(path).map_err(|_| invalid)?;
    Ok(components(&normalized)
        .into_iter()
        .map(|c| c.value)
        .collect())
}

fn expand_tilde<'a>(
    path: &'a str,
    home: Option<&'a str>,
) -> Result<(Option<&'a str>, usize), PathError> {
    if path.as_bytes().first() != Some(&b'~') {
        return Ok((None, 0));
    }

    if path.len() == 1 {
        if home.is_none() {
            return Err(PathError::MissingHome);
        }
        return Ok((Some(""), 1));
    }

    if path.as_bytes()[1] == b'/' {
        if home.is_none() {
            return Err(PathError::MissingHome);
        }
        return Ok((Some(&path[2..]), 2));
    }

    Err(PathError::UnsupportedTilde { at: 1 })
}

fn resolve_inner(
    path: &str,
    cwd: &str,
    home: Option<&str>,
) -> Result<String, PathError> {
    let cwd_stack = validated_context(cwd, PathError::InvalidCwd)?;
    let home_stack = match home {
        Some(h) => Some(validated_context(h, PathError::InvalidHome)?),
        None => None,
    };

    validate_raw(path)?;
    let (remainder, consumed) = expand_tilde(path, home)?;

    let absolute = path.as_bytes().first() == Some(&b'/');
    let (mut stack, request) = if absolute {
        (Vec::new(), path)
    } else if consumed != 0 {
        (home_stack.unwrap_or_default(), remainder.unwrap_or(""))
    } else {
        (cwd_stack, path)
    };

    for mut component in components(request) {
        component.at += consumed;
        reduce_component(&mut stack, &component, true)?;
    }

    Ok(render(&stack, true, directory_intent(path)))
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    resolve_inner(path, cwd, home)
}

fn validate_links(links: &[Link]) -> Result<(), PathError> {
    for (index, link) in links.iter().enumerate() {
        let canonical = link.path.starts_with('/')
            && !link.path.ends_with('/')
            && link.path != "/"
            && normalize(&link.path)
                .is_ok_and(|normalized| normalized == link.path);

        if !canonical || validate_raw(&link.target).is_err() {
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
}

impl LinkTraversal<'_> {
    fn process(
        &mut self,
        input: &str,
        offset: usize,
        stack: &mut Vec<String>,
    ) -> Result<(), PathError> {
        for mut component in components(input) {
            component.at += offset;
            match component.value.as_str() {
                "." => continue,
                ".." => {
                    if stack.pop().is_none() {
                        return Err(PathError::Escape { at: component.at });
                    }
                    continue;
                }
                _ => {}
            }

            stack.push(component.value);
            let matched = {
                let current = format!("/{}", stack.join("/"));
                self.links
                    .iter()
                    .position(|link| link.path == current)
            };

            if let Some(index) = matched {
                if self.active.contains(&index) {
                    return Err(PathError::Loop { index });
                }
                if self.expansions >= self.max_expansions {
                    return Err(PathError::Limit { index });
                }
                self.expansions += 1;
                self.active.push(index);

                let target = &self.links[index].target;
                if target.starts_with('/') {
                    stack.clear();
                } else {
                    stack.pop();
                }

                let target_result = self.process(target, 0, stack);
                self.active.pop();
                target_result?;
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
    let cwd_stack = validated_context(cwd, PathError::InvalidCwd)?;
    let home_stack = match home {
        Some(h) => Some(validated_context(h, PathError::InvalidHome)?),
        None => None,
    };

    validate_links(links)?;
    validate_raw(path)?;
    let (remainder, consumed) = expand_tilde(path, home)?;

    let mut traversal = LinkTraversal {
        links,
        max_expansions,
        expansions: 0,
        active: Vec::new(),
    };
    let mut stack = Vec::new();

    if path.as_bytes().first() != Some(&b'/') && consumed == 0 {
        for component in cwd_stack {
            traversal.process(&component, 0, &mut stack)?;
        }
    } else if consumed != 0 {
        for component in home_stack.unwrap_or_default() {
            traversal.process(&component, 0, &mut stack)?;
        }
    }

    let request = if path.as_bytes().first() == Some(&b'/') {
        path
    } else if consumed != 0 {
        remainder.unwrap_or("")
    } else {
        path
    };

    if path.as_bytes().first() == Some(&b'/') {
        stack.clear();
    }
    traversal.process(request, consumed, &mut stack)?;

    Ok(render(&stack, true, directory_intent(path)))
}