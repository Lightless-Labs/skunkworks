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

fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_name_continue(c: char) -> bool {
    is_name_start(c) || c.is_ascii_digit()
}

fn lookup<'a>(name: &str, env: &'a [(&str, &str)]) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| if *key == name { Some(*value) } else { None })
}

fn decode_default(text: &str) -> String {
    let mut result = String::new();
    let mut chars = text.char_indices().peekable();

    while let Some((_, ch)) = chars.next() {
        if ch == '\\' {
            if let Some((_, next)) = chars.next() {
                result.push(next);
            } else {
                result.push('\\');
            }
        } else {
            result.push(ch);
        }
    }

    result
}

fn parse_braced(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
) -> Result<(String, usize), SplitError> {
    let after_open = dollar + 2;
    let mut closing = None;
    let mut cursor = after_open;

    while cursor < input.len() {
        let ch = input[cursor..].chars().next().unwrap();
        if ch == '\\' {
            cursor += ch.len_utf8();
            if cursor < input.len() {
                cursor += input[cursor..].chars().next().unwrap().len_utf8();
            }
        } else if ch == '}' {
            closing = Some(cursor);
            break;
        } else {
            cursor += ch.len_utf8();
        }
    }

    let close_pos = closing.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
    let contents = &input[after_open..close_pos];

    let mut name_end = 0;
    let mut content_chars = contents.char_indices();
    match content_chars.next() {
        Some((_, first)) if is_name_start(first) => {
            name_end = first.len_utf8();
            while name_end < contents.len() {
                let ch = contents[name_end..].chars().next().unwrap();
                if is_name_continue(ch) {
                    name_end += ch.len_utf8();
                } else {
                    break;
                }
            }
        }
        _ => return Err(SplitError::InvalidExpansion { pos: dollar }),
    }

    let name = &contents[..name_end];
    let remainder = &contents[name_end..];

    if remainder.is_empty() {
        Ok((lookup(name, env).unwrap_or("").to_string(), close_pos + 1))
    } else if let Some(default) = remainder.strip_prefix(":-") {
        let value = lookup(name, env).unwrap_or("");
        if value.is_empty() {
            Ok((decode_default(default), close_pos + 1))
        } else {
            Ok((value.to_string(), close_pos + 1))
        }
    } else {
        Err(SplitError::InvalidExpansion { pos: dollar })
    }
}

fn parse_dollar(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
) -> Result<Option<(String, usize)>, SplitError> {
    let next_pos = dollar + 1;
    if next_pos >= input.len() {
        return Ok(None);
    }

    let next = input[next_pos..].chars().next().unwrap();
    if next == '{' {
        return parse_braced(input, dollar, env).map(Some);
    }

    if !is_name_start(next) {
        return Ok(None);
    }

    let mut end = next_pos + next.len_utf8();
    while end < input.len() {
        let ch = input[end..].chars().next().unwrap();
        if is_name_continue(ch) {
            end += ch.len_utf8();
        } else {
            break;
        }
    }

    Ok(Some((
        lookup(&input[next_pos..end], env).unwrap_or("").to_string(),
        end,
    )))
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<String>, SplitError> {
    if let Some(pos) = input.find('\0') {
        return Err(SplitError::Nul { pos });
    }

    let mut words = Vec::new();
    let mut word = String::new();
    let mut active = false;
    let mut quote = Quote::None;
    let mut cursor = 0;

    while cursor < input.len() {
        let ch = input[cursor..].chars().next().unwrap();
        let width = ch.len_utf8();

        match quote {
            Quote::Single(_) => {
                if ch == '\'' {
                    quote = Quote::None;
                } else {
                    word.push(ch);
                }
                cursor += width;
            }
            Quote::Double(open_pos) => {
                if ch == '"' {
                    quote = Quote::None;
                    cursor += width;
                } else if ch == '\\' {
                    let escape_pos = cursor;
                    cursor += width;
                    if cursor >= input.len() {
                        return Err(SplitError::TrailingEscape { pos: escape_pos });
                    }
                    let next = input[cursor..].chars().next().unwrap();
                    match next {
                        '"' | '\\' | '$' => {
                            word.push(next);
                            cursor += next.len_utf8();
                        }
                        '\n' => cursor += next.len_utf8(),
                        _ => word.push('\\'),
                    }
                } else if ch == '$' {
                    if let Some((value, end)) = parse_dollar(input, cursor, env)? {
                        active = true;
                        word.push_str(&value);
                        cursor = end;
                    } else {
                        word.push('$');
                        cursor += width;
                    }
                } else {
                    word.push(ch);
                    cursor += width;
                }
                let _ = open_pos;
            }
            Quote::None => {
                if is_separator(ch) {
                    if active {
                        words.push(std::mem::take(&mut word));
                        active = false;
                    }
                    cursor += width;
                } else if ch == '\'' {
                    active = true;
                    quote = Quote::Single(cursor);
                    cursor += width;
                } else if ch == '"' {
                    active = true;
                    quote = Quote::Double(cursor);
                    cursor += width;
                } else if ch == '\\' {
                    let escape_pos = cursor;
                    cursor += width;
                    if cursor >= input.len() {
                        return Err(SplitError::TrailingEscape { pos: escape_pos });
                    }
                    let next = input[cursor..].chars().next().unwrap();
                    if next == '\n' {
                        cursor += next.len_utf8();
                    } else {
                        active = true;
                        word.push(next);
                        cursor += next.len_utf8();
                    }
                } else if ch == '$' {
                    if let Some((value, end)) = parse_dollar(input, cursor, env)? {
                        active = true;
                        word.push_str(&value);
                        cursor = end;
                    } else {
                        active = true;
                        word.push('$');
                        cursor += width;
                    }
                } else {
                    active = true;
                    word.push(ch);
                    cursor += width;
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