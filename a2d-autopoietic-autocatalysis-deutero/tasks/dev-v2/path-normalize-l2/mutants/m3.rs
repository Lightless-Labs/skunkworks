#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathError {
    Empty, Nul { at: usize }, Backslash { at: usize }, Control { at: usize }, Escape { at: usize },
    InvalidCwd, InvalidHome, MissingHome, UnsupportedTilde { at: usize },

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
    let (c, h) = context(if path.starts_with('/') { "/" } else { cwd }, home)?;
    let (mut stack, p) = request(path, c, h)?;
    for (p, at) in p { push_part(&mut stack, &p, at, true)?; }
    Ok(render(&stack, true, directory(path)))
}
