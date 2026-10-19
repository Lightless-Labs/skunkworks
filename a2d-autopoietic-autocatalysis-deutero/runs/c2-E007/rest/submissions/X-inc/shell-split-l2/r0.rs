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

fn first_env<'a>(env: &'a [(&'a str, &'a str)], name: &str) -> Option<&'a str> {
    env.iter().find_map(|(key, value)| (*key == name).then_some(*value))
}

fn decode_default(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
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

fn parse_braced(
    input: &str,
    dollar: usize,
    after_dollar: usize,
    env: &[(&str, &str)],
) -> Result<(String, usize), SplitError> {
    let tail = &input[after_dollar..];
    let mut close = None;
    let mut iter = tail.char_indices().peekable();

    while let Some((offset, c)) = iter.next() {
        if c == '\\' {
            if iter.next().is_none() {
                break;
            }
        } else if c == '}' {
            close = Some(after_dollar + offset);
            break;
        }
    }

    let close = close.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
    let contents = &input[after_dollar..close];

    let name_end = contents
        .char_indices()
        .find_map(|(i, c)| (!is_name_continue(c)).then_some(i))
        .unwrap_or(contents.len());

    let name = &contents[..name_end];
    if name.is_empty() || !name.chars().next().is_some_and(is_name_start) {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }

    let remainder = &contents[name_end..];
    let (value, default) = if remainder.is_empty() {
        (first_env(env, name).unwrap_or("").to_string(), None)
    } else if let Some(default) = remainder.strip_prefix(":-") {
        (first_env(env, name).unwrap_or("").to_string(), Some(default))
    } else {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    };

    let result = match default {
        Some(default) if value.is_empty() => decode_default(default),
        _ => value,
    };

    Ok((result, close + 1))
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<String>, SplitError> {
    if let Some(pos) = input.as_bytes().iter().position(|&b| b == 0) {
        return Err(SplitError::Nul { pos });
    }

    let mut words = Vec::new();
    let mut word = String::new();
    let mut active = false;
    let mut quote = Quote::None;
    let mut i = 0;

    while i < input.len() {
        let c = input[i..].chars().next().expect("valid character boundary");
        let n = c.len_utf8();

        match quote {
            Quote::Single(_) => {
                if c == '\'' {
                    quote = Quote::None;
                    i += n;
                } else {
                    word.push(c);
                    i += n;
                }
            }
            Quote::Double(open) => {
                if c == '"' {
                    quote = Quote::None;
                    i += n;
                } else if c == '\\' {
                    let pos = i;
                    i += 1;
                    if i >= input.len() {
                        return Err(SplitError::TrailingEscape { pos });
                    }
                    let next = input[i..].chars().next().expect("valid character boundary");
                    if matches!(next, '"' | '\\' | '$') {
                        word.push(next);
                        active = true;
                        i += next.len_utf8();
                    } else if next == '\n' {
                        i += 1;
                    } else {
                        word.push('\\');
                    }
                } else if c == '$' {
                    let dollar = i;
                    let after = i + 1;
                    if after < input.len() {
                        let next = input[after..].chars().next().unwrap();
                        if next == '{' {
                            let (value, end) = parse_braced(input, dollar, after + 1, env)?;
                            word.push_str(&value);
                            active = true;
                            i = end;
                        } else if is_name_start(next) {
                            let mut end = after;
                            for (offset, ch) in input[after..].char_indices() {
                                if is_name_continue(ch) {
                                    end = after + offset + ch.len_utf8();
                                } else {
                                    break;
                                }
                            }
                            let name = &input[after..end];
                            word.push_str(first_env(env, name).unwrap_or(""));
                            active = true;
                            i = end;
                        } else {
                            word.push('$');
                            active = true;
                            i += 1;
                        }
                    } else {
                        word.push('$');
                        active = true;
                        i += 1;
                    }
                } else {
                    word.push(c);
                    active = true;
                    i += n;
                }
                let _ = open;
            }
            Quote::None => {
                if is_separator(c) {
                    if active {
                        words.push(std::mem::take(&mut word));
                        active = false;
                    }
                    i += n;
                } else if c == '\'' {
                    quote = Quote::Single(i);
                    active = true;
                    i += n;
                } else if c == '"' {
                    quote = Quote::Double(i);
                    active = true;
                    i += n;
                } else if c == '\\' {
                    let pos = i;
                    i += 1;
                    if i >= input.len() {
                        return Err(SplitError::TrailingEscape { pos });
                    }
                    let next = input[i..].chars().next().unwrap();
                    if next == '\n' {
                        i += 1;
                    } else {
                        word.push(next);
                        active = true;
                        i += next.len_utf8();
                    }
                } else if c == '$' {
                    let dollar = i;
                    let after = i + 1;
                    if after < input.len() {
                        let next = input[after..].chars().next().unwrap();
                        if next == '{' {
                            let (value, end) = parse_braced(input, dollar, after + 1, env)?;
                            word.push_str(&value);
                            active = true;
                            i = end;
                        } else if is_name_start(next) {
                            let mut end = after;
                            for (offset, ch) in input[after..].char_indices() {
                                if is_name_continue(ch) {
                                    end = after + offset + ch.len_utf8();
                                } else {
                                    break;
                                }
                            }
                            let name = &input[after..end];
                            word.push_str(first_env(env, name).unwrap_or(""));
                            active = true;
                            i = end;
                        } else {
                            word.push('$');
                            active = true;
                            i += 1;
                        }
                    } else {
                        word.push('$');
                        active = true;
                        i += 1;
                    }
                } else {
                    word.push(c);
                    active = true;
                    i += n;
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