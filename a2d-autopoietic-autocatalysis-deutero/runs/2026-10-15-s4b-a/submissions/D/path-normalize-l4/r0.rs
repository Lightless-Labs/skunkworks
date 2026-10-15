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
    text: String,
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

fn raw_components(path: &str, start: usize) -> Vec<Component> {
    let bytes = path.as_bytes();
    let mut result = Vec::new();
    let mut i = start.min(bytes.len());
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
        result.push(Component {
            text: path[begin..i].to_owned(),
            at: begin,
        });
    }
    result
}

fn directory_intent(path: &str, components: &[Component]) -> bool {
    path.as_bytes().last() == Some(&b'/')
        || components
            .last()
            .is_some_and(|c| c.text == "." || c.text == "..")
}

fn render(parts: &[String], absolute: bool, intent: bool) -> String {
    let mut result = if absolute {
        format!("/{}", parts.join("/"))
    } else if parts.is_empty() {
        ".".to_owned()
    } else {
        parts.join("/")
    };
    if absolute && parts.is_empty() {
        return "/".to_owned();
    }
    if intent && result != "/" && result != "." {
        result.push('/');
    }
    result
}

fn reduce(
    mut parts: Vec<String>,
    components: &[Component],
    absolute: bool,
) -> Result<Vec<String>, PathError> {
    for component in components {
        match component.text.as_str() {
            "." => {}
            ".." => {
                if parts.last().is_some_and(|p| p != "..") {
                    parts.pop();
                } else if absolute {
                    return Err(PathError::Escape { at: component.at });
                } else {
                    parts.push("..".to_owned());
                }
            }
            _ => parts.push(component.text.clone()),
        }
    }
    Ok(parts)
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;
    let absolute = path.starts_with('/');
    let components = raw_components(path, 0);
    let intent = directory_intent(path, &components);
    let parts = reduce(Vec::new(), &components, absolute)?;
    Ok(render(&parts, absolute, intent))
}

fn validate_context(path: &str) -> Result<Vec<String>, ()> {
    if !path.starts_with('/') {
        return Err(());
    }
    let normalized = normalize(path).map_err(|_| ())?;
    Ok(raw_components(&normalized, 0)
        .into_iter()
        .map(|c| c.text)
        .collect())
}

fn validate_home(home: Option<&str>) -> Result<Option<Vec<String>>, PathError> {
    match home {
        None => Ok(None),
        Some(path) => validate_context(path)
            .map(Some)
            .map_err(|_| PathError::InvalidHome),
    }
}

fn request_start<'a>(
    path: &'a str,
    home: Option<&'a [String]>,
) -> Result<(bool, Vec<String>, usize), PathError> {
    if path.starts_with('~') {
        let rest = path.as_bytes().get(1).copied();
        if rest.is_some_and(|b| b != b'/') {
            return Err(PathError::UnsupportedTilde { at: 1 });
        }
        let home = home.ok_or(PathError::MissingHome)?;
        Ok((true, home.to_vec(), if rest == Some(b'/') { 2 } else { 1 }))
    } else if path.starts_with('/') {
        Ok((true, Vec::new(), 0))
    } else {
        unreachable!("ordinary relative paths are initialized by the caller")
    }
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_parts = validate_context(cwd).map_err(|_| PathError::InvalidCwd)?;
    let home_parts = validate_home(home)?;

    validate_raw(path)?;
    let req = raw_components(path, 0);
    let intent = directory_intent(path, &req);

    let (absolute, mut parts, start) = if path.starts_with('~') {
        let (absolute, parts, start) = request_start(path, home_parts.as_deref())?;
        (absolute, parts, start)
    } else if path.starts_with('/') {
        (true, Vec::new(), 0)
    } else {
        (true, cwd_parts, 0)
    };

    let components = raw_components(path, start);
    let reduced = reduce(parts, &components, absolute)?;
    parts = reduced;
    Ok(render(&parts, true, intent))
}

fn validate_links(links: &[Link]) -> Result<(), PathError> {
    let mut paths: Vec<&str> = Vec::new();
    for (index, link) in links.iter().enumerate() {
        let canonical = link.path.starts_with('/')
            && !link.path.ends_with('/')
            && link.path != "/"
            && normalize(&link.path).is_ok_and(|p| p == link.path);
        if !canonical || validate_raw(&link.target).is_err() {
            return Err(PathError::InvalidLink { index });
        }
        if let Some(first) = paths.iter().position(|p| *p == link.path) {
            return Err(PathError::DuplicateLink {
                first,
                second: index,
            });
        }
        paths.push(&link.path);
    }
    Ok(())
}

struct Traversal<'a> {
    links: &'a [Link],
    active: Vec<usize>,
    expansions: usize,
    max_expansions: usize,
    parts: Vec<String>,
}

impl<'a> Traversal<'a> {
    fn push_component(
        &mut self,
        component: &Component,
        absolute: bool,
        source: &str,
    ) -> Result<(), PathError> {
        match component.text.as_str() {
            "." => return Ok(()),
            ".." => {
                if self.parts.pop().is_none() {
                    return Err(PathError::Escape { at: component.at });
                }
                return Ok(());
            }
            _ => {}
        }
        self.parts.push(component.text.clone());
        let key = format!("/{}", self.parts.join("/"));
        if let Some((index, link)) = self
            .links
            .iter()
            .enumerate()
            .find(|(_, link)| link.path == key)
        {
            self.expand(index, link)?;
        }
        let _ = (absolute, source);
        Ok(())
    }

