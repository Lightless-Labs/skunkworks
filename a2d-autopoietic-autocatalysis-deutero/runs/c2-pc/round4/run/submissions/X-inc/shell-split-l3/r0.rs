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

fn is_separator(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\r' | '\n')
}

fn is_name_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_name_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn append_scalar(word: &mut Word, ch: char, active_glob: bool) {
    let offset = word.text.len();
    word.text.push(ch);
    if active_glob && matches!(ch, '*' | '?' | '[') {
        word.glob.push(offset);
    }
}

fn append_text(word: &mut Word, text: &str, active_glob: bool) {
    for ch in text.chars() {
        append_scalar(word, ch, active_glob);
    }
}

fn lookup<'a>(name: &str, env: &'a [(&str, &str)]) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| (*key == name).then_some(*value))
}

fn decode_default(text: &str) -> String {
    let mut decoded = String::new();
    let mut chars = text.char_indices().peekable();

    while let Some((_, ch)) = chars.next() {
        if ch == '\\' {
            if let Some((_, next)) = chars.next() {
                decoded.push(next);
            }
        } else {
            decoded.push(ch);
        }
    }

    decoded
}

fn parse_braced(
    input: &str,
    dollar_pos: usize,
    env: &[(&str, &str)],
) -> Result<(String, usize), SplitError> {
    let after_open = dollar_pos + 2;
    let mut cursor = after_open;
    let mut close = None;

    while cursor < input.len() {
        let ch = input[cursor..].chars().next().unwrap();
        if ch == '\\' {
            cursor += ch.len_utf8();
            if cursor < input.len() {
                let next = input[cursor..].chars().next().unwrap();
                cursor += next.len_utf8();
            }
        } else if ch == '}' {
            close = Some(cursor);
            break;
        } else {
            cursor += ch.len_utf8();
        }
    }

    let close_pos = close.ok_or(SplitError::UnterminatedExpansion { pos: dollar_pos })?;
    let contents = &input[after_open..close_pos];

    let name_end = contents
        .char_indices()
        .find_map(|(i, ch)| (!is_name_continue(ch)).then_some(i))
        .unwrap_or(contents.len());

    let name = &contents[..name_end];
    if name.is_empty()
        || !name.chars().next().map(is_name_start).unwrap_or(false)
        || !name.chars().all(is_name_continue)
    {
        return Err(SplitError::InvalidExpansion { pos: dollar_pos });
    }

    let remainder = &contents[name_end..];
    let value = if remainder.is_empty() {
        lookup(name, env).unwrap_or("").to_string()
    } else if let Some(default) = remainder.strip_prefix(":-") {
        match lookup(name, env) {
            Some(value) if !value.is_empty() => value.to_string(),
            _ => decode_default(default),
        }
    } else {
        return Err(SplitError::InvalidExpansion { pos: dollar_pos });
    };

    Ok((value, close_pos + 1))
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<Word>, SplitError> {
    if let Some(pos) = input.as_bytes().iter().position(|&byte| byte == 0) {
        return Err(SplitError::Nul { pos });
    }

    let mut words = Vec::new();
    let mut word = Word {
        text: String::new(),
        glob: Vec::new(),
    };
    let mut active = false;
    let mut quote = Quote::None;
    let mut cursor = 0;

    while cursor < input.len() {
        let ch = input[cursor..].chars().next().unwrap();
        let next_cursor = cursor + ch.len_utf8();

        match quote {
            Quote::Single(open_pos) => {
                if ch == '\'' {
                    quote = Quote::None;
                } else {
                    append_scalar(&mut word, ch, false);
                }
                cursor = next_cursor;
                let _ = open_pos;
            }
            Quote::Double(open_pos) => {
                if ch == '"' {
                    quote = Quote::None;
                    cursor = next_cursor;
                } else if ch == '\\' {
                    if next_cursor == input.len() {
                        return Err(SplitError::TrailingEscape { pos: cursor });
                    }
                    let escaped = input[next_cursor..].chars().next().unwrap();
                    let after_escaped = next_cursor + escaped.len_utf8();
                    if matches!(escaped, '"' | '\\' | '$' | '\n') {
                        if escaped != '\n' {
                            append_scalar(&mut word, escaped, false);
                        }
                        cursor = after_escaped;
                    } else {
                        append_scalar(&mut word, '\\', false);
                        cursor = next_cursor;
                    }
                } else if ch == '`' {
                    return Err(SplitError::CommandSubstitution { pos: cursor });
                } else if ch == '$' {
                    if next_cursor < input.len() {
                        let next = input[next_cursor..].chars().next().unwrap();
                        if next == '(' {
                            return Err(SplitError::CommandSubstitution { pos: cursor });
                        }
                        if next == '{' {
                            let (value, after) = parse_braced(input, cursor, env)?;
                            append_text(&mut word, &value, false);
                            active = true;
                            cursor = after;
                            continue;
                        }
                        if is_name_start(next) {
                            let mut end = next_cursor;
                            while end < input.len() {
                                let item = input[end..].chars().next().unwrap();
                                if !is_name_continue(item) {
                                    break;
                                }
                                end += item.len_utf8();
                            }
                            let name = &input[next_cursor..end];
                            let value = lookup(name, env).unwrap_or("");
                            append_text(&mut word, value, false);
                            active = true;
                            cursor = end;
                            continue;
                        }
                    }
                    append_scalar(&mut word, '$', false);
                    active = true;
                    cursor = next_cursor;
                } else {
                    append_scalar(&mut word, ch, false);
                    cursor = next_cursor;
                }
                let _ = open_pos;
            }
            Quote::None => {
                if is_separator(ch) {
                    if active {
                        words.push(std::mem::replace(
                            &mut word,
                            Word {
                                text: String::new(),
                                glob: Vec::new(),
                            },
                        ));
                        active = false;
                    }
                    cursor = next_cursor;
                } else if ch == '\'' {
                    quote = Quote::Single(cursor);
                    active = true;
                    cursor = next_cursor;
                } else if ch == '"' {
                    quote = Quote::Double(cursor);
                    active = true;
                    cursor = next_cursor;
                } else if ch == '\\' {
                    if next_cursor == input.len() {
                        return Err(SplitError::TrailingEscape { pos: cursor });
                    }
                    let escaped = input[next_cursor..].chars().next().unwrap();
                    let after_escaped = next_cursor + escaped.len_utf8();
                    if escaped == '\n' {
                        cursor = after_escaped;
                    } else {
                        append_scalar(&mut word, escaped, false);
                        active = true;
                        cursor = after_escaped;
                    }
                } else if ch == '`' {
                    return Err(SplitError::CommandSubstitution { pos: cursor });
                } else if ch == '$' {
                    if next_cursor < input.len() {
                        let next = input[next_cursor..].chars().next().unwrap();
                        if next == '(' {
                            return Err(SplitError::CommandSubstitution { pos: cursor });
                        }
                        if next == '{' {
                            let (value, after) = parse_braced(input, cursor, env)?;
                            append_text(&mut word, &value, true);
                            active = true;
                            cursor = after;
                            continue;
                        }
                        if is_name_start(next) {
                            let mut end = next_cursor;
                            while end < input.len() {
                                let item = input[end..].chars().next().unwrap();
                                if !is_name_continue(item) {
                                    break;
                                }
                                end += item.len_utf8();
                            }
                            let name = &input[next_cursor..end];
                            let value = lookup(name, env).unwrap_or("");
                            append_text(&mut word, value, true);
                            active = true;
                            cursor = end;
                            continue;
                        }
                    }
                    append_scalar(&mut word, '$', false);
                    active = true;
                    cursor = next_cursor;
                } else {
                    append_scalar(&mut word, ch, true);
                    active = true;
                    cursor = next_cursor;
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