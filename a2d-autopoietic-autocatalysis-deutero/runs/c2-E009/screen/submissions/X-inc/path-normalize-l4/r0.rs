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
    path.split('/')
        .filter(|component| !component.is_empty())
        .last()
        .is_some_and(|component| component == "." || component == "..")
}

fn append_directory_slash(result: &mut String, intent: bool) {
    if intent && result != "/" && result != "." && !result.ends_with('/') {
        result.push('/');
    }
}

fn render_relative(components: &[String]) -> String {
    if components.is_empty() {
        ".".to_owned()
    } else {
        components.join("/")
    }
}

fn render_absolute(components: &[String]) -> String {
    if components.is_empty() {
        "/".to_owned()
    } else {
        format!("/{}", components.join("/"))
    }
}

fn reduce_components(
    stack: &mut Vec<String>,
    path: &str,
    base_offset: usize,
    absolute: bool,
) -> Result<(), PathError> {
    let mut offset = base_offset;
    for component in path.split('/') {
        let component_offset = offset;
        offset += component.len() + 1;
        if component.is_empty() || component == "." {
            continue;
        }
        if component == ".." {
            if stack.last().is_some_and(|last| last != "..") {
                stack.pop();
            } else if absolute {
                return Err(PathError::Escape {
                    at: component_offset,
                });
            } else {
                stack.push("..".to_owned());
            }
        } else {
            stack.push(component.to_owned());
        }
    }
    Ok(())
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;
    let absolute = path.as_bytes()[0] == b'/';
    let intent = directory_intent(path);
    let mut components = Vec::new();
    reduce_components(&mut components, path, 0, absolute)?;
    let mut result = if absolute {
        render_absolute(&components)
    } else {
        render_relative(&components)
    };
    append_directory_slash(&mut result, intent);
    Ok(result)
}

fn validate_context(path: &str) -> Result<Vec<String>, PathError> {
    if !path.starts_with('/') {
        return Err(PathError::InvalidCwd);
    }
    let normalized = normalize(path).map_err(|_| PathError::InvalidCwd)?;
    Ok(normalized
        .split('/')
        .filter(|component| !component.is_empty())
        .map(str::to_owned)
        .collect())
}

fn validate_home(home: Option<&str>) -> Result<Option<Vec<String>>, PathError> {
    home.map(|path| {
        if !path.starts_with('/') {
            return Err(PathError::InvalidHome);
        }
        let normalized = normalize(path).map_err(|_| PathError::InvalidHome)?;
        Ok(normalized
            .split('/')
            .filter(|component| !component.is_empty())
            .map(str::to_owned)
            .collect())
    })
    .transpose()
}

fn expand_tilde<'a>(
    path: &'a str,
    home: Option<&'a [String]>,
) -> Result<(bool, &'a str, usize, Option<Vec<String>>), PathError> {
    if !path.starts_with('~') {
        return Ok((false, path, 0, None));
    }
    let bytes = path.as_bytes();
    if bytes.len() > 1 && bytes[1] != b'/' {
        return Err(PathError::UnsupportedTilde { at: 1 });
    }
    let home = home.ok_or(PathError::MissingHome)?;
    let rest_offset = if bytes.len() > 1 { 2 } else { 1 };
    Ok((
        true,
        &path[rest_offset..],
        rest_offset,
        Some(home.to_vec()),
    ))
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_components = validate_context(cwd)?;
    let home_components = validate_home(home)?;
    validate_raw(path)?;
    let intent = directory_intent(path);
    let absolute = path.starts_with('/');
    let (tilde, remainder, offset, expanded_home) =
        expand_tilde(path, home_components.as_deref())?;

    let mut components = if absolute {
        Vec::new()
    } else if tilde {
        expanded_home.unwrap_or_default()
    } else {
        cwd_components
    };

    reduce_components(&mut components, remainder, offset, true)?;
    let mut result = render_absolute(&components);
    append_directory_slash(&mut result, intent);
    Ok(result)
}

fn validate_links(links: &[Link]) -> Result<HashMap<String, usize>, PathError> {
    let mut by_path = HashMap::new();
    for (index, link) in links.iter().enumerate() {
        let valid_path = link.path.starts_with('/')
            && !link.path.ends_with('/')
            && link.path != "/"
            && normalize(&link.path).is_ok_and(|normalized| normalized == link.path);
        if !valid_path || validate_raw(&link.target).is_err() {
            return Err(PathError::InvalidLink { index });
        }
        if let Some(&first) = by_path.get(&link.path) {
            return Err(PathError::DuplicateLink {
                first,
                second: index,
            });
        }
        by_path.insert(link.path.clone(), index);
    }
    Ok(by_path)
}

#[derive(Clone)]
struct Component {
    name: String,
    offset: usize,
}

fn raw_components(path: &str, base_offset: usize) -> Vec<Component> {
    let mut result = Vec::new();
    let mut offset = base_offset;
    for component in path.split('/') {
        result.push(Component {
            name: component.to_owned(),
            offset,
        });
        offset += component.len() + 1;
    }
    result
}

fn stack_path(stack: &[String]) -> String {
    render_absolute(stack)
}

