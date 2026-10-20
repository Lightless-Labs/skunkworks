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

fn validate_raw(s: &str) -> Result<(), PathError> {
    if s.is_empty() { return Err(PathError::Empty); }
    for (i, &b) in s.as_bytes().iter().enumerate() {
        if b == 0 { return Err(PathError::Nul { at: i }); }
    }
    for (i, &b) in s.as_bytes().iter().enumerate() {
        if b == b'\\' { return Err(PathError::Backslash { at: i }); }
    }
    for (i, &b) in s.as_bytes().iter().enumerate() {
        if (b >= 1 && b <= 31) || b == 127 { return Err(PathError::Control { at: i }); }
    }
    Ok(())
}

fn components(s: &str) -> Vec<(String, usize)> {
    let mut v = Vec::new();
    let mut pos = 0;
    for part in s.split('/') {
        if !part.is_empty() {
            let start = pos;
            v.push((part.to_string(), start));
        }
        pos += part.len() + 1;
    }
    v
}

fn directory_intent(s: &str) -> bool {
    if s.ends_with('/') { return true; }
    s.split('/').filter(|x| !x.is_empty()).last().map_or(false, |x| x == "." || x == "..")
}

fn push_component(stack: &mut Vec<String>, c: &str, at: usize) -> Result<(), PathError> {
    if c == "." { return Ok(()); }
    if c == ".." {
        if stack.last().map_or(false, |v| v != "..") {
            stack.pop();
        } else {
            return Err(PathError::Escape { at });
        }
    } else {
        stack.push(c.to_string());
    }
    Ok(())
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate_raw(path)?;
    let abs = path.starts_with('/');
    let mut stack = Vec::new();
    for (c, at) in components(path) {
        if c == ".." {
            if stack.last().map_or(false, |v| v != "..") { stack.pop(); }
            else if abs { return Err(PathError::Escape { at }); }
            else { stack.push(c); }
        } else if c != "." { stack.push(c); }
    }
    let mut out = if abs { format!("/{}", stack.join("/")) } else if stack.is_empty() { ".".into() } else { stack.join("/") };
    if abs && stack.is_empty() { out = "/".into(); }
    if !stack.is_empty() && directory_intent(path) { out.push('/'); }
    Ok(out)
}

fn context(s: &str, home: bool) -> Result<Vec<String>, PathError> {
    let bad = if home { PathError::InvalidHome } else { PathError::InvalidCwd };
    if !s.starts_with('/') { return Err(bad); }
    let n = normalize(s).map_err(|_| bad.clone())?;
    Ok(components(&n).into_iter().map(|x| x.0).collect())
}

fn initial(path: &str, home: Option<&[String]>) -> Result<(Vec<String>, usize), PathError> {
    if path.starts_with('~') {
        if path == "~" || path.starts_with("~/") {
            let h = home.ok_or(PathError::MissingHome)?;
            return Ok((h.to_vec(), if path == "~" { path.len() } else { 2 }));
        }
        return Err(PathError::UnsupportedTilde { at: 1 });
    }
    Ok((Vec::new(), 0))
}

fn resolve_core(path: &str, cwd: &[String], home: Option<&[String]>) -> Result<String, PathError> {
    validate_raw(path)?;
    let (mut stack, skip) = if path.starts_with('/') {
        (Vec::new(), 0)
    } else {
        let (mut s, k) = initial(path, home)?;
        if k == 0 { s = cwd.to_vec(); }
        (s, k)
    };
    for (c, at) in components(&path[skip..]) {
        push_component(&mut stack, &c, skip + at)?;
    }
    let mut out = format!("/{}", stack.join("/"));
    if stack.is_empty() { out = "/".into(); }
    if !stack.is_empty() && directory_intent(path) { out.push('/'); }
    Ok(out)
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let c = context(cwd, false)?;
    let h = match home { Some(x) => Some(context(x, true)?), None => None };
    resolve_core(path, &c, h.as_deref())
}

fn links_map(links: &[Link]) -> Result<HashMap<String, usize>, PathError> {
    let mut m = HashMap::new();
    for (i, l) in links.iter().enumerate() {
        let path_ok = l.path.starts_with('/') && l.path != "/" && !l.path.ends_with('/')
            && normalize(&l.path).map_or(false, |x| x == l.path);
        if !path_ok || validate_raw(&l.target).is_err() { return Err(PathError::InvalidLink { index: i }); }
        if let Some(&first) = m.get(&l.path) { return Err(PathError::DuplicateLink { first, second: i }); }
        m.insert(l.path.clone(), i);
    }
    Ok(m)
}

struct Resolver<'a> {
    links: &'a [Link],
    map: &'a HashMap<String, usize>,
    active: Vec<usize>,
    count: usize,
    max: usize,
}

