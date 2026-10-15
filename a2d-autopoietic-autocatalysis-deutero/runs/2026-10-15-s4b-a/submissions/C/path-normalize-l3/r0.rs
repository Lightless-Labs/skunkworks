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

fn components(path: &str, offset: usize) -> Vec<(String, usize)> {
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
        out.push((path[start..i].to_string(), offset + start));
    }
    out
}

fn directory_intent(path: &str) -> bool {
    path.ends_with('/')
        || components(path, 0)
            .last()
            .is_some_and(|(component, _)| component == "." || component == "..")
}

fn render(stack: &[String], intent: bool) -> String {
    let mut result = if stack.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", stack.join("/"))
    };
    if intent && result != "/" {
        result.push('/');
    }
    result
}

fn reduce_components(
    stack: &mut Vec<String>,
    items: &[(String, usize)],
    absolute: bool,
) -> Result<(), PathError> {
    for (component, at) in items {
        match component.as_str() {
            "." => {}
            ".." => {
                if stack.last().is_some_and(|s| s != "..") {
                    stack.pop();
                } else if absolute {
                    return Err(PathError::Escape { at: *at });
                } else {
                    stack.push("..".to_string());
                }
            }
            _ => stack.push(component.clone()),
        }
    }
    Ok(())
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;
    let absolute = path.starts_with('/');
    let intent = directory_intent(path);
    let mut stack = Vec::new();
    reduce_components(&mut stack, &components(path, 0), absolute)?;

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
    Ok(result)
}

fn normalized_context(path: &str, is_home: bool) -> Result<Vec<String>, PathError> {
    if !path.starts_with('/') {
        return Err(if is_home {
            PathError::InvalidHome
        } else {
            PathError::InvalidCwd
        });
    }
    let normalized = normalize(path).map_err(|_| {
        if is_home {
            PathError::InvalidHome
        } else {
            PathError::InvalidCwd
        }
    })?;
    Ok(components(&normalized, 0)
        .into_iter()
        .map(|(component, _)| component)
        .collect())
}

fn validate_contexts(
    cwd: &str,
    home: Option<&str>,
) -> Result<(Vec<String>, Option<Vec<String>>), PathError> {
    let cwd_components = normalized_context(cwd, false)?;
    let home_components = match home {
        Some(home) => Some(normalized_context(home, true)?),
        None => None,
    };
    Ok((cwd_components, home_components))
}

fn request_start<'a>(
    path: &'a str,
    cwd: Vec<String>,
    home: Option<Vec<String>>,
) -> Result<(&'a str, Vec<String>, usize), PathError> {
    if path.starts_with('~') {
        if path == "~" || path.starts_with("~/") {
            let base = home.ok_or(PathError::MissingHome)?;
            let (remainder, offset) = if path == "~" {
                ("", 1)
            } else {
                (&path[2..], 2)
            };
            Ok((remainder, base, offset))
        } else {
            Err(PathError::UnsupportedTilde { at: 1 })
        }
    } else if path.starts_with('/') {
        Ok((path, Vec::new(), 0))
    } else {
        Ok((path, cwd, 0))
    }
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let (cwd_components, home_components) = validate_contexts(cwd, home)?;
    validate_raw(path)?;
    let intent = directory_intent(path);
    let (remainder, mut stack, offset) =
        request_start(path, cwd_components, home_components)?;
    reduce_components(&mut stack, &components(remainder, offset), true)?;
    Ok(render(&stack, intent))
}

fn canonical_link_path(path: &str) -> bool {
    path.starts_with('/')
        && path != "/"
        && !path.ends_with('/')
        && normalize(path).is_ok_and(|normalized| normalized == path)
}

#[derive(Clone)]
struct WorkItem {
    text: String,
    offset: usize,
    end_link: Option<usize>,
}

pub fn resolve_links(
    path: &str,
    cwd: &str,
    home: Option<&str>,
    links: &[Link],
    max_expansions: usize,
) -> Result<String, PathError> {
    let (cwd_components, home_components) = validate_contexts(cwd, home)?;

    let mut first_indices = std::collections::HashMap::<String, usize>::new();
    for (index, link) in links.iter().enumerate() {
        if !canonical_link_path(&link.path) || validate_raw(&link.target).is_err() {
            return Err(PathError::InvalidLink { index });
        }
        if let Some(&first) = first_indices.get(&link.path) {
            return Err(PathError::DuplicateLink {
                first,
                second: index,
            });
        }
        first_indices.insert(link.path.clone(), index);
    }

    validate_raw(path)?;
    let intent = directory_intent(path);
    let (remainder, mut stack, offset) =
        request_start(path, cwd_components, home_components)?;

    let mut queue: std::collections::VecDeque<WorkItem> = components(remainder, offset)
        .into_iter()
        .map(|(text, offset)| WorkItem {
            text,
            offset,
            end_link: None,
        })
        .collect();

    // Traverse the normalized starting directory before request components.
    if !path.starts_with('/') && !path.starts_with('~') {
        let request_items: Vec<_> = queue.drain(..).collect();
        let mut prefix = std::collections::VecDeque::new();
        for component in stack.drain(..) {
            prefix.push_back(WorkItem {
                text: component,
                offset: 0,
                end_link: None,
            });
        }
        prefix.extend(request_items);
        queue = prefix;
    } else if path.starts_with('~') && (path == "~" || path.starts_with("~/")) {
        // Home components are already present in the initial stack. Traverse them
        // from root so their links are considered as well.
        let home = normalized_context(home.unwrap(), true)?;
        let request_items: Vec<_> = queue.drain(..).collect();
        stack.clear();
        let mut prefix = std::collections::VecDeque::new();
        for component in home {
            prefix.push_back(WorkItem {
                text: component,
                offset: 0,
                end_link: None,
            });
        }
        prefix.extend(request_items);
        queue = prefix;
    }

    let mut active = Vec::<usize>::new();
    let mut expansions = 0usize;

    while let Some(item) = queue.pop_front() {
        if let Some(index) = item.end_link {
            if let Some(position) = active.iter().rposition(|&active_index| active_index == index) {
                active.remove(position);
            }
            continue;
        }

        match item.text.as_str() {
            "." => continue,
            ".." => {
                if stack.pop().is_none() {
                    return Err(PathError::Escape { at: item.offset });
                }
                continue;
            }
            _ => {}
        }

        stack.push(item.text);
        let absolute = render(&stack, false);
        if let Some((index, link)) = links
            .iter()
            .enumerate()
            .find(|(_, link)| link.path == absolute)
        {
            if active.contains(&index) {
                return Err(PathError::Loop { index });
            }
            if expansions >= max_expansions {
                return Err(PathError::Limit { index });
            }
            expansions += 1;
            active.push(index);
            stack.pop();

            if link.target.starts_with('/') {
                stack.clear();
            }

            let target_items = components(&link.target, 0);
            let mut expanded = std::collections::VecDeque::new();
            for (text, offset) in target_items {
                expanded.push_back(WorkItem {
                    text,
                    offset,
                    end_link: None,
                });
            }
            expanded.push_back(WorkItem {
                text: String::new(),
                offset: 0,
                end_link: Some(index),
            });
            expanded.extend(queue.drain(..));
            queue = expanded;
        }
    }

    Ok(render(&stack, intent))
}

fn main() {}