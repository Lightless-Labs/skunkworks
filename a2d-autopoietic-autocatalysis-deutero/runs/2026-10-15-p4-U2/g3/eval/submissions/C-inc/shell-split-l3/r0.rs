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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Quote {
    None,
    Single(usize),
    Double(usize),
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_name_char(c: char) -> bool {
    is_name_start(c) || c.is_ascii_digit()
}

fn lookup<'a>(env: &'a [(&str, &str)], name: &str) -> &'a str {
    env.iter()
        .find_map(|(key, value)| (*key == name).then_some(*value))
        .unwrap_or("")
}

fn append_literal(word: &mut Word, c: char, active_glob: bool) {
    if active_glob && matches!(c, '*' | '?' | '[') {
        word.glob.push(word.text.len());
    }
    word.text.push(c);
}

fn append_expansion(word: &mut Word, value: &str, active_glob: bool) {
    for c in value.chars() {
        append_literal(word, c, active_glob);
    }
}

fn decode_default(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.char_indices().peekable();

    while let Some((_, c)) = chars.next() {
        if c == '\\' {
            if let Some((_, next)) = chars.next() {
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

fn braced_expansion(
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

    let name_len = content
        .char_indices()
        .take_while(|(_, c)| is_name_char(*c))
        .map(|(i, c)| i + c.len_utf8())
        .last()
        .unwrap_or(0);

    let name = &content[..name_len];
    let valid_name = !name.is_empty()
        && name.chars().next().is_some_and(is_name_start)
        && name.chars().all(is_name_char);

    if !valid_name {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }

    let value = if name_len == content.len() {
        lookup(env, name).to_string()
    } else {
        let rest = &content[name_len..];
        let Some(default_text) = rest.strip_prefix(":-") else {
            return Err(SplitError::InvalidExpansion { pos: dollar });
        };

        let env_value = lookup(env, name);
        if env_value.is_empty() {
            decode_default(default_text)
        } else {
            env_value.to_string()
        }
    };

    Ok((close + 1, value))
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<Word>, SplitError> {
    if let Some(pos) = input.find('\0') {
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
        let c = input[cursor..].chars().next().unwrap();
        let next_cursor = cursor + c.len_utf8();

        match quote {
            Quote::Single(_) => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    word.text.push(c);
                }
                cursor = next_cursor;
            }
            Quote::Double(open_pos) => {
                match c {
                    '"' => {
                        quote = Quote::None;
                        cursor = next_cursor;
                    }
                    '\\' => {
                        if next_cursor == input.len() {
                            return Err(SplitError::TrailingEscape { pos: cursor });
                        }
                        let next = input[next_cursor..].chars().next().unwrap();
                        let after_next = next_cursor + next.len_utf8();
                        if matches!(next, '"' | '\\' | '$' | '\n') {
                            if next != '\n' {
                                word.text.push(next);
                            }
                            cursor = after_next;
                        } else {
                            word.text.push('\\');
                            cursor = next_cursor;
                        }
                    }
                    '`' => {
                        return Err(SplitError::CommandSubstitution { pos: cursor });
                    }
                    '$' => {
                        let after_dollar = next_cursor;
                        if after_dollar < input.len() {
                            let following = input[after_dollar..].chars().next().unwrap();
                            if following == '(' {
                                return Err(SplitError::CommandSubstitution { pos: cursor });
                            }
                            if following == '{' {
                                let (end, value) = braced_expansion(input, cursor, env)?;
                                active = true;
                                append_expansion(&mut word, &value, false);
                                cursor = end;
                                continue;
                            }
                            if is_name_start(following) {
                                let mut end = after_dollar;
                                while end < input.len() {
                                    let ch = input[end..].chars().next().unwrap();
                                    if !is_name_char(ch) {
                                        break;
                                    }
                                    end += ch.len_utf8();
                                }
                                let name = &input[after_dollar..end];
                                active = true;
                                append_expansion(&mut word, lookup(env, name), false);
                                cursor = end;
                                continue;
                            }
                        }
                        word.text.push('$');
                        cursor = next_cursor;
                    }
                    _ => {
                        word.text.push(c);
                        cursor = next_cursor;
                    }
                }
                let _ = open_pos;
            }
            Quote::None => {
                if matches!(c, ' ' | '\t' | '\r' | '\n') {
                    if active {
                        words.push(word);
                        word = Word {
                            text: String::new(),
                            glob: Vec::new(),
                        };
                        active = false;
                    }
                    cursor = next_cursor;
                } else if c == '\'' {
                    active = true;
                    quote = Quote::Single(cursor);
                    cursor = next_cursor;
                } else if c == '"' {
                    active = true;
                    quote = Quote::Double(cursor);
                    cursor = next_cursor;
                } else if c == '\\' {
                    if next_cursor == input.len() {
                        return Err(SplitError::TrailingEscape { pos: cursor });
                    }
                    let next = input[next_cursor..].chars().next().unwrap();
                    if next == '\n' {
                        cursor = next_cursor + next.len_utf8();
                    } else {
                        active = true;
                        append_literal(&mut word, next, false);
                        cursor = next_cursor + next.len_utf8();
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: cursor });
                } else if c == '$' {
                    let after_dollar = next_cursor;
                    if after_dollar < input.len() {
                        let following = input[after_dollar..].chars().next().unwrap();
                        if following == '(' {
                            return Err(SplitError::CommandSubstitution { pos: cursor });
                        }
                        if following == '{' {
                            let (end, value) = braced_expansion(input, cursor, env)?;
                            active = true;
                            append_expansion(&mut word, &value, true);
                            cursor = end;
                            continue;
                        }
                        if is_name_start(following) {
                            let mut end = after_dollar;
                            while end < input.len() {
                                let ch = input[end..].chars().next().unwrap();
                                if !is_name_char(ch) {
                                    break;
                                }
                                end += ch.len_utf8();
                            }
                            let name = &input[after_dollar..end];
                            active = true;
                            append_expansion(&mut word, lookup(env, name), true);
                            cursor = end;
                            continue;
                        }
                    }
                    active = true;
                    word.text.push('$');
                    cursor = next_cursor;
                } else {
                    active = true;
                    append_literal(&mut word, c, true);
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

fn main() {}