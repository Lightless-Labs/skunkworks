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

fn raw_check(s: &str) -> Result<(), PathError> {
    if s.is_empty() {
        return Err(PathError::Empty);
    }
    for (i, b) in s.bytes().enumerate() {
        if b == 0 {
            return Err(PathError::Nul { at: i });
        }
    }
    for (i, b) in s.bytes().enumerate() {
        if b == b'\\' {
            return Err(PathError::Backslash { at: i });
        }
    }
    for (i, b) in s.bytes().enumerate() {
        if (1..=31).contains(&b) || b == 127 {
            return Err(PathError::Control { at: i });
        }
    }
    Ok(())
}

fn parts<'a>(s: &'a str, offset: usize) -> Vec<(usize, &'a str)> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, b) in s.bytes().enumerate() {
        if b == b'/' {
            if let Some(a) = start.take() {
                out.push((offset + a, &s[a..i]));
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(a) = start {
        out.push((offset + a, &s[a..]));
    }
    out
}

fn want_dir(s: &str) -> bool {
    s.ends_with('/')
        || parts(s, 0)
            .last()
            .is_some_and(|(_, p)| *p == "." || *p == "..")
}

fn render(stack: &[String], intent: bool) -> String {
    let mut out = if stack.is_empty() {
        "/".to_owned()
    } else {
        format!("/{}", stack.join("/"))
    };
    if intent && out != "/" {
        out.push('/');
    }
    out
}

fn reduce_norm(s: &str) -> Result<String, PathError> {
    raw_check(s)?;
    let abs = s.starts_with('/');
    let mut stack: Vec<&str> = Vec::new();
    for (at, p) in parts(s, 0) {
        match p {
            "." => {}
            ".." => {
                if stack.last().is_some_and(|x| *x != "..") {
                    stack.pop();
                } else if abs {
                    return Err(PathError::Escape { at });
                } else {
                    stack.push("..");
                }
            }
            _ => stack.push(p),
        }
    }
    let mut out = if abs {
        if stack.is_empty() {
            "/".to_owned()
        } else {
            format!("/{}", stack.join("/"))
        }
    } else if stack.is_empty() {
        ".".to_owned()
    } else {
        stack.join("/")
    };
    if want_dir(s) && out != "/" && out != "." {
        out.push('/');
    }
    Ok(out)
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    reduce_norm(path)
}

fn context(s: &str) -> Result<Vec<String>, ()> {
    if !s.starts_with('/') {
        return Err(());
    }
    let n = reduce_norm(s).map_err(|_| ())?;
    Ok(parts(&n, 0)
        .into_iter()
        .map(|(_, p)| p.to_owned())
        .collect())
}

fn cwd_parts(s: &str) -> Result<Vec<String>, PathError> {
    context(s).map_err(|_| PathError::InvalidCwd)
}

fn home_parts(s: Option<&str>) -> Result<Option<Vec<String>>, PathError> {
    match s {
        Some(v) => context(v).map(Some).map_err(|_| PathError::InvalidHome),
        None => Ok(None),
    }
}

fn tilt<'a>(
    s: &'a str,
    home: Option<&[String]>,
) -> Result<(&'a str, usize, Option<Vec<String>>), PathError> {
    if s.starts_with('~') {
        if s.len() == 1 || s.as_bytes()[1] == b'/' {
            let h = home.ok_or(PathError::MissingHome)?;
            let n = if s.len() > 1 { 2 } else { 1 };
            Ok((&s[n..], n, Some(h.to_vec())))
        } else {
            Err(PathError::UnsupportedTilde { at: 1 })
        }
    } else {
        Ok((s, 0, None))
    }
}

fn resolve_base(s: &str, cwd: Vec<String>, home: Option<&[String]>) -> Result<Vec<String>, PathError> {
    raw_check(s)?;
    let abs = s.starts_with('/');
    let (rest, off, tilde) = if abs {
        (s, 0, None)
    } else {
        tilt(s, home)?
    };
    let mut stack = if abs {
        Vec::new()
    } else if let Some(h) = tilde {
        h
    } else {
        cwd
    };
    for (at, p) in parts(rest, off) {
        match p {
            "." => {}
            ".." => {
                if stack.pop().is_none() {
                    return Err(PathError::Escape { at });
                }
            }
            _ => stack.push(p.to_owned()),
        }
    }
    Ok(stack)
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd = cwd_parts(cwd)?;
    let home = home_parts(home)?;
    let stack = resolve_base(path, cwd, home.as_deref())?;
    let mut out = render(&stack, want_dir(path));
    if out == "/." {
        out = "/".to_owned();
    }
    Ok(out)
}

fn table(links: &[Link]) -> Result<Vec<(String, String)>, PathError> {
    let mut out = Vec::new();
    for (i, l) in links.iter().enumerate() {
        let valid_path = l.path.starts_with('/')
            && l.path != "/"
            && !l.path.ends_with('/')
            && reduce_norm(&l.path).as_ref().is_ok_and(|v| v == &l.path);
        if !valid_path || raw_check(&l.target).is_err() {
            return Err(PathError::InvalidLink { index: i });
        }
        if let Some(first) = out.iter().position(|(p, _)| p == &l.path) {
            return Err(PathError::DuplicateLink { first, second: i });
        }
        out.push((l.path.clone(), l.target.clone()));
    }
    Ok(out)
}

fn apply_part(
    p: &str,
    at: usize,
    stack: &mut Vec<String>,
    links: &[(String, String)],
    active: &mut Vec<usize>,
    count: &mut usize,
    max: usize,
) -> Result<(), PathError> {
    if p == "." {
        return Ok(());
    }
    if p == ".." {
        if stack.pop().is_none() {
            return Err(PathError::Escape { at });
        }
        return Ok(());
    }
    stack.push(p.to_owned());
    let key = render(stack, false);
    if let Some((idx, (_, target))) = links.iter().enumerate().find(|(_, (name, _))| *name == key) {
        if active.contains(&idx) {
            return Err(PathError::Loop { index: idx });
        }
        if *count >= max {
            return Err(PathError::Limit { index: idx });
        }
        *count += 1;
        active.push(idx);
        if target.starts_with('/') {
            stack.clear();
        } else {
            stack.pop();
        }
        for (off, part) in parts(target, 0) {
            apply_part(part, off, stack, links, active, count, max)?;
        }
        active.pop();
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
    let cwd = cwd_parts(cwd)?;
    let home = home_parts(home)?;
    let links = table(links)?;
    raw_check(path)?;
    let absolute = path.starts_with('/');
    let (rest, off, tilde) = if absolute {
        (path, 0, None)
    } else {
        tilt(path, home.as_deref())?
    };

    let mut stack = if absolute {
        Vec::new()
    } else if let Some(h) = tilde {
        h
    } else {
        cwd
    };

    let mut active = Vec::new();
    let mut count = 0;
    if !absolute {
        let initial = std::mem::take(&mut stack);
        for p in initial {
            apply_part(&p, 0, &mut stack, &links, &mut active, &mut count, max_expansions)?;
        }
    }
    for (at, p) in parts(rest, off) {
        apply_part(p, at, &mut stack, &links, &mut active, &mut count, max_expansions)?;
    }

    Ok(render(&stack, want_dir(path)))
}
