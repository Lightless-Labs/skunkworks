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
    c.is_ascii_alphanumeric() || c == '_'
}

fn lookup<'a>(name: &str, env: &'a [(&str, &str)]) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| if *key == name { Some(*value) } else { None })
}

fn decode_default(text: &str) -> String {
    let mut result = String::new();
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
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
    if let Some((pos, _)) = input.char_indices().find(|(_, c)| *c == '\0') {
        return Err(SplitError::Nul { pos });
    }

    let mut words = Vec::new();
    let mut word = String::new();
    let mut active = false;
    let mut quote = Quote::None;
    let mut i = 0;

    while i < input.len() {
        let c = input[i..].chars().next().expect("valid character boundary");
        let width = c.len_utf8();

        match quote {
            Quote::Single(_) => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    word.push(c);
                }
                i += width;
            }
            Quote::Double(_) => {
                if c == '"' {
                    quote = Quote::None;
                    i += width;
                } else if c == '\\' {
                    let slash_pos = i;
                    i += width;
                    if i >= input.len() {
                        return Err(SplitError::TrailingEscape { pos: slash_pos });
                    }
                    let next = input[i..].chars().next().expect("valid character boundary");
                    if matches!(next, '"' | '\\' | '$') {
                        word.push(next);
                        active = true;
                        i += next.len_utf8();
                    } else if next == '\n' {
                        i += next.len_utf8();
                    } else {
                        word.push('\\');
                    }
                } else if c == '$' {
                    let (consumed, value, expanded) = parse_expansion(input, i, env)?;
                    if expanded {
                        word.push_str(&value);
                        active = true;
                        i += consumed;
                    } else {
                        word.push('$');
                        active = true;
                        i += width;
                    }
                } else {
                    word.push(c);
                    active = true;
                    i += width;
                }
            }
            Quote::None => {
                if is_separator(c) {
                    if active {
                        words.push(std::mem::take(&mut word));
                        active = false;
                    }
                    i += width;
                } else if c == '\'' {
                    quote = Quote::Single(i);
                    active = true;
                    i += width;
                } else if c == '"' {
                    quote = Quote::Double(i);
                    active = true;
                    i += width;
                } else if c == '\\' {
                    let slash_pos = i;
                    i += width;
                    if i >= input.len() {
                        return Err(SplitError::TrailingEscape { pos: slash_pos });
                    }
                    let next = input[i..].chars().next().expect("valid character boundary");
                    if next == '\n' {
                        i += next.len_utf8();
                    } else {
                        word.push(next);
                        active = true;
                        i += next.len_utf8();
                    }
                } else if c == '$' {
                    let (consumed, value, expanded) = parse_expansion(input, i, env)?;
                    if expanded {
                        word.push_str(&value);
                        active = true;
                        i += consumed;
                    } else {
                        word.push('$');
                        active = true;
                        i += width;
                    }
                } else {
                    word.push(c);
                    active = true;
                    i += width;
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

fn parse_expansion(
    input: &str,
    dollar_pos: usize,
    env: &[(&str, &str)],
) -> Result<(usize, String, bool), SplitError> {
    let after_dollar = dollar_pos + 1;
    if after_dollar >= input.len() {
        return Ok((1, String::new(), false));
    }

    let next = input[after_dollar..]
        .chars()
        .next()
        .expect("valid character boundary");

    if next == '{' {
        let content_start = after_dollar + 1;
        let mut cursor = content_start;
        let mut closing = None;

        while cursor < input.len() {
            let c = input[cursor..]
                .chars()
                .next()
                .expect("valid character boundary");
            if c == '\\' {
                cursor += c.len_utf8();
                if cursor < input.len() {
                    let escaped = input[cursor..]
                        .chars()
                        .next()
                        .expect("valid character boundary");
                    cursor += escaped.len_utf8();
                }
            } else if c == '}' {
                closing = Some(cursor);
                break;
            } else {
                cursor += c.len_utf8();
            }
        }

        let close_pos = closing.ok_or(SplitError::UnterminatedExpansion { pos: dollar_pos })?;
        let contents = &input[content_start..close_pos];

        let name_len = contents
            .char_indices()
            .find_map(|(offset, c)| {
                if (offset == 0 && is_name_start(c)) || (offset > 0 && is_name_continue(c)) {
                    None
                } else {
                    Some(offset)
                }
            })
            .unwrap_or(contents.len());

        if name_len == 0 {
            return Err(SplitError::InvalidExpansion { pos: dollar_pos });
        }

        let name = &contents[..name_len];
        let rest = &contents[name_len..];

        let selected = if rest.is_empty() {
            lookup(name, env).unwrap_or("").to_string()
        } else if let Some(default_text) = rest.strip_prefix(":-") {
            match lookup(name, env) {
                Some(value) if !value.is_empty() => value.to_string(),
                _ => decode_default(default_text),
            }
        } else {
            return Err(SplitError::InvalidExpansion { pos: dollar_pos });
        };

        return Ok((close_pos + 1 - dollar_pos, selected, true));
    }

    if is_name_start(next) {
        let mut end = after_dollar;
        for (offset, c) in input[after_dollar..].char_indices() {
            if is_name_continue(c) {
                end = after_dollar + offset + c.len_utf8();
            } else {
                break;
            }
        }
        let name = &input[after_dollar..end];
        return Ok((end - dollar_pos, lookup(name, env).unwrap_or("").to_string(), true));
    }

    Ok((1, String::new(), false))
}