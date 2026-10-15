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
    Single { open: usize },
    Double { open: usize },
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn lookup<'a>(name: &str, env: &'a [(&str, &str)]) -> &'a str {
    env.iter()
        .find_map(|(key, value)| (*key == name).then_some(*value))
        .unwrap_or("")
}

fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<String>, SplitError> {
    if let Some(pos) = input.as_bytes().iter().position(|&b| b == 0) {
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
                    let slash = i;
                    if next_i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: slash });
                    }
                    let escaped = input[next_i..].chars().next().unwrap();
                    let after = next_i + escaped.len_utf8();
                    if matches!(escaped, '"' | '\\' | '$') {
                        word.push(escaped);
                        i = after;
                    } else if escaped == '\n' {
                        i = after;
                    } else {
                        word.push('\\');
                        i = next_i;
                    }
                } else if c == '$' {
                    active = true;
                    let (value, end) = parse_expansion(input, i, env)?;
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
                    quote = Quote::Single { open: i };
                    active = true;
                    i = next_i;
                } else if c == '"' {
                    quote = Quote::Double { open: i };
                    active = true;
                    i = next_i;
                } else if c == '\\' {
                    let slash = i;
                    if next_i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: slash });
                    }
                    let escaped = input[next_i..].chars().next().unwrap();
                    let after = next_i + escaped.len_utf8();
                    if escaped == '\n' {
                        i = after;
                    } else {
                        word.push(escaped);
                        active = true;
                        i = after;
                    }
                } else if c == '$' {
                    let (value, end, did_expand) = parse_expansion_with_flag(input, i, env)?;
                    if did_expand {
                        active = true;
                        word.push_str(&value);
                        i = end;
                    } else {
                        word.push('$');
                        i = next_i;
                    }
                } else {
                    word.push(c);
                    active = true;
                    i = next_i;
                }
            }
        }
    }

    match quote {
        Quote::Single { open } => return Err(SplitError::UnterminatedSingle { pos: open }),
        Quote::Double { open } => return Err(SplitError::UnterminatedDouble { pos: open }),
        Quote::None => {}
    }

    if active {
        words.push(word);
    }
    Ok(words)
}

fn parse_expansion(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
) -> Result<(String, usize), SplitError> {
    let (value, end, _) = parse_expansion_with_flag(input, dollar, env)?;
    Ok((value, end))
}

fn parse_expansion_with_flag(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
) -> Result<(String, usize, bool), SplitError> {
    let after_dollar = dollar + 1;
    if after_dollar >= input.len() {
        return Ok((String::new(), after_dollar, false));
    }

    let following = input[after_dollar..].chars().next().unwrap();
    if following == '{' {
        let content_start = after_dollar + 1;
        let mut cursor = content_start;
        let mut close = None;

        while cursor < input.len() {
            let c = input[cursor..].chars().next().unwrap();
            let next = cursor + c.len_utf8();
            if c == '\\' {
                if next < input.len() {
                    let escaped = input[next..].chars().next().unwrap();
                    cursor = next + escaped.len_utf8();
                } else {
                    cursor = next;
                }
            } else if c == '}' {
                close = Some(cursor);
                break;
            } else {
                cursor = next;
            }
        }

        let close = close.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
        let contents = &input[content_start..close];
        let (name, default) = validate_braced(contents)
            .ok_or(SplitError::InvalidExpansion { pos: dollar })?;

        let value = match default {
            None => lookup(name, env).to_string(),
            Some(text) => {
                let found = lookup(name, env);
                if found.is_empty() {
                    decode_default(text)
                } else {
                    found.to_string()
                }
            }
        };
        return Ok((value, close + 1, true));
    }

    if !is_name_start(following) {
        return Ok((String::new(), after_dollar, false));
    }

    let mut end = after_dollar;
    while end < input.len() {
        let c = input[end..].chars().next().unwrap();
        if is_name_char(c) {
            end += c.len_utf8();
        } else {
            break;
        }
    }
    let name = &input[after_dollar..end];
    Ok((lookup(name, env).to_string(), end, true))
}

fn validate_braced(contents: &str) -> Option<(&str, Option<&str>)> {
    let mut name_end = 0;
    for c in contents.chars() {
        if is_name_char(c) {
            name_end += c.len_utf8();
        } else {
            break;
        }
    }

    if name_end == 0 {
        return None;
    }
    let name = &contents[..name_end];
    if !is_name_start(name.chars().next()?) {
        return None;
    }

    let rest = &contents[name_end..];
    if rest.is_empty() {
        Some((name, None))
    } else if let Some(text) = rest.strip_prefix(":-") {
        Some((name, Some(text)))
    } else {
        None
    }
}

fn decode_default(text: &str) -> String {
    let mut result = String::new();
    let mut i = 0;
    while i < text.len() {
        let c = text[i..].chars().next().unwrap();
        let next = i + c.len_utf8();
        if c == '\\' && next < text.len() {
            let escaped = text[next..].chars().next().unwrap();
            result.push(escaped);
            i = next + escaped.len_utf8();
        } else {
            result.push(c);
            i = next;
        }
    }
    result
}

fn main() {}