impl<'a> Resolver<'a> {
    fn run(&mut self, stack: &mut Vec<String>, raw: &str, origin: usize) -> Result<(), PathError> {
        for (c, local) in components(raw) {
            let at = origin + local;
            match c.as_str() {
                "." => {}
                ".." => {
                    if stack.pop().is_none() { return Err(PathError::Escape { at }); }
                }
                _ => {
                    stack.push(c);
                    let key = format!("/{}", stack.join("/"));
                    if let Some(&i) = self.map.get(&key) {
                        if self.active.contains(&i) { return Err(PathError::Loop { index: i }); }
                        if self.count >= self.max { return Err(PathError::Limit { index: i }); }
                        self.count += 1;
                        self.active.push(i);
                        let target = &self.links[i].target;
                        let abs = target.starts_with('/');
                        if abs { stack.clear(); } else { stack.pop(); }
                        let result = self.run(stack, target, 0);
                        self.active.pop();
                        result?;
                    }
                }
            }
        }
        Ok(())
    }
}

pub fn resolve_links(path: &str, cwd: &str, home: Option<&str>, links: &[Link], max_expansions: usize) -> Result<String, PathError> {
    let cwd_parts = context(cwd, false)?;
    let home_parts = match home { Some(h) => Some(context(h, true)?), None => None };
    let map = links_map(links)?;
    validate_raw(path)?;
    let (mut stack, skip) = if path.starts_with('/') {
        (Vec::new(), 0)
    } else if path.starts_with('~') {
        let (s, k) = initial(path, home_parts.as_deref())?;
        (s, k)
    } else {
        (cwd_parts, 0)
    };
    let mut r = Resolver { links, map: &map, active: Vec::new(), count: 0, max: max_expansions };
    r.run(&mut stack, &path[skip..], skip)?;
    let mut out = format!("/{}", stack.join("/"));
    if stack.is_empty() { out = "/".into(); }
    if !stack.is_empty() && directory_intent(path) { out.push('/'); }
    Ok(out)
}

fn comps(s: &str) -> Vec<String> {
    components(s).into_iter().map(|x| x.0).collect()
}

fn make_route(b: &[String], t: &[String]) -> String {
    let mut i = 0;
    while i < b.len() && i < t.len() && b[i] == t[i] { i += 1; }
    let mut x = vec![".."; b.len() - i];
    x.extend(t[i..].iter().map(|s| s.as_str()));
    if x.is_empty() { ".".into() } else { x.join("/") }
}

pub fn relative_to(base: &str, target: &str, cwd: &str, home: Option<&str>, links: &[Link], max_expansions: usize) -> Result<String, PathError> {
    let b = resolve_links(base, cwd, home, links, max_expansions)?;
    let t = resolve_links(target, cwd, home, links, max_expansions)?;
    let bc = comps(&b);
    let tc = comps(&t);
    let mut choices = vec![make_route(&bc, &tc)];
    for link in links {
        let resolved_key = match resolve_links(&link.path, "/", None, links, max_expansions) {
            Ok(x) => x,
            Err(_) => continue,
        };
        let a = comps(&resolved_key);
        if a.len() <= tc.len() && a.iter().zip(&tc).all(|(x, y)| x == y) {
            let suffix = &tc[a.len()..];
            let mut c = make_route(&bc, &comps(&link.path));
            if !suffix.is_empty() {
                if c == "." { c = suffix.join("/"); } else { c.push('/'); c.push_str(&suffix.join("/")); }
            }
            choices.push(c);
        }
    }
    let dir = t.ends_with('/');
    let mut ok = Vec::new();
    for mut c in choices {
        if c.starts_with('~') { c = format!("./{}", c); }
        if dir && c != "." { c.push('/'); }
        if let Ok(r) = resolve_links(&c, &b, home, links, max_expansions) {
            if comps(&r) == tc { ok.push(c); }
        }
    }
    ok.sort_by(|x, y| x.as_bytes().len().cmp(&y.as_bytes().len()).then_with(|| x.as_bytes().cmp(y.as_bytes())));
    ok.into_iter().next().ok_or(PathError::Escape { at: 0 })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_retained_apis() {
        assert_eq!(normalize("//p/./q"), Ok("/p/q".into()));
        assert_eq!(resolve("~/x", "/cwd", Some("/home")), Ok("/home/x".into()));
        let links = vec![Link { path: "/short".into(), target: "/real".into() }];
        assert_eq!(resolve_links("/short/file", "/", None, &links, 2), Ok("/real/file".into()));
    }
    #[test]
    fn public_relative_direct() {
        assert_eq!(relative_to("/work/code", "/work/docs/readme", "/", None, &[], 0), Ok("../docs/readme".into()));
        assert_eq!(relative_to("/same", "/same", "/", None, &[], 0), Ok(".".into()));
    }
    #[test]
    fn public_relative_shortcut() {
        let links = vec![Link { path: "/s".into(), target: "/long/destination".into() }];
        assert_eq!(relative_to("/", "/long/destination/file", "/", None, &links, 2), Ok("s/file".into()));
    }
    #[test]
    fn public_relative_directory() {
        assert_eq!(relative_to("/a/b", "/a/c/", "/", None, &[], 0), Ok("../c/".into()));
    }
}
