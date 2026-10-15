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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Quote {
    None,
    Single(usize),
    Double(usize),
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_name_continue(c: char) -> bool {
    is_name_start(c) || c.is_ascii_digit()
}

fn append(text: &mut String, glob: &mut Vec<usize>, value: &str, active: bool) {
    for c in value.chars() {
        if active && matches!(c, '*' | '?' | '[') {
            glob.push(text.len());
        }
        text.push(c);
    }
}

fn first_env<'a>(env: &'a [(&'a str, &'a str)], name: &str) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| if *key == name { Some(*value) } else { None })
}

fn decode_default(raw: &str) -> String {
    let mut out = String::new();
    let mut chars = raw.char_indices().peekable();
    while let Some((_, c)) = chars.next() {
        if c == '\\' {
            if let Some((_, next)) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<Word>, SplitError> {
    if let Some(pos) = input.find('\0') {
        return Err(SplitError::Nul { pos });
    }

    let mut words = Vec::new();
    let mut current = Word {
        text: String::new(),
        glob: Vec::new(),
    };
    let mut active_word = false;
    let mut quote = Quote::None;
    let mut i = 0;

    while i < input.len() {
        let c = input[i..].chars().next().unwrap();
        let next_i = i + c.len_utf8();

        match quote {
            Quote::Single(_) => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    append(&mut current.text, &mut current.glob, &input[i..next_i], false);
                }
                i = next_i;
            }
            Quote::Double(open_pos) => {
                if c == '"' {
                    quote = Quote::None;
                    i = next_i;
                } else if c == '\\' {
                    if next_i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let n = input[next_i..].chars().next().unwrap();
                    let after_n = next_i + n.len_utf8();
                    if matches!(n, '"' | '\\' | '$' | '\n') {
                        if n != '\n' {
                            append(
                                &mut current.text,
                                &mut current.glob,
                                &input[next_i..after_n],
                                false,
                            );
                        }
                        i = after_n;
                    } else {
                        append(&mut current.text, &mut current.glob, "\\", false);
                        i = next_i;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if input[next_i..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }
                    if input[next_i..].starts_with('{') {
                        let (end, value) = parse_braced(input, i, env)?;
                        append(&mut current.text, &mut current.glob, &value, false);
                        active_word = true;
                        i = end;
                    } else if let Some((name, end)) = parse_unbraced_name(input, next_i) {
                        let value = first_env(env, name).unwrap_or("");
                        append(&mut current.text, &mut current.glob, value, false);
                        active_word = true;
                        i = end;
                    } else {
                        append(&mut current.text, &mut current.glob, "$", false);
                        i = next_i;
                    }
                } else {
                    append(&mut current.text, &mut current.glob, &input[i..next_i], false);
                    i = next_i;
                }
                let _ = open_pos;
            }
            Quote::None => {
                if matches!(c, ' ' | '\t' | '\r' | '\n') {
                    if active_word {
                        words.push(current);
                        current = Word {
                            text: String::new(),
                            glob: Vec::new(),
                        };
                        active_word = false;
                    }
                    i = next_i;
                } else if c == '\'' {
                    quote = Quote::Single(i);
                    active_word = true;
                    i = next_i;
                } else if c == '"' {
                    quote = Quote::Double(i);
                    active_word = true;
                    i = next_i;
                } else if c == '\\' {
                    if next_i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let n = input[next_i..].chars().next().unwrap();
                    let after_n = next_i + n.len_utf8();
                    if n != '\n' {
                        append(
                            &mut current.text,
                            &mut current.glob,
                            &input[next_i..after_n],
                            false,
                        );
                        active_word = true;
                    }
                    i = after_n;
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if input[next_i..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }
                    if input[next_i..].starts_with('{') {
                        let (end, value) = parse_braced(input, i, env)?;
                        append(&mut current.text, &mut current.glob, &value, true);
                        active_word = true;
                        i = end;
                    } else if let Some((name, end)) = parse_unbraced_name(input, next_i) {
                        let value = first_env(env, name).unwrap_or("");
                        append(&mut current.text, &mut current.glob, value, true);
                        active_word = true;
                        i = end;
                    } else {
                        append(&mut current.text, &mut current.glob, "$", true);
                        active_word = true;
                        i = next_i;
                    }
                } else {
                    append(&mut current.text, &mut current.glob, &input[i..next_i], true);
                    active_word = true;
                    i = next_i;
                }
            }
        }
    }

    match quote {
        Quote::Single(pos) => return Err(SplitError::UnterminatedSingle { pos }),
        Quote::Double(pos) => return Err(SplitError::UnterminatedDouble { pos }),
        Quote::None => {}
    }

    if active_word {
        words.push(current);
    }
    Ok(words)
}

fn parse_unbraced_name(input: &str, start: usize) -> Option<(&str, usize)> {
    let c = input.get(start..)?.chars().next()?;
    if !is_name_start(c) {
        return None;
    }
    let mut end = start + c.len_utf8();
    while end < input.len() {
        let c = input[end..].chars().next().unwrap();
        if !is_name_continue(c) {
            break;
        }
        end += c.len_utf8();
    }
    Some((&input[start..end], end))
}

fn parse_braced(input: &str, dollar: usize, env: &[(&str, &str)]) -> Result<(usize, String), SplitError> {
    let content_start = dollar + 2;
    let mut cursor = content_start;
    let mut closing = None;

    while cursor < input.len() {
        let c = input[cursor..].chars().next().unwrap();
        if c == '\\' {
            cursor += c.len_utf8();
            if cursor < input.len() {
                let next = input[cursor..].chars().next().unwrap();
                cursor += next.len_utf8();
            }
        } else if c == '}' {
            closing = Some(cursor);
            break;
        } else {
            cursor += c.len_utf8();
        }
    }

    let close_pos = closing.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
    let contents = &input[content_start..close_pos];

    let name_end = contents
        .char_indices()
        .find_map(|(idx, c)| if !is_name_continue(c) { Some(idx) } else { None })
        .unwrap_or(contents.len());

    if name_end == 0 {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }

    let first = contents.chars().next().unwrap();
    if !is_name_start(first) {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }

    let name = &contents[..name_end];
    let suffix = &contents[name_end..];

    let value = if suffix.is_empty() {
        first_env(env, name).unwrap_or("").to_string()
    } else if let Some(default_raw) = suffix.strip_prefix(":-") {
        match first_env(env, name) {
            Some(value) if !value.is_empty() => value.to_string(),
            _ => decode_default(default_raw),
        }
    } else {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    };

    Ok((close_pos + 1, value))
}