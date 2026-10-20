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

fn parse_expansion(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
) -> Result<(String, usize), SplitError> {
    let after_dollar = dollar + 1;
    let rest = &input[after_dollar..];

    if rest.starts_with('{') {
        let content_start = after_dollar + 1;
        let mut close = None;
        let mut cursor = content_start;
        while cursor < input.len() {
            let c = input[cursor..].chars().next().unwrap();
            if c == '\\' {
                cursor += 1;
                if cursor < input.len() {
                    cursor += input[cursor..].chars().next().unwrap().len_utf8();
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

        let name_end = content
            .char_indices()
            .find_map(|(i, c)| if !is_name_continue(c) { Some(i) } else { None })
            .unwrap_or(content.len());

        if name_end == 0 {
            return Err(SplitError::InvalidExpansion { pos: dollar });
        }

        let name = &content[..name_end];
        let mut name_chars = name.chars();
        if !name_chars.next().map(is_name_start).unwrap_or(false)
            || !name_chars.all(is_name_continue)
        {
            return Err(SplitError::InvalidExpansion { pos: dollar });
        }

        let tail = &content[name_end..];
        if tail.is_empty() {
            let value = lookup(name, env).unwrap_or("");
            Ok((value.to_string(), close + 1))
        } else if let Some(default) = tail.strip_prefix(":-") {
            if let Some(value) = lookup(name, env).filter(|v| !v.is_empty()) {
                Ok((value.to_string(), close + 1))
            } else {
                Ok((decode_default(default), close + 1))
            }
        } else {
            Err(SplitError::InvalidExpansion { pos: dollar })
        }
    } else {
        let mut chars = rest.char_indices();
        let Some((_, first)) = chars.next() else {
            return Ok(("$".to_string(), after_dollar));
        };
        if !is_name_start(first) {
            return Ok(("$".to_string(), after_dollar));
        }

        let mut end = after_dollar + first.len_utf8();
        for (offset, c) in chars {
            if is_name_continue(c) {
                end = after_dollar + offset + c.len_utf8();
            } else {
                break;
            }
        }
        let name = &input[after_dollar..end];
        Ok((lookup(name, env).unwrap_or("").to_string(), end))
    }
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
        let c = input[cursor..].chars().next().unwrap();
        let next = cursor + c.len_utf8();

        match quote {
            Quote::Single(_) => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    word.push(c);
                }
                cursor = next;
            }
            Quote::Double(_) => {
                if c == '"' {
                    quote = Quote::None;
                    cursor = next;
                } else if c == '\\' {
                    if next == input.len() {
                        return Err(SplitError::TrailingEscape { pos: cursor });
                    }
                    let escaped = input[next..].chars().next().unwrap();
                    let after = next + escaped.len_utf8();
                    if matches!(escaped, '"' | '\\' | '$') {
                        word.push(escaped);
                        cursor = after;
                    } else if escaped == '\n' {
                        cursor = after;
                    } else {
                        word.push('\\');
                        cursor = next;
                    }
                } else if c == '$' {
                    let (value, after) = parse_expansion(input, cursor, env)?;
                    word.push_str(&value);
                    active = true;
                    cursor = after;
                } else {
                    word.push(c);
                    cursor = next;
                }
            }
            Quote::None => {
                if is_separator(c) {
                    if active {
                        words.push(std::mem::take(&mut word));
                        active = false;
                    }
                    cursor = next;
                } else if c == '\'' {
                    quote = Quote::Single(cursor);
                    active = true;
                    cursor = next;
                } else if c == '"' {
                    quote = Quote::Double(cursor);
                    active = true;
                    cursor = next;
                } else if c == '\\' {
                    if next == input.len() {
                        return Err(SplitError::TrailingEscape { pos: cursor });
                    }
                    let escaped = input[next..].chars().next().unwrap();
                    let after = next + escaped.len_utf8();
                    if escaped == '\n' {
                        cursor = after;
                    } else {
                        word.push(escaped);
                        active = true;
                        cursor = after;
                    }
                } else if c == '$' {
                    let (value, after) = parse_expansion(input, cursor, env)?;
                    word.push_str(&value);
                    active = true;
                    cursor = after;
                } else {
                    word.push(c);
                    active = true;
                    cursor = next;
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
