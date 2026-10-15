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
    Single { start: usize },
    Double { start: usize },
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
        .find_map(|(key, value)| (*key == name).then_some(*value))
}

fn decode_default(text: &str) -> String {
    let mut result = String::new();
    let mut chars = text.char_indices().peekable();

    while let Some((_, c)) = chars.next() {
        if c == '\\' {
            if let Some((_, next)) = chars.next() {
                result.push(next);
            } else {
                result.push('\\');
            }
        } else {
            result.push(c);
        }
    }

    result
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
            Quote::Single { .. } => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    word.push(c);
                }
                i = next_i;
            }
            Quote::Double { .. } => {
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
                    if next_i < input.len() {
                        let next = input[next_i..].chars().next().unwrap();
                        if next == '{' {
                            let (end, value) = parse_braced(input, i, env)?;
                            word.push_str(&value);
                            i = end;
                        } else if is_name_start(next) {
                            let (end, value) = parse_named(input, next_i, env);
                            word.push_str(&value);
                            i = end;
                        } else {
                            word.push('$');
                            i = next_i;
                        }
                    } else {
                        word.push('$');
                        i = next_i;
                    }
                } else {
                    word.push(c);
                    i = next_i;
                }
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
                    quote = Quote::Single { start: i };
                    i = next_i;
                } else if c == '"' {
                    active = true;
                    quote = Quote::Double { start: i };
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
                    if next_i < input.len() {
                        let next = input[next_i..].chars().next().unwrap();
                        if next == '{' {
                            let (end, value) = parse_braced(input, i, env)?;
                            word.push_str(&value);
                            i = end;
                        } else if is_name_start(next) {
                            let (end, value) = parse_named(input, next_i, env);
                            word.push_str(&value);
                            i = end;
                        } else {
                            word.push('$');
                            i = next_i;
                        }
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
        Quote::Single { start } => {
            return Err(SplitError::UnterminatedSingle { pos: start });
        }
        Quote::Double { start } => {
            return Err(SplitError::UnterminatedDouble { pos: start });
        }
        Quote::None => {}
    }

    if active {
        words.push(word);
    }

    Ok(words)
}

fn parse_named(input: &str, name_start: usize, env: &[(&str, &str)]) -> (usize, String) {
    let mut end = name_start;
    while end < input.len() {
        let c = input[end..].chars().next().unwrap();
        if !is_name_continue(c) {
            break;
        }
        end += c.len_utf8();
    }

    let name = &input[name_start..end];
    (end, lookup(name, env).unwrap_or("").to_string())
}

fn parse_braced(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
) -> Result<(usize, String), SplitError> {
    let content_start = dollar + 2;
    let mut cursor = content_start;
    let mut close = None;

    while cursor < input.len() {
        let c = input[cursor..].chars().next().unwrap();
        if c == '\\' {
            cursor += c.len_utf8();
            if cursor < input.len() {
                let next = input[cursor..].chars().next().unwrap();
                cursor += next.len_utf8();
            }
        } else if c == '}' {
            close = Some(cursor);
            break;
        } else {
            cursor += c.len_utf8();
        }
    }

    let close = close.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
    let content = &input[content_start..close];

    let mut name_end = 0;
    let mut chars = content.char_indices();
    match chars.next() {
        Some((_, first)) if is_name_start(first) => {
            name_end = first.len_utf8();
        }
        _ => return Err(SplitError::InvalidExpansion { pos: dollar }),
    }

    while name_end < content.len() {
        let c = content[name_end..].chars().next().unwrap();
        if !is_name_continue(c) {
            break;
        }
        name_end += c.len_utf8();
    }

    let name = &content[..name_end];
    let remainder = &content[name_end..];

    let value = if remainder.is_empty() {
        lookup(name, env).unwrap_or("").to_string()
    } else if let Some(default) = remainder.strip_prefix(":-") {
        match lookup(name, env) {
            Some(value) if !value.is_empty() => value.to_string(),
            _ => decode_default(default),
        }
    } else {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    };

    Ok((close + 1, value))
}

fn main() {}