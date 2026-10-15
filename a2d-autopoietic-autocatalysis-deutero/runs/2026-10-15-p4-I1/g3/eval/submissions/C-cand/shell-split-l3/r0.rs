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
    Single,
    Double,
}

fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_name_char(c: char) -> bool {
    is_name_start(c) || c.is_ascii_digit()
}

fn append_char(word: &mut Word, c: char, active_glob: bool) {
    let offset = word.text.len();
    word.text.push(c);
    if active_glob && matches!(c, '*' | '?' | '[') {
        word.glob.push(offset);
    }
}

fn append_str(word: &mut Word, value: &str, active_glob: bool) {
    for c in value.chars() {
        append_char(word, c, active_glob);
    }
}

fn env_value<'a>(env: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
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
    let mut quote_start = 0usize;
    let mut chars = input.char_indices().peekable();

    while let Some((pos, c)) = chars.next() {
        match quote {
            Quote::Single => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    append_char(&mut word, c, false);
                }
            }
            Quote::Double => {
                if c == '"' {
                    quote = Quote::None;
                } else if c == '\\' {
                    let Some(&(_, next)) = chars.peek() else {
                        return Err(SplitError::TrailingEscape { pos });
                    };
                    if matches!(next, '"' | '\\' | '$' | '\n') {
                        chars.next();
                        if next != '\n' {
                            append_char(&mut word, next, false);
                        }
                    } else {
                        append_char(&mut word, '\\', false);
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos });
                } else if c == '$' {
                    let Some(&(next_pos, next)) = chars.peek() else {
                        append_char(&mut word, '$', false);
                        continue;
                    };
                    if next == '(' {
                        return Err(SplitError::CommandSubstitution { pos });
                    }
                    if next == '{' {
                        let (end, name, default) = parse_braced(input, pos)?;
                        let value = if let Some(default_text) = default {
                            match env_value(env, name) {
                                Some(value) if !value.is_empty() => value.to_string(),
                                _ => decode_default(default_text),
                            }
                        } else {
                            env_value(env, name).unwrap_or("").to_string()
                        };
                        append_str(&mut word, &value, false);
                        while let Some(&(p, _)) = chars.peek() {
                            if p < end {
                                chars.next();
                            } else {
                                break;
                            }
                        }
                        continue;
                    }
                    if is_name_start(next) {
                        let start = next_pos;
                        let mut end = next_pos + next.len_utf8();
                        chars.next();
                        while let Some(&(p, n)) = chars.peek() {
                            if is_name_char(n) {
                                chars.next();
                                end = p + n.len_utf8();
                            } else {
                                break;
                            }
                        }
                        let name = &input[start..end];
                        let value = env_value(env, name).unwrap_or("");
                        append_str(&mut word, value, false);
                        continue;
                    }
                    append_char(&mut word, '$', false);
                } else {
                    append_char(&mut word, c, false);
                }
            }
            Quote::None => {
                if is_separator(c) {
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
                } else if c == '\'' {
                    active = true;
                    quote = Quote::Single;
                    quote_start = pos;
                } else if c == '"' {
                    active = true;
                    quote = Quote::Double;
                    quote_start = pos;
                } else if c == '\\' {
                    let Some((_, next)) = chars.next() else {
                        return Err(SplitError::TrailingEscape { pos });
                    };
                    if next == '\n' {
                        continue;
                    }
                    active = true;
                    append_char(&mut word, next, false);
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos });
                } else if c == '$' {
                    let Some(&(next_pos, next)) = chars.peek() else {
                        active = true;
                        append_char(&mut word, '$', true);
                        continue;
                    };
                    if next == '(' {
                        return Err(SplitError::CommandSubstitution { pos });
                    }
                    if next == '{' {
                        let (end, name, default) = parse_braced(input, pos)?;
                        active = true;
                        let value = if let Some(default_text) = default {
                            match env_value(env, name) {
                                Some(value) if !value.is_empty() => value.to_string(),
                                _ => decode_default(default_text),
                            }
                        } else {
                            env_value(env, name).unwrap_or("").to_string()
                        };
                        append_str(&mut word, &value, true);
                        while let Some(&(p, _)) = chars.peek() {
                            if p < end {
                                chars.next();
                            } else {
                                break;
                            }
                        }
                        continue;
                    }
                    if is_name_start(next) {
                        let start = next_pos;
                        let mut end = next_pos + next.len_utf8();
                        chars.next();
                        while let Some(&(p, n)) = chars.peek() {
                            if is_name_char(n) {
                                chars.next();
                                end = p + n.len_utf8();
                            } else {
                                break;
                            }
                        }
                        active = true;
                        let name = &input[start..end];
                        append_str(&mut word, env_value(env, name).unwrap_or(""), true);
                    } else {
                        active = true;
                        append_char(&mut word, '$', true);
                    }
                } else {
                    active = true;
                    append_char(&mut word, c, true);
                }
            }
        }
    }

    match quote {
        Quote::Single => return Err(SplitError::UnterminatedSingle { pos: quote_start }),
        Quote::Double => return Err(SplitError::UnterminatedDouble { pos: quote_start }),
        Quote::None => {}
    }

    if active {
        words.push(word);
    }
    Ok(words)
}

fn parse_braced<'a>(
    input: &'a str,
    dollar: usize,
) -> Result<(usize, &'a str, Option<&'a str>), SplitError> {
    let content_start = dollar + 2;
    let mut search = content_start;
    let close = loop {
        if search >= input.len() {
            return Err(SplitError::UnterminatedExpansion { pos: dollar });
        }
        let c = input[search..].chars().next().unwrap();
        if c == '\\' {
            search += c.len_utf8();
            if search < input.len() {
                let next = input[search..].chars().next().unwrap();
                search += next.len_utf8();
            }
        } else if c == '}' {
            break search;
        } else {
            search += c.len_utf8();
        }
    };

    let content = &input[content_start..close];
    let mut name_end = 0usize;
    let mut iter = content.char_indices();
    let Some((_, first)) = iter.next() else {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    };
    if !is_name_start(first) {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }
    name_end = first.len_utf8();
    for (idx, c) in iter {
        if is_name_char(c) {
            name_end = idx + c.len_utf8();
        } else {
            break;
        }
    }

    let name = &content[..name_end];
    let rest = &content[name_end..];
    let default = if rest.is_empty() {
        None
    } else if let Some(text) = rest.strip_prefix(":-") {
        Some(text)
    } else {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    };

    Ok((close + 1, name, default))
}

fn main() {}