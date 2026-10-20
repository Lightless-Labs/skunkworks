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
    is_name_start(c) || c.is_ascii_digit()
}

fn lookup<'a>(name: &str, env: &'a [(&str, &str)]) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| (*key == name).then_some(*value))
}

fn next_char(input: &str, pos: usize) -> Option<(char, usize)> {
    input[pos..]
        .chars()
        .next()
        .map(|c| (c, pos + c.len_utf8()))
}

fn expand(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
) -> Result<(String, usize), SplitError> {
    let after_dollar = dollar + 1;
    let Some((next, next_end)) = next_char(input, after_dollar) else {
        return Ok(("$".to_string(), after_dollar));
    };

    if next == '{' {
        let content_start = next_end;
        let mut cursor = content_start;
        let mut close = None;

        while let Some((c, end)) = next_char(input, cursor) {
            if c == '\\' {
                if let Some((_, escaped_end)) = next_char(input, end) {
                    cursor = escaped_end;
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

        let Some(close_pos) = close else {
            return Err(SplitError::UnterminatedExpansion { pos: dollar });
        };

        let contents = &input[content_start..close_pos];
        let mut name_end = 0;
        let mut chars = contents.char_indices();
        let valid_start = chars
            .next()
            .map(|(_, c)| is_name_start(c))
            .unwrap_or(false);

        if valid_start {
            for (offset, c) in contents.char_indices() {
                if !is_name_continue(c) {
                    name_end = offset;
                    break;
                }
                name_end = offset + c.len_utf8();
            }
        }

        if !valid_start {
            return Err(SplitError::InvalidExpansion { pos: dollar });
        }

        let name = &contents[..name_end];
        let rest = &contents[name_end..];

        let value = if rest.is_empty() {
            lookup(name, env).unwrap_or("").to_string()
        } else if let Some(default) = rest.strip_prefix(":-") {
            match lookup(name, env) {
                Some(value) if !value.is_empty() => value.to_string(),
                _ => decode_default(default),
            }
        } else {
            return Err(SplitError::InvalidExpansion { pos: dollar });
        };

        return Ok((value, close_pos + 1));
    }

    if is_name_start(next) {
        let mut end = next_end;
        while let Some((c, char_end)) = next_char(input, end) {
            if !is_name_continue(c) {
                break;
            }
            end = char_end;
        }
        return Ok((
            lookup(&input[after_dollar..end], env).unwrap_or("").to_string(),
            end,
        ));
    }

    Ok(("$".to_string(), after_dollar))
}

fn decode_default(text: &str) -> String {
    let mut out = String::new();
    let mut cursor = 0;
    while let Some((c, end)) = next_char(text, cursor) {
        if c == '\\' {
            if let Some((escaped, escaped_end)) = next_char(text, end) {
                out.push(escaped);
                cursor = escaped_end;
            } else {
                out.push('\\');
                cursor = end;
            }
        } else {
            out.push(c);
            cursor = end;
        }
    }
    out
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<String>, SplitError> {
    if let Some(pos) = input.as_bytes().iter().position(|&b| b == 0) {
        return Err(SplitError::Nul { pos });
    }

    let mut words = Vec::new();
    let mut word = String::new();
    let mut active = false;
    let mut quote = Quote::None;
    let mut pos = 0;

    while let Some((c, end)) = next_char(input, pos) {
        match quote {
            Quote::Single(_) => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    word.push(c);
                }
                pos = end;
            }
            Quote::Double(opening) => {
                if c == '"' {
                    quote = Quote::None;
                    pos = end;
                } else if c == '\\' {
                    let Some((next, next_end)) = next_char(input, end) else {
                        return Err(SplitError::TrailingEscape { pos });
                    };
                    if next == '"' || next == '\\' || next == '$' {
                        word.push(next);
                        pos = next_end;
                    } else if next == '\n' {
                        pos = next_end;
                    } else {
                        word.push('\\');
                        pos = end;
                    }
                } else if c == '$' {
                    let (value, next_pos) = expand(input, pos, env)?;
                    word.push_str(&value);
                    active = true;
                    pos = next_pos;
                } else {
                    word.push(c);
                    pos = end;
                }
                let _ = opening;
            }
            Quote::None => {
                if c == ' ' || c == '\t' || c == '\r' || c == '\n' {
                    if active {
                        words.push(std::mem::take(&mut word));
                        active = false;
                    }
                    pos = end;
                } else if c == '\'' {
                    quote = Quote::Single(pos);
                    active = true;
                    pos = end;
                } else if c == '"' {
                    quote = Quote::Double(pos);
                    active = true;
                    pos = end;
                } else if c == '\\' {
                    let Some((next, next_end)) = next_char(input, end) else {
                        return Err(SplitError::TrailingEscape { pos });
                    };
                    if next != '\n' {
                        word.push(next);
                        active = true;
                    }
                    pos = next_end;
                } else if c == '$' {
                    let (value, next_pos) = expand(input, pos, env)?;
                    word.push_str(&value);
                    active = true;
                    pos = next_pos;
                } else {
                    word.push(c);
                    active = true;
                    pos = end;
                }
            }
        }
    }

    match quote {
        Quote::Single(opening) => {
            return Err(SplitError::UnterminatedSingle { pos: opening });
        }
        Quote::Double(opening) => {
            return Err(SplitError::UnterminatedDouble { pos: opening });
        }
        Quote::None => {}
    }

    if active {
        words.push(word);
    }
    Ok(words)
}