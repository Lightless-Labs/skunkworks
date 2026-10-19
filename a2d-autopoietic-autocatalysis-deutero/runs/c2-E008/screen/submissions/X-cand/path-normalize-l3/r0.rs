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
    path.ends_with('/')
        || path
            .rsplit('/')
            .find(|component| !component.is_empty())
            .is_some_and(|component| component == "." || component == "..")
}

fn append_directory_slash(mut result: String, directory: bool) -> String {
    if directory && result != "/" && result != "." {
        result.push('/');
    }
    result
}

fn render_absolute(components: &[String], directory: bool) -> String {
    let mut result = String::from("/");
    result.push_str(&components.join("/"));
    append_directory_slash(result, directory)
}

fn render_relative(components: &[String], directory: bool) -> String {
    let mut result = if components.is_empty() {
        String::from(".")
    } else {
        components.join("/")
    };
    append_directory_slash(result, directory)
}

fn components(path: &str) -> impl Iterator<Item = (usize, &str)> {
    path.split('/').scan(0usize, |offset, component| {
        let start = *offset;
        *offset += component.len() + 1;
        Some((start, component))
    })
}

fn normalize_parts(path: &str) -> Result<(bool, Vec<String>), PathError> {
    validate_raw(path)?;
    let absolute = path.as_bytes()[0] == b'/';
    let mut stack: Vec<String> = Vec::new();

    for (at, component) in components(path) {
        if component.is_empty() || component == "." {
            continue;
        }
        if component == ".." {
            if stack.last().is_some_and(|previous| previous != "..") {
                stack.pop();
            } else if absolute {
                return Err(PathError::Escape { at });
            } else {
                stack.push(String::from(".."));
            }
        } else {
            stack.push(component.to_owned());
        }
    }

    Ok((absolute, stack))
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    let directory = directory_intent(path);
    let (absolute, stack) = normalize_parts(path)?;

    if absolute {
        Ok(render_absolute(&stack, directory))
    } else {
        Ok(render_relative(&stack, directory))
    }
}

fn validate_context(path: &str, error: PathError) -> Result<Vec<String>, PathError> {
    if !path.starts_with('/') {
        return Err(error);
    }
    match normalize_parts(path) {
        Ok((true, components)) => Ok(components),
        _ => Err(error),
    }
}

fn requested_start<'a>(
    path: &'a str,
    home: Option<&'a str>,
) -> Result<(bool, &'a str, usize, Vec<String>), PathError> {
    if path.starts_with('~') {
        if path.len() > 1 && path.as_bytes()[1] != b'/' {
            return Err(PathError::UnsupportedTilde { at: 1 });
        }
        let home_components = match home {
            Some(value) => match normalize_parts(value) {
                Ok((true, parts)) => parts,
                _ => return Err(PathError::InvalidHome),
            },
            None => return Err(PathError::MissingHome),
        };
        let remainder_start = if path.len() >= 2 { 2 } else { 1 };
        Ok((
            false,
            &path[remainder_start..],
            remainder_start,
            home_components,
        ))
    } else if path.starts_with('/') {
        Ok((true, path, 0, Vec::new()))
    } else {
        Ok((false, path, 0, Vec::new()))
    }
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_components = validate_context(cwd, PathError::InvalidCwd)?;
    let home_components = match home {
        Some(value) => Some(validate_context(value, PathError::InvalidHome)?),
        None => None,
    };

    validate_raw(path)?;

    if path.starts_with('~') && path.len() > 1 && path.as_bytes()[1] != b'/' {
        return Err(PathError::UnsupportedTilde { at: 1 });
    }

    let directory = directory_intent(path);
    let (absolute, remainder, offset, home_for_tilde) =
        requested_start(path, home)?;
    let mut stack = if absolute {
        Vec::new()
    } else if path.starts_with('~') {
        home_for_tilde
    } else {
        cwd_components
    };

    for (local_at, component) in components(remainder) {
        if component.is_empty() || component == "." {
            continue;
        }
        if component == ".." {
            if stack.pop().is_none() {
                return Err(PathError::Escape {
                    at: offset + local_at,
                });
            }
        } else {
            stack.push(component.to_owned());
        }
    }

    Ok(render_absolute(&stack, directory))
}

fn validate_links(links: &[Link]) -> Result<HashMap<String, (usize, String)>, PathError> {
    let mut table: HashMap<String, (usize, String)> = HashMap::new();

    for (index, link) in links.iter().enumerate() {
        let canonical = link.path.starts_with('/')
            && !link.path.ends_with('/')
            && link.path != "/"
            && normalize(&link.path)
                .is_ok_and(|normalized| normalized == link.path);
        if !canonical || validate_raw(&link.target).is_err() {
            return Err(PathError::InvalidLink { index });
        }

        if let Some((first, _)) = table.get(&link.path) {
            return Err(PathError::DuplicateLink {
                first: *first,
                second: index,
            });
        }
        table.insert(link.path.clone(), (index, link.target.clone()));
    }

    Ok(table)
}

