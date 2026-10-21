#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitError {
    Nul { pos: usize },
    TrailingEscape { pos: usize },
    UnterminatedSingle { pos: usize },
    UnterminatedDouble { pos: usize },
    UnterminatedExpansion { pos: usize },
    InvalidExpansion { pos: usize },
    CommandSubstitution { pos: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    pub text: String,
    pub glob: Vec<usize>,
}

#[derive(Clone, Copy)]
enum Mode {
    Bare,
    Single(usize),
    Double(usize),
}

fn name_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn name_byte(b: u8) -> bool {
    name_start(b) || b.is_ascii_digit()
}

fn push(word: &mut Word, s: &str, active: bool) {
    for c in s.chars() {
        if active && matches!(c, '*' | '?' | '[') {
            word.glob.push(word.text.len());
        }
        word.text.push(c);
    }
}

fn env_value<'a>(env: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    env.iter().find(|(n, _)| *n == name).map(|(_, v)| *v)
}

fn parse_name(input: &str, mut p: usize) -> Option<(usize, &str)> {
    let bytes = input.as_bytes();
    if p >= bytes.len() || !name_start(bytes[p]) {
        return None;
    }
    let start = p;
    p += 1;
    while p < bytes.len() && name_byte(bytes[p]) {
        p += 1;
    }
    Some((p, &input[start..p]))
}

fn brace(input: &str, dollar: usize) -> Result<(usize, &str, Option<&str>), SplitError> {
    let bytes = input.as_bytes();
    let mut p = dollar + 2;
    while p < bytes.len() {
        if bytes[p] == b'\\' {
            p += 1;
            if p < bytes.len() {
                let c = input[p..].chars().next().unwrap();
                p += c.len_utf8();
            }
        } else if bytes[p] == b'}' {
            break;
        } else {
            let c = input[p..].chars().next().unwrap();
            p += c.len_utf8();
        }
    }
    if p == bytes.len() {
        return Err(SplitError::UnterminatedExpansion { pos: dollar });
    }

    let body = &input[dollar + 2..p];
    let (name_end, name) = parse_name(body, 0)
        .ok_or(SplitError::InvalidExpansion { pos: dollar })?;
    let tail = &body[name_end..];
    let default = if tail.is_empty() {
        None
    } else if let Some(t) = tail.strip_prefix(":-") {
        Some(t)
    } else {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    };
    Ok((p + 1, name, default))
}

fn decoded(s: &str) -> String {
    let mut result = String::new();
    let mut p = 0;
    while p < s.len() {
        let c = s[p..].chars().next().unwrap();
        p += c.len_utf8();
        if c == '\\' && p < s.len() {
            let next = s[p..].chars().next().unwrap();
            result.push(next);
            p += next.len_utf8();
        } else {
            result.push(c);
        }
    }
    result
}

fn expand(name: &str, default: Option<&str>, env: &[(&str, &str)]) -> String {
    match default {
        None => env_value(env, name).unwrap_or("").to_owned(),
        Some(d) => match env_value(env, name) {
            Some(v) if !v.is_empty() => v.to_owned(),
            _ => decoded(d),
        },
    }
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<Word>, SplitError> {
    if let Some(pos) = input.find('\0') {
        return Err(SplitError::Nul { pos });
    }

    let mut out = Vec::new();
    let mut word = Word { text: String::new(), glob: Vec::new() };
    let mut started = false;
    let mut mode = Mode::Bare;
    let mut p = 0;

    while p < input.len() {
        let c = input[p..].chars().next().unwrap();
        let size = c.len_utf8();

        match mode {
            Mode::Single(_) => {
                if c == '\'' {
                    mode = Mode::Bare;
                } else {
                    push(&mut word, &input[p..p + size], false);
                }
                p += size;
            }
            Mode::Double(_) => {
                if c == '"' {
                    mode = Mode::Bare;
                    p += size;
                } else if c == '\\' {
                    let slash = p;
                    p += size;
                    if p == input.len() {
                        return Err(SplitError::TrailingEscape { pos: slash });
                    }
                    let next = input[p..].chars().next().unwrap();
                    if matches!(next, '"' | '\\' | '$' | '\n') {
                        if next != '\n' {
                            push(&mut word, &input[p..p + next.len_utf8()], false);
                        }
                        p += next.len_utf8();
                    } else {
                        push(&mut word, "\\", false);
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: p });
                } else if c == '$' {
                    let q = p + size;
                    if input[q..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: p });
                    }
                    if input[q..].starts_with('{') {
                        let (end, name, default) = brace(input, p)?;
                        let val = expand(name, default, env);
                        started = true;
                        push(&mut word, &val, false);
                        p = end;
                    } else if let Some((end, name)) = parse_name(input, q) {
                        started = true;
                        let val = env_value(env, name).unwrap_or("");
                        push(&mut word, val, false);
                        p = end;
                    } else {
                        started = true;
                        push(&mut word, "$", false);
                        p += size;
                    }
                } else {
                    push(&mut word, &input[p..p + size], false);
                    p += size;
                }
            }
            Mode::Bare => {
                if matches!(c, ' ' | '\t' | '\r' | '\n') {
                    if started {
                        out.push(word);
                        word = Word { text: String::new(), glob: Vec::new() };
                        started = false;
                    }
                    p += size;
                } else if c == '\'' {
                    started = true;
                    mode = Mode::Single(p);
                    p += size;
                } else if c == '"' {
                    started = true;
                    mode = Mode::Double(p);
                    p += size;
                } else if c == '\\' {
                    let slash = p;
                    p += size;
                    if p == input.len() {
                        return Err(SplitError::TrailingEscape { pos: slash });
                    }
                    let next = input[p..].chars().next().unwrap();
                    if next == '\n' {
                        p += next.len_utf8();
                    } else {
                        started = true;
                        push(&mut word, &input[p..p + next.len_utf8()], false);
                        p += next.len_utf8();
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: p });
                } else if c == '$' {
                    let q = p + size;
                    if input[q..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: p });
                    }
                    if input[q..].starts_with('{') {
                        let (end, name, default) = brace(input, p)?;
                        let val = expand(name, default, env);
                        started = true;
                        push(&mut word, &val, true);
                        p = end;
                    } else if let Some((end, name)) = parse_name(input, q) {
                        started = true;
                        let val = env_value(env, name).unwrap_or("");
                        push(&mut word, val, true);
                        p = end;
                    } else {
                        started = true;
                        push(&mut word, "$", true);
                        p += size;
                    }
                } else {
                    started = true;
                    push(&mut word, &input[p..p + size], true);
                    p += size;
                }
            }
        }
    }

    match mode {
        Mode::Single(pos) => return Err(SplitError::UnterminatedSingle { pos }),
        Mode::Double(pos) => return Err(SplitError::UnterminatedDouble { pos }),
        Mode::Bare => {}
    }
    if started {
        out.push(word);
    }
    Ok(out)
}