fn push_or_reduce(
    stack: &mut Vec<String>,
    component: &Component,
    links: &HashMap<String, usize>,
    link_table: &[Link],
    active: &mut Vec<usize>,
    expansions: &mut usize,
    max_expansions: usize,
) -> Result<(), PathError> {
    if component.name.is_empty() || component.name == "." {
        return Ok(());
    }
    if component.name == ".." {
        if stack.is_empty() {
            return Err(PathError::Escape {
                at: component.offset,
            });
        }
        stack.pop();
        return Ok(());
    }

    stack.push(component.name.clone());
    let key = stack_path(stack);
    if let Some(&index) = links.get(&key) {
        if active.contains(&index) {
            return Err(PathError::Loop { index });
        }
        if *expansions >= max_expansions {
            return Err(PathError::Limit { index });
        }
        *expansions += 1;

        let matched_stack_len = stack.len();
        let target = &link_table[index].target;
        let target_absolute = target.starts_with('/');
        if target_absolute {
            stack.clear();
        } else {
            stack.pop();
        }

        active.push(index);
        let target_components = raw_components(target, 0);
        for target_component in &target_components {
            process_component(
                stack,
                target_component,
                links,
                link_table,
                active,
                expansions,
                max_expansions,
            )?;
        }
        active.pop();

        let _ = matched_stack_len;
    }
    Ok(())
}

fn process_component(
    stack: &mut Vec<String>,
    component: &Component,
    links: &HashMap<String, usize>,
    link_table: &[Link],
    active: &mut Vec<usize>,
    expansions: &mut usize,
    max_expansions: usize,
) -> Result<(), PathError> {
    push_or_reduce(
        stack,
        component,
        links,
        link_table,
        active,
        expansions,
        max_expansions,
    )
}

fn resolve_links_prevalidated(
    path: &str,
    cwd_components: &[String],
    home_components: Option<&[String]>,
    links: &[Link],
    link_map: &HashMap<String, usize>,
    max_expansions: usize,
) -> Result<String, PathError> {
    validate_raw(path)?;
    let intent = directory_intent(path);
    let absolute = path.starts_with('/');
    let (tilde, remainder, offset, expanded_home) =
        expand_tilde(path, home_components)?;

    let mut stack = if absolute {
        Vec::new()
    } else if tilde {
        expanded_home.unwrap_or_default()
    } else {
        cwd_components.to_vec()
    };

    let mut active = Vec::new();
    let mut expansions = 0;
    let request_components = raw_components(remainder, offset);
    for component in &request_components {
        process_component(
            &mut stack,
            component,
            link_map,
            links,
            &mut active,
            &mut expansions,
            max_expansions,
        )?;
    }

    let mut result = render_absolute(&stack);
    append_directory_slash(&mut result, intent);
    Ok(result)
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
    let link_map = validate_links(links)?;
    resolve_links_prevalidated(
        path,
        &cwd_components,
        home_components.as_deref(),
        links,
        &link_map,
        max_expansions,
    )
}

fn absolute_components(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|component| !component.is_empty())
        .map(str::to_owned)
        .collect()
}

fn route(base: &[String], target: &[String]) -> String {
    let common = base
        .iter()
        .zip(target)
        .take_while(|(left, right)| left == right)
        .count();
    let mut parts = vec!["..".to_owned(); base.len() - common];
    parts.extend(target[common..].iter().cloned());
    render_relative(&parts)
}

fn protect_tilde(candidate: String) -> String {
    if candidate.starts_with('~') {
        format!("./{candidate}")
    } else {
        candidate
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
    let base_components = absolute_components(&resolved_base);
    let target_components = absolute_components(&resolved_target);
    let target_directory = resolved_target.ends_with('/');

    let mut candidates = vec![route(&base_components, &target_components)];

    for link in links {
        let probed = match resolve_links(
            &link.path,
            "/",
            None,
            links,
            max_expansions,
        ) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let alias_components = absolute_components(&link.path);
        let probed_components = absolute_components(&probed);
        if probed_components.len() <= target_components.len()
            && probed_components
                .iter()
                .zip(&target_components)
                .all(|(left, right)| left == right)
        {
            let suffix = &target_components[probed_components.len()..];
            let mut candidate = route(&base_components, &alias_components);
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

    let mut valid = Vec::new();
    for candidate in candidates {
        let mut candidate = protect_tilde(candidate);
        if target_directory && candidate != "." && !candidate.ends_with('/') {
            candidate.push('/');
        }
        if let Ok(resolved) = resolve_links(
            &candidate,
            &resolved_base,
            home,
            links,
            max_expansions,
        ) {
            if absolute_components(&resolved) == target_components {
                valid.push(candidate);
            }
        }
    }

    valid.sort_by(|left, right| {
        left.len()
            .cmp(&right.len())
            .then_with(|| left.as_bytes().cmp(right.as_bytes()))
    });
    valid.into_iter().next().ok_or(PathError::Empty)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_retained_apis() {
        assert_eq!(normalize("//p/./q"), Ok("/p/q".into()));
        assert_eq!(resolve("~/x", "/cwd", Some("/home")), Ok("/home/x".into()));
        let links = vec![Link {
            path: "/short".into(),
            target: "/real".into(),
        }];
        assert_eq!(
            resolve_links("/short/file", "/", None, &links, 2),
            Ok("/real/file".into())
        );
    }

    #[test]
    fn public_relative_direct() {
        assert_eq!(
            relative_to("/work/code", "/work/docs/readme", "/", None, &[], 0),
            Ok("../docs/readme".into())
        );
        assert_eq!(
            relative_to("/same", "/same", "/", None, &[], 0),
            Ok(".".into())
        );
    }

    #[test]
    fn public_relative_shortcut() {
        let links = vec![Link {
            path: "/s".into(),
            target: "/long/destination".into(),
        }];
        assert_eq!(
            relative_to("/", "/long/destination/file", "/", None, &links, 2),
            Ok("s/file".into())
        );
    }

    #[test]
    fn public_relative_directory() {
        assert_eq!(
            relative_to("/a/b", "/a/c/", "/", None, &[], 0),
            Ok("../c/".into())
        );
    }
}