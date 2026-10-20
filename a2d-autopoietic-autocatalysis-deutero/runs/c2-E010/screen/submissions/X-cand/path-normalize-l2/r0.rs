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
}

fn validate(path: &str) -> Result<(), PathError> {
    if path.is_empty() {
        return Err(PathError::Empty);
    }

    let bytes = path.as_bytes();
    for (i, &byte) in bytes.iter().enumerate() {
        if byte == 0 {
            return Err(PathError::Nul { at: i });
        }
    }
    for (i, &byte) in bytes.iter().enumerate() {
        if byte == b'\\' {
            return Err(PathError::Backslash { at: i });
        }
    }
    for (i, &byte) in bytes.iter().enumerate() {
        if (1..=31).contains(&byte) || byte == 127 {
            return Err(PathError::Control { at: i });
        }
    }
    Ok(())
}

fn intent(path: &str) -> bool {
    path.ends_with('/')
        || path
            .rsplit('/')
            .find(|part| !part.is_empty())
            .is_some_and(|part| part == "." || part == "..")
}

fn scan(path: &str, offset: usize) -> impl Iterator<Item = (&str, usize)> {
    let mut items = Vec::new();
    let bytes = path.as_bytes();
    let mut pos = offset;
    while pos < bytes.len() {
        while pos < bytes.len() && bytes[pos] == b'/' {
            pos += 1;
        }
        let begin = pos;
        while pos < bytes.len() && bytes[pos] != b'/' {
            pos += 1;
        }
        if begin < pos {
            items.push((&path[begin..pos], begin));
        }
    }
    items.into_iter()
}

fn apply<'a>(
    parts: impl IntoIterator<Item = (&'a str, usize)>,
    stack: &mut Vec<String>,
) -> Result<(), PathError> {
    for (part, at) in parts {
        match part {
            "." => {}
            ".." => {
                if stack.is_empty() {
                    return Err(PathError::Escape { at });
                }
                stack.pop();
            }
            _ => stack.push(part.to_owned()),
        }
    }
    Ok(())
}

fn format_path(parts: &[String], directory: bool) -> String {
    let mut out = String::from("/");
    out.push_str(&parts.join("/"));
    if directory && out.len() > 1 {
        out.push('/');
    }
    out
}

pub fn normalize(path: &str) -> Result<String, PathError> {
    validate(path)?;
    let absolute = path.starts_with('/');
    let mut stack: Vec<String> = Vec::new();

    for (part, at) in scan(path, 0) {
        match part {
            "." => {}
            ".." if stack.last().is_some_and(|p| p != "..") => {
                stack.pop();
            }
            ".." if absolute => return Err(PathError::Escape { at }),
            ".." => stack.push("..".to_owned()),
            _ => stack.push(part.to_owned()),
        }
    }

    let mut out = if absolute {
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

    if intent(path) && out != "/" && out != "." {
        out.push('/');
    }
    Ok(out)
}

fn context(path: &str) -> Result<Vec<String>, ()> {
    if !path.starts_with('/') {
        return Err(());
    }
    let normalized = normalize(path).map_err(|_| ())?;
    Ok(normalized
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .map(str::to_owned)
        .collect())
}

pub fn resolve(path: &str, cwd: &str, home: Option<&str>) -> Result<String, PathError> {
    let cwd_parts = context(cwd).map_err(|_| PathError::InvalidCwd)?;
    let home_parts = home
        .map(context)
        .transpose()
        .map_err(|_| PathError::InvalidHome)?;

    validate(path)?;
    let bytes = path.as_bytes();
    let tilde = bytes[0] == b'~';
    let (mut stack, offset) = if tilde {
        if bytes.len() > 1 && bytes[1] != b'/' {
            return Err(PathError::UnsupportedTilde { at: 1 });
        }
        let parts = home_parts.ok_or(PathError::MissingHome)?;
        (parts, if bytes.len() == 1 { 1 } else { 2 })
    } else if bytes[0] == b'/' {
        (Vec::new(), 0)
    } else {
        (cwd_parts, 0)
    };

    apply(scan(path, offset), &mut stack)?;
    Ok(format_path(&stack, intent(path)))
}
