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
    c == '_' || c.is_ascii_alphabetic()
}

fn is_name_continue(c: char) -> bool {
    is_name_start(c) || c.is_ascii_digit()
}

fn append_literal(word: &mut Word, c: char, wildcard_active: bool) {
    let offset = word.text.len();
    word.text.push(c);
    if wildcard_active && matches!(c, '*' | '?' | '[') {
        word.glob.push(offset);
    }
}

fn append_expansion(word: &mut Word, text: &str, wildcard_active: bool) {
    for c in text.chars() {
        append_literal(word, c, wildcard_active);
    }
}

fn first_env_value<'a>(env: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| (*key == name).then_some(*value))
}

fn decode_default(text: &str) -> String {
    let mut result = String::new();
    let mut chars = text.chars();

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

fn parse_braced(
    input: &str,
    dollar_pos: usize,
    env: &[(&str, &str)],
) -> Result<(String, usize), SplitError> {
    let bytes = input.as_bytes();
    let mut close = None;
    let mut i = dollar_pos + 2;

    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 1;
            if i < bytes.len() {
                let c = input[i..].chars().next().unwrap();
                i += c.len_utf8();
            }
        } else if bytes[i] == b'}' {
            close = Some(i);
            break;
        } else {
            let c = input[i..].chars().next().unwrap();
            i += c.len_utf8();
        }
    }

    let close = close.ok_or(SplitError::UnterminatedExpansion { pos: dollar_pos })?;
    let contents = &input[dollar_pos + 2..close];

    let mut name_end = 0;
    let mut chars = contents.char_indices();
    match chars.next() {
        Some((_, c)) if is_name_start(c) => {
            name_end = c.len_utf8();
        }
        _ => return Err(SplitError::InvalidExpansion { pos: dollar_pos }),
    }

    for (offset, c) in chars {
        if !is_name_continue(c) {
            break;
        }
        name_end = offset + c.len_utf8();
    }

    let name = &contents[..name_end];
    let rest = &contents[name_end..];

    let result = if rest.is_empty() {
        first_env_value(env, name).unwrap_or("").to_string()
    } else if let Some(default_text) = rest.strip_prefix(":-") {
        match first_env_value(env, name) {
            Some(value) if !value.is_empty() => value.to_string(),
            _ => decode_default(default_text),
        }
    } else {
        return Err(SplitError::InvalidExpansion { pos: dollar_pos });
    };

    Ok((result, close + 1))
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<Word>, SplitError> {
    if let Some(pos) = input.as_bytes().iter().position(|&b| b == 0) {
        return Err(SplitError::Nul { pos });
    }

    let mut words = Vec::new();
    let mut current = Word {
        text: String::new(),
        glob: Vec::new(),
    };
    let mut active = false;
    let mut quote = Quote::None;
    let mut quote_start = 0;
    let mut i = 0;

    while i < input.len() {
        let c = input[i..].chars().next().unwrap();
        let next_i = i + c.len_utf8();

        match quote {
            Quote::Single => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    append_literal(&mut current, c, false);
                }
                i = next_i;
            }
            Quote::Double => {
                if c == '"' {
                    quote = Quote::None;
                    i = next_i;
                } else if c == '\\' {
                    let slash_pos = i;
                    if next_i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: slash_pos });
                    }
                    let following = input[next_i..].chars().next().unwrap();
                    let after_following = next_i + following.len_utf8();
                    if matches!(following, '"' | '\\' | '$') {
                        append_literal(&mut current, following, false);
                        i = after_following;
                    } else if following == '\n' {
                        i = after_following;
                    } else {
                        append_literal(&mut current, '\\', false);
                        i = next_i;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if next_i < input.len() && input[next_i..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }

                    if next_i < input.len() && input[next_i..].starts_with('{') {
                        let (value, after) = parse_braced(input, i, env)?;
                        append_expansion(&mut current, &value, false);
                        active = true;
                        i = after;
                    } else if next_i < input.len() {
                        let first = input[next_i..].chars().next().unwrap();
                        if is_name_start(first) {
                            let mut end = next_i + first.len_utf8();
                            while end < input.len() {
                                let ch = input[end..].chars().next().unwrap();
                                if !is_name_continue(ch) {
                                    break;
                                }
                                end += ch.len_utf8();
                            }
                            let name = &input[next_i..end];
                            let value = first_env_value(env, name).unwrap_or("");
                            append_expansion(&mut current, value, false);
                            active = true;
                            i = end;
                        } else {
                            append_literal(&mut current, '$', false);
                            i = next_i;
                        }
                    } else {
                        append_literal(&mut current, '$', false);
                        i = next_i;
                    }
                } else {
                    append_literal(&mut current, c, false);
                    i = next_i;
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
                    i = next_i;
                } else if c == '\'' {
                    quote = Quote::Single;
                    quote_start = i;
                    active = true;
                    i = next_i;
                } else if c == '"' {
                    quote = Quote::Double;
                    quote_start = i;
                    active = true;
                    i = next_i;
                } else if c == '\\' {
                    let slash_pos = i;
                    if next_i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: slash_pos });
                    }
                    let following = input[next_i..].chars().next().unwrap();
                    let after_following = next_i + following.len_utf8();
                    if following == '\n' {
                        i = after_following;
                    } else {
                        append_literal(&mut current, following, false);
                        active = true;
                        i = after_following;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if next_i < input.len() && input[next_i..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }

                    if next_i < input.len() && input[next_i..].starts_with('{') {
                        let (value, after) = parse_braced(input, i, env)?;
                        append_expansion(&mut current, &value, true);
                        active = true;
                        i = after;
                    } else if next_i < input.len() {
                        let first = input[next_i..].chars().next().unwrap();
                        if is_name_start(first) {
                            let mut end = next_i + first.len_utf8();
                            while end < input.len() {
                                let ch = input[end..].chars().next().unwrap();
                                if !is_name_continue(ch) {
                                    break;
                                }
                                end += ch.len_utf8();
                            }
                            let name = &input[next_i..end];
                            let value = first_env_value(env, name).unwrap_or("");
                            append_expansion(&mut current, value, true);
                            active = true;
                            i = end;
                        } else {
                            append_literal(&mut current, '$', true);
                            active = true;
                            i = next_i;
                        }
                    } else {
                        append_literal(&mut current, '$', true);
                        active = true;
                        i = next_i;
                    }
                } else {
                    append_literal(&mut current, c, true);
                    active = true;
                    i = next_i;
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
        words.push(current);
    }

    Ok(words)
}