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
    offset: usize,
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
            offset: base + start,
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

fn render(stack: &[String], intent: bool) -> String {
    let mut result = String::from("/");
    result.push_str(&stack.join("/"));
    if intent && !stack.is_empty() {
        result.push('/');
    }
    result
}

fn normalized_absolute_stack(path: &str) -> Result<Vec<String>, PathError> {
    validate_raw(path)?;
    if !path.starts_with('/') {
        return Err(PathError::Escape { at: 0 });
    }

    let mut stack = Vec::new();
    for component in components(path, 0) {
        match component.text {
            "." => {}
            ".." => {
                if stack.pop().is_none() {
                    return Err(PathError::Escape {
                        at: component.offset,
                    });
                }
            }
            ordinary => stack.push(ordinary.to_owned()),
        }
    }

    Ok(stack)
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;

    let absolute = path.starts_with('/');
    let intent = directory_intent(path);
    let mut stack: Vec<String> = Vec::new();

    for component in components(path, 0) {
        match component.text {
            "." => {}
            ".." => {
                if stack.last().is_some_and(|last| last != "..") {
                    stack.pop();
                } else if absolute {
                    return Err(PathError::Escape {
                        at: component.offset,
                    });
                } else {
                    stack.push("..".to_owned());
                }
            }
            ordinary => stack.push(ordinary.to_owned()),
        }
    }

    if absolute {
        Ok(render(&stack, intent))
    } else if stack.is_empty() {
        Ok(".".to_owned())
    } else {
        let mut result = stack.join("/");
        if intent {
            result.push('/');
        }
        Ok(result)
    }
}

fn expand_request<'a>(
    path: &'a str,
    home: Option<&'a str>,
) -> Result<(bool, &'a str, usize), PathError> {
    let bytes = path.as_bytes();
    if bytes.first() != Some(&b'~') {
        return Ok((false, path, 0));
    }

    if bytes.len() == 1 {
        if home.is_none() {
            return Err(PathError::MissingHome);
        }
        return Ok((true, "", 1));
    }

    if bytes[1] == b'/' {
        if home.is_none() {
            return Err(PathError::MissingHome);
        }
        return Ok((true, &path[2..], 2));
    }

    Err(PathError::UnsupportedTilde { at: 1 })
}

fn validate_context(
    cwd: &str,
    home: Option<&str>,
) -> Result<(Vec<String>, Option<Vec<String>>), PathError> {
    let cwd_stack =
        normalized_absolute_stack(cwd).map_err(|_| PathError::InvalidCwd)?;
    let home_stack = match home {
        Some(home) => Some(
            normalized_absolute_stack(home).map_err(|_| PathError::InvalidHome)?,
        ),
        None => None,
    };
    Ok((cwd_stack, home_stack))
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let (cwd_stack, home_stack) = validate_context(cwd, home)?;
    validate_raw(path)?;

    let intent = directory_intent(path);
    let (tilde_expansion, remainder, base) = expand_request(path, home)?;
    let absolute = path.starts_with('/') && !tilde_expansion;

    let mut stack = if absolute {
        Vec::new()
    } else if tilde_expansion {
        home_stack.ok_or(PathError::MissingHome)?
    } else {
        cwd_stack
    };

    for component in components(remainder, base) {
        match component.text {
            "." => {}
            ".." => {
                if stack.pop().is_none() {
                    return Err(PathError::Escape {
                        at: component.offset,
                    });
                }
            }
            ordinary => stack.push(ordinary.to_owned()),
        }
    }

    Ok(render(&stack, intent))
}

fn validate_links(links: &[Link]) -> Result<(), PathError> {
    let mut seen: Vec<(&str, usize)> = Vec::new();

    for (index, link) in links.iter().enumerate() {
        let path_valid = link.path.starts_with('/')
            && !link.path.ends_with('/')
            && link.path != "/"
            && normalize(&link.path).is_ok_and(|normalized| normalized == link.path);
        let target_valid = validate_raw(&link.target).is_ok();

        if !path_valid || !target_valid {
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

fn link_index(stack: &[String], links: &[Link]) -> Option<usize> {
    let current = render(stack, false);
    links.iter().position(|link| link.path == current)
}

struct LinkTraversal<'a> {
    links: &'a [Link],
    active: Vec<usize>,
    expansions: usize,
    max_expansions: usize,
}

impl LinkTraversal<'_> {
    fn push_component(
        &mut self,
        stack: &mut Vec<String>,
        component: Component<'_>,
    ) -> Result<(), PathError> {
        match component.text {
            "." => return Ok(()),
            ".." => {
                if stack.pop().is_none() {
                    return Err(PathError::Escape {
                        at: component.offset,
                    });
                }
                return Ok(());
            }
            ordinary => stack.push(ordinary.to_owned()),
        }

        self.expand_if_link(stack)
    }

    fn expand_if_link(&mut self, stack: &mut Vec<String>) -> Result<(), PathError> {
        let Some(index) = link_index(stack, self.links) else {
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

        let target = &self.links[index].target;
        if target.starts_with('/') {
            stack.clear();
        } else {
            stack.pop();
        }

        let result = self.process_raw(stack, target, 0);
        self.active.pop();
        result
    }

    fn process_raw(
        &mut self,
        stack: &mut Vec<String>,
        raw: &str,
        base: usize,
    ) -> Result<(), PathError> {
        for component in components(raw, base) {
            self.push_component(stack, component)?;
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
    let (cwd_stack, home_stack) = validate_context(cwd, home)?;
    validate_links(links)?;
    validate_raw(path)?;

    let intent = directory_intent(path);
    let (tilde_expansion, remainder, base) = expand_request(path, home)?;
    let request_absolute = path.starts_with('/') && !tilde_expansion;

    let mut stack = if request_absolute {
        Vec::new()
    } else if tilde_expansion {
        home_stack.clone().ok_or(PathError::MissingHome)?
    } else {
        cwd_stack.clone()
    };

    let mut traversal = LinkTraversal {
        links,
        active: Vec::new(),
        expansions: 0,
        max_expansions,
    };

    if !request_absolute {
        let context = if tilde_expansion {
            home_stack.unwrap_or_default()
        } else {
            cwd_stack
        };

        stack.clear();
        for component in context {
            stack.push(component);
            traversal.expand_if_link(&mut stack)?;
        }
    }

    traversal.process_raw(&mut stack, remainder, base)?;
    Ok(render(&stack, intent))
}

fn main() {}