#[derive(Clone)]
struct WorkComponent {
    value: String,
    at: usize,
}

fn absolute_key(stack: &[String]) -> String {
    let mut key = String::from("/");
    key.push_str(&stack.join("/"));
    key
}

fn process_work(
    initial: Vec<WorkComponent>,
    stack: &mut Vec<String>,
    table: &HashMap<String, (usize, String)>,
    active: &mut Vec<usize>,
    expansions: &mut usize,
    max_expansions: usize,
) -> Result<(), PathError> {
    let mut pending = initial;
    while let Some(component) = pending.pop() {
        if component.value.is_empty() || component.value == "." {
            continue;
        }
        if component.value == ".." {
            if stack.pop().is_none() {
                return Err(PathError::Escape { at: component.at });
            }
            continue;
        }

        stack.push(component.value);
        let key = absolute_key(stack);
        if let Some((index, target)) = table.get(&key) {
            if active.contains(index) {
                return Err(PathError::Loop { index: *index });
            }
            if *expansions >= max_expansions {
                return Err(PathError::Limit { index: *index });
            }
            *expansions += 1;
            active.push(*index);

            let absolute_target = target.starts_with('/');
            if absolute_target {
                stack.clear();
            } else {
                stack.pop();
            }

            let mut target_work: Vec<WorkComponent> = components(target)
                .filter(|(_, value)| !value.is_empty())
                .map(|(at, value)| WorkComponent {
                    value: value.to_owned(),
                    at,
                })
                .collect();
            target_work.reverse();

            // Preserve the not-yet-processed request components while the target runs.
            let remaining = std::mem::take(&mut pending);
            process_work(
                target_work,
                stack,
                table,
                active,
                expansions,
                max_expansions,
            )?;
            active.pop();
            pending = remaining;
        }
    }
    Ok(())
}

pub fn resolve_links(
    path: &str,
    cwd: &str,
    home: Option<&str>,
    links: &[Link],
    max_expansions: usize,
) -> Result<String, PathError> {
    let cwd_components = validate_context(cwd, PathError::InvalidCwd)?;
    let home_components = match home {
        Some(value) => Some(validate_context(value, PathError::InvalidHome)?),
        None => None,
    };
    let table = validate_links(links)?;

    validate_raw(path)?;
    if path.starts_with('~') && path.len() > 1 && path.as_bytes()[1] != b'/' {
        return Err(PathError::UnsupportedTilde { at: 1 });
    }

    let directory = directory_intent(path);
    let (absolute, remainder, offset, tilde_components) =
        if path.starts_with('~') {
            let start = home_components.ok_or(PathError::MissingHome)?;
            let remainder_start = if path.len() >= 2 { 2 } else { 1 };
            (
                false,
                &path[remainder_start..],
                remainder_start,
                start,
            )
        } else if path.starts_with('/') {
            (true, path, 0, Vec::new())
        } else {
            (false, path, 0, cwd_components)
        };

    let mut stack = if absolute {
        Vec::new()
    } else {
        tilde_components
    };

    // Starting directory components are walked through link lookup too.
    if !absolute && !stack.is_empty() {
        let mut context_work: Vec<WorkComponent> = stack
            .drain(..)
            .map(|value| WorkComponent { value, at: 0 })
            .collect();
        context_work.reverse();
        let mut walked = Vec::new();
        let mut active = Vec::new();
        let mut expansions = 0;
        process_work(
            context_work,
            &mut walked,
            &table,
            &mut active,
            &mut expansions,
            max_expansions,
        )?;
        stack = walked;

        let request_work: Vec<WorkComponent> = components(remainder)
            .filter(|(_, value)| !value.is_empty())
            .map(|(at, value)| WorkComponent {
                value: value.to_owned(),
                at: offset + at,
            })
            .collect();
        let mut request_work = request_work;
        request_work.reverse();
        process_work(
            request_work,
            &mut stack,
            &table,
            &mut active,
            &mut expansions,
            max_expansions,
        )?;
    } else {
        let mut request_work: Vec<WorkComponent> = components(remainder)
            .filter(|(_, value)| !value.is_empty())
            .map(|(at, value)| WorkComponent {
                value: value.to_owned(),
                at: offset + at,
            })
            .collect();
        request_work.reverse();
        let mut active = Vec::new();
        let mut expansions = 0;
        process_work(
            request_work,
            &mut stack,
            &table,
            &mut active,
            &mut expansions,
            max_expansions,
        )?;
    }

    Ok(render_absolute(&stack, directory))
}