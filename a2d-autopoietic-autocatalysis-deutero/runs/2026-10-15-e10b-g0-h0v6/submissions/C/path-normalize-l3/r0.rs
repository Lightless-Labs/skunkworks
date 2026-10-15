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

fn render_relative(stack: &[String]) -> String {
    if stack.is_empty() {
        ".".to_string()
    } else {
        stack.join("/")
    }
}

fn render_absolute(stack: &[String]) -> String {
    if stack.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", stack.join("/"))
    }
}

fn reduce_relative<'a>(
    stack: &mut Vec<String>,
    parts: impl IntoIterator<Item = Component<'a>>,
    absolute: bool,
) -> Result<(), PathError> {
    for component in parts {
        match component.text {
            "." => {}
            ".." => {
                if stack.last().is_some_and(|name| name != "..") {
                    stack.pop();
                } else if absolute {
                    return Err(PathError::Escape { at: component.at });
                } else {
                    stack.push("..".to_string());
                }
            }
            name => stack.push(name.to_string()),
        }
    }
    Ok(())
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;

    let absolute = path.as_bytes().first() == Some(&b'/');
    let mut stack = Vec::new();
    reduce_relative(&mut stack, components(path, 0), absolute)?;

    let mut result = if absolute {
        render_absolute(&stack)
    } else {
        render_relative(&stack)
    };

    if directory_intent(path) && result != "/" && result != "." {
        result.push('/');
    }
    Ok(result)
}

fn validate_context(path: &str, error: PathError) -> Result<Vec<String>, PathError> {
    if !path.starts_with('/') {
        return Err(error);
    }

    match normalize(path) {
        Ok(normalized) => Ok(components(&normalized, 0)
            .into_iter()
            .map(|component| component.text.to_string())
            .collect()),
        Err(_) => Err(error),
    }
}

fn requested_parts<'a>(path: &'a str) -> Result<(bool, Vec<Component<'a>>), PathError> {
    validate_raw(path)?;

    if path.starts_with('~') {
        if path == "~" {
            return Ok((true, Vec::new()));
        }
        if path.as_bytes().get(1) != Some(&b'/') {
            return Err(PathError::UnsupportedTilde { at: 1 });
        }
        return Ok((true, components(&path[2..], 2)));
    }

    Ok((false, components(path, 0)))
}

fn resolve_basic(
    path: &str,
    cwd: &[String],
    home: Option<&[String]>,
) -> Result<String, PathError> {
    let (tilde, parts) = requested_parts(path)?;
    let absolute = !tilde && path.starts_with('/');

    let mut stack = if absolute {
        Vec::new()
    } else if tilde {
        home.ok_or(PathError::MissingHome)?.to_vec()
    } else {
        cwd.to_vec()
    };

    reduce_relative(&mut stack, parts, true)?;

    let mut result = render_absolute(&stack);
    if directory_intent(path) && result != "/" {
        result.push('/');
    }
    Ok(result)
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd = validate_context(cwd, PathError::InvalidCwd)?;
    let home = match home {
        Some(path) => Some(validate_context(path, PathError::InvalidHome)?),
        None => None,
    };
    resolve_basic(path, &cwd, home.as_deref())
}

fn validate_links(links: &[Link]) -> Result<(), PathError> {
    let mut seen: Vec<(&str, usize)> = Vec::new();

    for (index, link) in links.iter().enumerate() {
        let path_valid = link.path.starts_with('/')
            && !link.path.ends_with('/')
            && link.path != "/"
            && normalize(&link.path).as_deref() == Ok(link.path.as_str());

        if !path_valid || validate_raw(&link.target).is_err() {
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

#[derive(Clone)]
struct WorkComponent {
    text: String,
    at: usize,
}

fn work_components(path: &str, base: usize) -> Vec<WorkComponent> {
    components(path, base)
        .into_iter()
        .map(|component| WorkComponent {
            text: component.text.to_string(),
            at: component.at,
        })
        .collect()
}

fn push_component(stack: &mut Vec<String>, component: &WorkComponent) -> Result<bool, PathError> {
    match component.text.as_str() {
        "." => Ok(false),
        ".." => {
            if stack.is_empty() {
                Err(PathError::Escape { at: component.at })
            } else {
                stack.pop();
                Ok(false)
            }
        }
        name => {
            stack.push(name.to_string());
            Ok(true)
        }
    }
}

struct LinkTraversal<'a> {
    links: &'a [Link],
    stack: Vec<String>,
    active: Vec<usize>,
    expansions: usize,
    max_expansions: usize,
}

impl<'a> LinkTraversal<'a> {
    fn lookup(&self) -> Option<usize> {
        let path = render_absolute(&self.stack);
        self.links.iter().position(|link| link.path == path)
    }

    fn expand(&mut self, index: usize) -> Result<(), PathError> {
        if self.active.contains(&index) {
            return Err(PathError::Loop { index });
        }
        if self.expansions >= self.max_expansions {
            return Err(PathError::Limit { index });
        }

        self.expansions += 1;
        self.active.push(index);

        let target = &self.links[index].target;
        let target_parts = work_components(target, 0);
        if target.starts_with('/') {
            self.stack.clear();
        } else {
            self.stack.pop();
        }

        let result = self.process(target_parts);
        self.active.pop();
        result
    }

    fn process(&mut self, work: Vec<WorkComponent>) -> Result<(), PathError> {
        for component in work {
            if push_component(&mut self.stack, &component)? {
                if let Some(index) = self.lookup() {
                    self.expand(index)?;
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
    let cwd = validate_context(cwd, PathError::InvalidCwd)?;
    let home = match home {
        Some(path) => Some(validate_context(path, PathError::InvalidHome)?),
        None => None,
    };
    validate_links(links)?;

    let (tilde, parts) = requested_parts(path)?;
    let absolute = !tilde && path.starts_with('/');

    let starting_components = if absolute {
        Vec::new()
    } else if tilde {
        home.ok_or(PathError::MissingHome)?
    } else {
        cwd
    };

    let mut traversal = LinkTraversal {
        links,
        stack: Vec::new(),
        active: Vec::new(),
        expansions: 0,
        max_expansions,
    };

    let context_work = starting_components
        .into_iter()
        .map(|text| WorkComponent { text, at: 0 })
        .collect();
    traversal.process(context_work)?;

    let request_work = parts
        .into_iter()
        .map(|component| WorkComponent {
            text: component.text.to_string(),
            at: component.at,
        })
        .collect();
    traversal.process(request_work)?;

    let mut result = render_absolute(&traversal.stack);
    if directory_intent(path) && result != "/" {
        result.push('/');
    }
    Ok(result)
}

fn main() {}