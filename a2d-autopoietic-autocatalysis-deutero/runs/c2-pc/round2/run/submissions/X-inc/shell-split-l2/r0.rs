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
        .find_map(|(key, value)| (*key == name).then_some(*value))
}

fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
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
        let c = input[i..].chars().next().expect("valid character boundary");
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
            Quote::Double(open_pos) => {
                if c == '"' {
                    quote = Quote::None;
                    i = next_i;
                } else if c == '\\' {
                    if next_i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let escaped = input[next_i..].chars().next().expect("character after slash");
                    let after_escaped = next_i + escaped.len_utf8();
                    if matches!(escaped, '"' | '\\' | '$') {
                        word.push(escaped);
                        i = after_escaped;
                    } else if escaped == '\n' {
                        i = after_escaped;
                    } else {
                        word.push('\\');
                        i = next_i;
                    }
                } else if c == '$' {
                    active = true;
                    let (end, value) = expand(input, i, env)?;
                    if let Some(value) = value {
                        word.push_str(&value);
                        i = end;
                    } else {
                        word.push('$');
                        i = next_i;
                    }
                } else {
                    word.push(c);
                    i = next_i;
                }

                let _ = open_pos;
            }
            Quote::None => {
                if is_separator(c) {
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
                    let escaped = input[next_i..].chars().next().expect("character after slash");
                    let after_escaped = next_i + escaped.len_utf8();
                    if escaped == '\n' {
                        i = after_escaped;
                    } else {
                        active = true;
                        word.push(escaped);
                        i = after_escaped;
                    }
                } else if c == '$' {
                    active = true;
                    let (end, value) = expand(input, i, env)?;
                    if let Some(value) = value {
                        word.push_str(&value);
                        i = end;
                    } else {
                        word.push('$');
                        i = next_i;
                    }
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

fn expand(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
) -> Result<(usize, Option<String>), SplitError> {
    let after_dollar = dollar + 1;
    if after_dollar >= input.len() {
        return Ok((after_dollar, None));
    }

    let first = input[after_dollar..]
        .chars()
        .next()
        .expect("character after dollar");

    if first == '{' {
        let content_start = after_dollar + 1;
        let mut cursor = content_start;
        let mut closing = None;

        while cursor < input.len() {
            let c = input[cursor..].chars().next().expect("valid boundary");
            let next = cursor + c.len_utf8();
            if c == '\\' {
                if next < input.len() {
                    let escaped = input[next..].chars().next().expect("escaped character");
                    cursor = next + escaped.len_utf8();
                } else {
                    cursor = next;
                }
            } else if c == '}' {
                closing = Some(cursor);
                break;
            } else {
                cursor = next;
            }
        }

        let close = closing.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
        let contents = &input[content_start..close];

        let (name_end, name) = parse_braced_name(contents)
            .ok_or(SplitError::InvalidExpansion { pos: dollar })?;

        let default = if name_end == contents.len() {
            None
        } else if contents[name_end..].starts_with(":-") {
            Some(&contents[name_end + 2..])
        } else {
            return Err(SplitError::InvalidExpansion { pos: dollar });
        };

        let value = if let Some(default_text) = default {
            match lookup(env, name) {
                Some(found) if !found.is_empty() => found.to_string(),
                _ => decode_default(default_text),
            }
        } else {
            lookup(env, name).unwrap_or("").to_string()
        };

        Ok((close + 1, Some(value)))
    } else if is_name_start(first) {
        let mut end = after_dollar;
        while end < input.len() {
            let c = input[end..].chars().next().expect("valid boundary");
            if is_name_continue(c) {
                end += c.len_utf8();
            } else {
                break;
            }
        }
        let name = &input[after_dollar..end];
        Ok((end, Some(lookup(env, name).unwrap_or("").to_string())))
    } else {
        Ok((after_dollar, None))
    }
}

fn parse_braced_name(contents: &str) -> Option<(usize, &str)> {
    let first = contents.chars().next()?;
    if !is_name_start(first) {
        return None;
    }

    let mut end = first.len_utf8();
    while end < contents.len() {
        let c = contents[end..].chars().next()?;
        if is_name_continue(c) {
            end += c.len_utf8();
        } else {
            break;
        }
    }

    Some((end, &contents[..end]))
}

fn decode_default(text: &str) -> String {
    let mut decoded = String::new();
    let mut i = 0;
    while i < text.len() {
        let c = text[i..].chars().next().expect("valid boundary");
        let next = i + c.len_utf8();
        if c == '\\' && next < text.len() {
            let escaped = text[next..].chars().next().expect("escaped character");
            decoded.push(escaped);
            i = next + escaped.len_utf8();
        } else {
            decoded.push(c);
            i = next;
        }
    }
    decoded
}