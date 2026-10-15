#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathError {
    Empty, Nul { at: usize }, Backslash { at: usize }, Control { at: usize }, Escape { at: usize },
    InvalidCwd, InvalidHome, MissingHome, UnsupportedTilde { at: usize },
    InvalidLink { index: usize }, DuplicateLink { first: usize, second: usize }, Loop { index: usize }, Limit { index: usize },

}
fn validate(s: &str) -> Result<(), PathError> {
    if s.is_empty() { return Err(PathError::Empty); }
    if let Some(at) = s.bytes().position(|b| b == 0) { return Err(PathError::Nul { at }); }
    if let Some(at) = s.bytes().position(|b| b == b'\\') { return Err(PathError::Backslash { at }); }
    if let Some(at) = s.bytes().position(|b| (1..=31).contains(&b) || b == 127) { return Err(PathError::Control { at }); }
    Ok(())
}
fn parts(s: &str) -> Vec<(String, usize)> {
    let mut at = 0;
    s.split('/').filter_map(|p| {
        let start = at; at += p.len() + 1;
        if p.is_empty() { None } else { Some((p.to_string(), start)) }
    }).collect()
}
fn directory(s: &str) -> bool {
    s.ends_with('/') || matches!(s.rsplit('/').find(|p| !p.is_empty()), Some("." | ".."))
}
fn push_part(stack: &mut Vec<String>, p: &str, at: usize, absolute: bool) -> Result<(), PathError> {
    if p == "." { return Ok(()); }
    if p == ".." {
        if stack.last().is_some_and(|p| p != "..") { stack.pop(); }
        else if absolute { return Err(PathError::Escape { at }); }
        else { stack.push(p.to_string()); }
    } else { stack.push(p.to_string()); }
    Ok(())
}
fn render(stack: &[String], absolute: bool, dir: bool) -> String {
    let mut s = stack.join("/");
    if absolute { s.insert(0, '/'); }
    if s.is_empty() { s.push('.'); }
    if dir && s != "/" && s != "." { s.push('/'); }
    s
}
pub fn normalize(path: &str) -> Result<String, PathError> {
    validate(path)?;
    let absolute = path.starts_with('/');
    let mut stack = Vec::new();
    for (p, at) in parts(path) { push_part(&mut stack, &p, at, absolute)?; }
    Ok(render(&stack, absolute, directory(path)))
}

fn context(cwd: &str, home: Option<&str>) -> Result<(Vec<String>, Option<Vec<String>>), PathError> {
    let c = normalize(cwd).ok().filter(|_| cwd.starts_with('/')).ok_or(PathError::InvalidCwd)?;
    let h = match home {
        Some(h) => Some(normalize(h).ok().filter(|_| h.starts_with('/')).ok_or(PathError::InvalidHome)?),
        None => None,
    };
    Ok((names(&c), h.map(|h| names(&h))))
}
fn names(s: &str) -> Vec<String> { parts(s).into_iter().map(|(s, _)| s).collect() }
fn request(path: &str, c: Vec<String>, h: Option<Vec<String>>) -> Result<(Vec<String>, Vec<(String, usize)>), PathError> {
    validate(path)?;
    if path.starts_with('~') {
        if path.len() > 1 && path.as_bytes()[1] != b'/' { return Err(PathError::UnsupportedTilde { at: 1 }); }
        let start = h.ok_or(PathError::MissingHome)?;
        let skip = if path.len() > 1 { 2 } else { 1 };
        let p = parts(&path[skip..]).into_iter().map(|(p, at)| (p, at + skip)).collect();
        Ok((start, p))
    } else { Ok((if path.starts_with('/') { Vec::new() } else { c }, parts(path))) }
}
pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let (c, h) = context(cwd, home)?;
    let (mut stack, p) = request(path, c, h)?;
    for (p, at) in p { push_part(&mut stack, &p, at, true)?; }
    Ok(render(&stack, true, directory(path)))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link { pub path: String, pub target: String }
fn check_links(links: &[Link]) -> Result<(), PathError> {
    let mut seen = std::collections::HashMap::new();
    for (index, link) in links.iter().enumerate() {
        if !link.path.starts_with('/') || link.path.ends_with('/') || normalize(&link.path).ok().as_deref() != Some(link.path.as_str()) || validate(&link.target).is_err() {
            return Err(PathError::InvalidLink { index });
        }
        if let Some(first) = seen.insert(link.path.as_str(), index) { return Err(PathError::DuplicateLink { first, second: index }); }
    }
    Ok(())
}
enum Event { Part(String, usize), End(usize) }
pub fn resolve_links(path: &str, cwd: &str, home: Option<&str>, links: &[Link], max_expansions: usize) -> Result<String, PathError> {
    let (c, h) = context(cwd, home)?;
    check_links(links)?;
    let (start, p) = request(path, c, h)?;
    let mut queue: std::collections::VecDeque<Event> = start.into_iter().map(|p| Event::Part(p, 0)).chain(p.into_iter().map(|(p, at)| Event::Part(p, at))).collect();
    let mut stack = Vec::new();
    let mut active = vec![false; links.len()];
    let mut count = 0;
    while let Some(event) = queue.pop_front() {
        match event {
            Event::End(i) => active[i] = false,
            Event::Part(p, at) => {
                push_part(&mut stack, &p, at, true)?;
                if p == "." || p == ".." { continue; }
                let key = render(&stack, true, false);
                if let Some(index) = links.iter().position(|l| l.path == key) {
                    if active[index] { return Err(PathError::Loop { index }); }
                    if count >= max_expansions { return Err(PathError::Limit { index }); }
                    count += 1;
                    active[index] = true;
                    stack.pop();
                    let target = &links[index].target;
                    if target.starts_with('/') { stack.clear(); }
                    queue.push_front(Event::End(index));
                    for (p, at) in parts(target).into_iter().rev() { queue.push_front(Event::Part(p, at)); }
                }
            }
        }
    }
    Ok(render(&stack, true, directory(path)))
}

fn route(base: &[String], target: &[String]) -> String {
    let n = base.iter().zip(target).take_while(|(a, b)| a == b).count();
    let mut p = vec!["..".to_string(); base.len() - n];
    p.extend_from_slice(&target[n..]);
    if p.is_empty() { ".".to_string() } else { p.join("/") }
}
pub fn relative_to(base: &str, target: &str, cwd: &str, home: Option<&str>, links: &[Link], max_expansions: usize) -> Result<String, PathError> {
    let b = resolve_links(base, cwd, home, links, max_expansions)?;
    let t = resolve_links(target, cwd, home, links, max_expansions)?;
    let bn = names(&b);
    let tn = names(&t);
    let mut candidates = vec![route(&bn, &tn)];
    for link in links {
        if let Ok(anchor) = resolve_links(&link.path, "/", None, links, max_expansions) {
            let a = names(&anchor);
            if tn.starts_with(&a) {
                let mut candidate = route(&bn, &names(&link.path));
                let suffix = tn[a.len()..].join("/");
                if !suffix.is_empty() {
                    if candidate == "." { candidate = suffix; }
                    else { candidate.push('/'); candidate.push_str(&suffix); }
                }
                candidates.push(candidate);
            }
        }
    }
    let mut valid = Vec::new();
    for mut candidate in candidates {
        if candidate.starts_with('~') { candidate.insert_str(0, "./"); }
        if target.ends_with('/') && candidate != "." { candidate.push('/'); }
        if let Ok(result) = resolve_links(&candidate, &b, home, links, max_expansions) {
            if names(&result) == tn { valid.push(candidate); }
        }
    }
    valid.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes())));
    Ok(valid.remove(0))
}
