#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitError {
    Nul { pos: usize },
    TrailingEscape { pos: usize },
    UnterminatedSingle { pos: usize },
    UnterminatedDouble { pos: usize },
    UnterminatedExpansion { pos: usize },
    InvalidExpansion { pos: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Quote {
    None,
    Single(usize),
    Double(usize),
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_name_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn lookup<'a>(env: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| if *key == name { Some(*value) } else { None })
}

fn decode_default(text: &str) -> String {
    let mut out = String::new();
    let mut iter = text.char_indices().peekable();
    while let Some((_, c)) = iter.next() {
        if c == '\\' {
            if let Some((_, next)) = iter.next() {
                out.push(next);
            } else {
                out.push('\\');
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<String>, SplitError> {
    if let Some(pos) = input.find('\0') {
        return Err(SplitError::Nul { pos });
    }

    let mut words = Vec::new();
    let mut word = String::new();
    let mut active = false;
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
                    word.push(c);
                }
                i = next_i;
            }
            Quote::Double(_) => {
                if c == '"' {
                    quote = Quote::None;
                    i = next_i;
                } else if c == '\\' {
                    if next_i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let next = input[next_i..].chars().next().unwrap();
                    let after_next = next_i + next.len_utf8();
                    if matches!(next, '"' | '\\' | '$') {
                        word.push(next);
                        i = after_next;
                    } else if next == '\n' {
                        i = after_next;
                    } else {
                        word.push('\\');
                        i = next_i;
                    }
                } else if c == '$' {
                    active = true;
                    let (end, value) = expand(input, i, env)?;
                    word.push_str(&value);
                    i = end;
                } else {
                    word.push(c);
                    i = next_i;
                }
            }
            Quote::None => {
                if matches!(c, ' ' | '\t' | '\r' | '\n') {
                    if active {
                        words.push(std::mem::take(&mut word));
                        active = false;
                    }
                    i = next_i;
                } else if c == '\'' {
                    active = true;
                    quote = Quote::Single(i);
                    i = next_i;
                } else if c == '"' {
                    active = true;
                    quote = Quote::Double(i);
                    i = next_i;
                } else if c == '\\' {
                    if next_i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let next = input[next_i..].chars().next().unwrap();
                    let after_next = next_i + next.len_utf8();
                    if next == '\n' {
                        i = after_next;
                    } else {
                        active = true;
                        word.push(next);
                        i = after_next;
                    }
                } else if c == '$' {
                    active = true;
                    let (end, value) = expand(input, i, env)?;
                    word.push_str(&value);
                    i = end;
                } else {
                    active = true;
                    word.push(c);
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

    if active {
        words.push(word);
    }
    Ok(words)
}

fn expand(input: &str, dollar: usize, env: &[(&str, &str)]) -> Result<(usize, String), SplitError> {
    let after_dollar = dollar + 1;
    if after_dollar >= input.len() {
        return Ok((after_dollar, "$".to_string()));
    }
    let first = input[after_dollar..].chars().next().unwrap();

    if first == '{' {
        let content_start = after_dollar + 1;
        let mut cursor = content_start;
        let mut close = None;
        while cursor < input.len() {
            let c = input[cursor..].chars().next().unwrap();
            let end = cursor + c.len_utf8();
            if c == '\\' {
                if end < input.len() {
                    let escaped = input[end..].chars().next().unwrap();
                    cursor = end + escaped.len_utf8();
                } else {
                    cursor = end;
                }
            } else if c == '}' {
                close = Some(cursor);
                break;
            } else {
                cursor = end;
            }
        }

        let close = close.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
        let body = &input[content_start..close];
        let (name_end, name) = parse_braced_name(body)
            .ok_or(SplitError::InvalidExpansion { pos: dollar })?;
        let rest = &body[name_end..];

        let value = if rest.is_empty() {
            lookup(env, name).unwrap_or("").to_string()
        } else if let Some(default) = rest.strip_prefix(":-") {
            match lookup(env, name) {
                Some(value) if !value.is_empty() => value.to_string(),
                _ => decode_default(default),
            }
        } else {
            return Err(SplitError::InvalidExpansion { pos: dollar });
        };

        Ok((close + 1, value))
    } else if is_name_start(first) {
        let mut end = after_dollar;
        for (offset, c) in input[after_dollar..].char_indices() {
            if is_name_continue(c) {
                end = after_dollar + offset + c.len_utf8();
            } else {
                break;
            }
        }
        let name = &input[after_dollar..end];
        Ok((end, lookup(env, name).unwrap_or("").to_string()))
    } else {
        Ok((after_dollar, "$".to_string()))
    }
}

fn parse_braced_name(body: &str) -> Option<(usize, &str)> {
    let first = body.chars().next()?;
    if !is_name_start(first) {
        return None;
    }
    let mut end = 0;
    for (offset, c) in body.char_indices() {
        if is_name_continue(c) {
            end = offset + c.len_utf8();
        } else {
            break;
        }
    }
    Some((end, &body[..end]))
}