    fn expand(&mut self, index: usize, link: &Link) -> Result<(), PathError> {
        if self.active.contains(&index) {
            return Err(PathError::Loop { index });
        }
        if self.expansions >= self.max_expansions {
            return Err(PathError::Limit { index });
        }
        self.expansions += 1;
        self.active.push(index);

        let target = &link.target;
        let absolute = target.starts_with('/');
        if absolute {
            self.parts.clear();
        } else {
            self.parts.pop();
        }
        let components = raw_components(target, 0);
        for component in &components {
            self.push_component(component, absolute, target)?;
        }
        self.active.pop();
        Ok(())
    }

    fn process(
        &mut self,
        components: &[Component],
        absolute: bool,
        source: &str,
    ) -> Result<(), PathError> {
        for component in components {
            self.push_component(component, absolute, source)?;
        }
        Ok(())
    }
}

fn resolve_links_internal(
    path: &str,
    cwd_parts: &[String],
    home_parts: Option<&[String]>,
    links: &[Link],
    max_expansions: usize,
) -> Result<String, PathError> {
    validate_raw(path)?;
    let req = raw_components(path, 0);
    let intent = directory_intent(path, &req);

    let (absolute, mut initial, start) = if path.starts_with('~') {
        let (absolute, initial, start) = request_start(path, home_parts)?;
        (absolute, initial, start)
    } else if path.starts_with('/') {
        (true, Vec::new(), 0)
    } else {
        (true, cwd_parts.to_vec(), 0)
    };

    let mut traversal = Traversal {
        links,
        active: Vec::new(),
        expansions: 0,
        max_expansions,
        parts: Vec::new(),
    };

    if !path.starts_with('/') && !path.starts_with('~') {
        let base: Vec<Component> = initial
            .drain(..)
            .map(|text| Component { text, at: 0 })
            .collect();
        traversal.process(&base, true, "")?;
    } else if path.starts_with('~') {
        let base: Vec<Component> = initial
            .drain(..)
            .map(|text| Component { text, at: 0 })
            .collect();
        traversal.process(&base, true, "")?;
    }

    let components = raw_components(path, start);
    traversal.process(&components, absolute, path)?;

    Ok(render(&traversal.parts, true, intent))
}

pub fn resolve_links(
    path: &str,
    cwd: &str,
    home: Option<&str>,
    links: &[Link],
    max_expansions: usize,
) -> Result<String, PathError> {
    let cwd_parts = validate_context(cwd).map_err(|_| PathError::InvalidCwd)?;
    let home_parts = validate_home(home)?;
    validate_links(links)?;
    resolve_links_internal(
        path,
        &cwd_parts,
        home_parts.as_deref(),
        links,
        max_expansions,
    )
}

fn components_of_absolute(path: &str) -> Vec<String> {
    raw_components(path, 0)
        .into_iter()
        .map(|c| c.text)
        .collect()
}

fn route(base: &[String], target: &[String]) -> String {
    let common = base
        .iter()
        .zip(target.iter())
        .take_while(|(a, b)| a == b)
        .count();
    let mut parts = vec!["..".to_owned(); base.len() - common];
    parts.extend_from_slice(&target[common..]);
    if parts.is_empty() {
        ".".to_owned()
    } else {
        parts.join("/")
    }
}

fn candidate_with_suffix(mut route: String, suffix: &[String]) -> String {
    if suffix.is_empty() {
        return route;
    }
    if route == "." {
        suffix.join("/")
    } else {
        route.push('/');
        route.push_str(&suffix.join("/"));
        route
    }
}

fn append_intent(candidate: &mut String, target: &str) {
    if candidate.starts_with('~') {
        *candidate = format!("./{candidate}");
    }
    if target.ends_with('/') && candidate != "." && !candidate.ends_with('/') {
        candidate.push('/');
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
    let base_parts = components_of_absolute(&resolved_base);
    let target_parts = components_of_absolute(&resolved_target);

    let mut candidates = vec![route(&base_parts, &target_parts)];
    for link in links {
        let probe = resolve_links(&link.path, "/", None, links, max_expansions);
        let Ok(probe) = probe else {
            continue;
        };
        let alias_parts = components_of_absolute(&probe);
        if alias_parts.len() <= target_parts.len()
            && alias_parts
                .iter()
                .zip(target_parts.iter())
                .all(|(a, b)| a == b)
        {
            let suffix = &target_parts[alias_parts.len()..];
            candidates.push(candidate_with_suffix(
                route(&base_parts, &components_of_absolute(&link.path)),
                suffix,
            ));
        }
    }

    for candidate in &mut candidates {
        append_intent(candidate, &resolved_target);
    }

    let mut best: Option<String> = None;
    for candidate in candidates {
        let Ok(resolved) = resolve_links(
            &candidate,
            &resolved_base,
            home,
            links,
            max_expansions,
        ) else {
            continue;
        };
        if components_of_absolute(&resolved) != target_parts {
            continue;
        }
        let replace = best.as_ref().is_none_or(|old| {
            candidate.len() < old.len()
                || (candidate.len() == old.len() && candidate.as_bytes() < old.as_bytes())
        });
        if replace {
            best = Some(candidate);
        }
    }

    best.ok_or(PathError::Empty)
}