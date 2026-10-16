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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
        .find_map(|(key, value)| (*key == name).then_some(*value))
}

fn append_literal(word: &mut Word, c: char, active_glob: bool) {
    let offset = word.text.len();
    word.text.push(c);
    if active_glob && matches!(c, '*' | '?' | '[') {
        word.glob.push(offset);
    }
}

fn append_expansion(word: &mut Word, value: &str, active_glob: bool) {
    let base = word.text.len();
    word.text.push_str(value);
    if active_glob {
        for (offset, c) in value.char_indices() {
            if matches!(c, '*' | '?' | '[') {
                word.glob.push(base + offset);
            }
        }
    }
}

fn decode_default(text: &str) -> String {
    let mut decoded = String::new();
    let mut chars = text.char_indices().peekable();

    while let Some((_, c)) = chars.next() {
        if c == '\\' {
            if let Some((_, next)) = chars.next() {
                decoded.push(next);
            } else {
                decoded.push('\\');
            }
        } else {
            decoded.push(c);
        }
    }

    decoded
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
    let mut active = false;
    let mut quote = Quote::None;
    let mut pos = 0;

    while pos < input.len() {
        let c = input[pos..].chars().next().unwrap();
        let next_pos = pos + c.len_utf8();

        match quote {
            Quote::Single(_) => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    append_literal(&mut current, c, false);
                }
                pos = next_pos;
            }
            Quote::Double(_) => {
                match c {
                    '"' => {
                        quote = Quote::None;
                        pos = next_pos;
                    }
                    '\\' => {
                        if next_pos == input.len() {
                            return Err(SplitError::TrailingEscape { pos });
                        }
                        let escaped = input[next_pos..].chars().next().unwrap();
                        let escaped_end = next_pos + escaped.len_utf8();
                        if matches!(escaped, '"' | '\\' | '$' | '\n') {
                            if escaped != '\n' {
                                append_literal(&mut current, escaped, false);
                            }
                            pos = escaped_end;
                        } else {
                            append_literal(&mut current, '\\', false);
                            pos = next_pos;
                        }
                    }
                    '$' => {
                        if input[next_pos..].starts_with('(') {
                            return Err(SplitError::CommandSubstitution { pos });
                        }
                        if input[next_pos..].starts_with('{') {
                            let dollar_pos = pos;
                            let content_start = next_pos + 1;
                            let mut cursor = content_start;
                            let mut closing = None;
                            while cursor < input.len() {
                                let ch = input[cursor..].chars().next().unwrap();
                                let end = cursor + ch.len_utf8();
                                if ch == '\\' {
                                    if end < input.len() {
                                        let skipped = input[end..].chars().next().unwrap();
                                        cursor = end + skipped.len_utf8();
                                    } else {
                                        cursor = end;
                                    }
                                } else if ch == '}' {
                                    closing = Some(cursor);
                                    break;
                                } else {
                                    cursor = end;
                                }
                            }

                            let Some(close_pos) = closing else {
                                return Err(SplitError::UnterminatedExpansion { pos: dollar_pos });
                            };
                            let contents = &input[content_start..close_pos];
                            let (name, default) = parse_braced_contents(contents)
                                .ok_or(SplitError::InvalidExpansion { pos: dollar_pos })?;
                            let selected = match default {
                                Some(default_text) => match lookup(name, env) {
                                    Some(value) if !value.is_empty() => value.to_string(),
                                    _ => decode_default(default_text),
                                },
                                None => lookup(name, env).unwrap_or("").to_string(),
                            };
                            active = true;
                            append_expansion(&mut current, &selected, false);
                            pos = close_pos + 1;
                        } else if let Some((name, end)) = parse_dollar_name(input, pos) {
                            active = true;
                            append_expansion(
                                &mut current,
                                lookup(name, env).unwrap_or(""),
                                false,
                            );
                            pos = end;
                        } else {
                            append_literal(&mut current, '$', false);
                            pos = next_pos;
                        }
                    }
                    '`' => return Err(SplitError::CommandSubstitution { pos }),
                    _ => {
                        append_literal(&mut current, c, false);
                        pos = next_pos;
                    }
                }
            }
            Quote::None => {
                if is_separator(c) {
                    if active {
                        words.push(current);
                        current = Word {
                            text: String::new(),
                            glob: Vec::new(),
                        };
                        active = false;
                    }
                    pos = next_pos;
                } else {
                    match c {
                        '\'' => {
                            active = true;
                            quote = Quote::Single(pos);
                            pos = next_pos;
                        }
                        '"' => {
                            active = true;
                            quote = Quote::Double(pos);
                            pos = next_pos;
                        }
                        '\\' => {
                            if next_pos == input.len() {
                                return Err(SplitError::TrailingEscape { pos });
                            }
                            let escaped = input[next_pos..].chars().next().unwrap();
                            let escaped_end = next_pos + escaped.len_utf8();
                            if escaped != '\n' {
                                active = true;
                                append_literal(&mut current, escaped, false);
                            }
                            pos = escaped_end;
                        }
                        '$' => {
                            if input[next_pos..].starts_with('(') {
                                return Err(SplitError::CommandSubstitution { pos });
                            }
                            if input[next_pos..].starts_with('{') {
                                let dollar_pos = pos;
                                let content_start = next_pos + 1;
                                let mut cursor = content_start;
                                let mut closing = None;
                                while cursor < input.len() {
                                    let ch = input[cursor..].chars().next().unwrap();
                                    let end = cursor + ch.len_utf8();
                                    if ch == '\\' {
                                        if end < input.len() {
                                            let skipped = input[end..].chars().next().unwrap();
                                            cursor = end + skipped.len_utf8();
                                        } else {
                                            cursor = end;
                                        }
                                    } else if ch == '}' {
                                        closing = Some(cursor);
                                        break;
                                    } else {
                                        cursor = end;
                                    }
                                }

                                let Some(close_pos) = closing else {
                                    return Err(SplitError::UnterminatedExpansion { pos: dollar_pos });
                                };
                                let contents = &input[content_start..close_pos];
                                let (name, default) = parse_braced_contents(contents)
                                    .ok_or(SplitError::InvalidExpansion { pos: dollar_pos })?;
                                let selected = match default {
                                    Some(default_text) => match lookup(name, env) {
                                        Some(value) if !value.is_empty() => value.to_string(),
                                        _ => decode_default(default_text),
                                    },
                                    None => lookup(name, env).unwrap_or("").to_string(),
                                };
                                active = true;
                                append_expansion(&mut current, &selected, true);
                                pos = close_pos + 1;
                            } else if let Some((name, end)) = parse_dollar_name(input, pos) {
                                active = true;
                                append_expansion(
                                    &mut current,
                                    lookup(name, env).unwrap_or(""),
                                    true,
                                );
                                pos = end;
                            } else {
                                active = true;
                                append_literal(&mut current, '$', false);
                                pos = next_pos;
                            }
                        }
                        '`' => return Err(SplitError::CommandSubstitution { pos }),
                        _ => {
                            active = true;
                            append_literal(&mut current, c, true);
                            pos = next_pos;
                        }
                    }
                }
            }
        }
    }

    match quote {
        Quote::Single(open) => return Err(SplitError::UnterminatedSingle { pos: open }),
        Quote::Double(open) => return Err(SplitError::UnterminatedDouble { pos: open }),
        Quote::None => {}
    }

    if active {
        words.push(current);
    }
    Ok(words)
}

fn parse_dollar_name(input: &str, dollar_pos: usize) -> Option<(&str, usize)> {
    let start = dollar_pos + 1;
    let first = input.get(start..)?.chars().next()?;
    if !is_name_start(first) {
        return None;
    }

    let mut end = start + first.len_utf8();
    while end < input.len() {
        let c = input[end..].chars().next().unwrap();
        if !is_name_continue(c) {
            break;
        }
        end += c.len_utf8();
    }
    Some((&input[start..end], end))
}

fn parse_braced_contents(contents: &str) -> Option<(&str, Option<&str>)> {
    let first = contents.chars().next()?;
    if !is_name_start(first) {
        return None;
    }

    let mut name_end = first.len_utf8();
    while name_end < contents.len() {
        let c = contents[name_end..].chars().next().unwrap();
        if !is_name_continue(c) {
            break;
        }
        name_end += c.len_utf8();
    }

    let name = &contents[..name_end];
    let rest = &contents[name_end..];
    if rest.is_empty() {
        Some((name, None))
    } else if let Some(default) = rest.strip_prefix(":-") {
        Some((name, Some(default)))
    } else {
        None
    }
}