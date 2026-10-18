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

fn lookup<'a>(env: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    env.iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| *value)
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
    if let Some(pos) = input.find('\0') {
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
            Quote::Single(_) => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    word.push(c);
                }
                i = next_i;
            }
            Quote::Double(_) => {
                if c == '"' {
                    quote = Quote::None;
                    i = next_i;
                } else if c == '\\' {
                    let Some(next) = input[next_i..].chars().next() else {
                        return Err(SplitError::TrailingEscape { pos: i });
                    };
                    if next == '"' || next == '\\' || next == '$' {
                        word.push(next);
                        i = next_i + next.len_utf8();
                    } else if next == '\n' {
                        i = next_i + next.len_utf8();
                    } else {
                        word.push('\\');
                        i = next_i;
                    }
                } else if c == '$' {
                    active = true;
                    if next_i == input.len() {
                        word.push('$');
                        i = next_i;
                    } else {
                        let following = input[next_i..].chars().next().unwrap();
                        if following == '{' {
                            let (value, end) = parse_braced(input, i, env)?;
                            word.push_str(&value);
                            i = end;
                        } else if is_name_start(following) {
                            let (name, end) = parse_name(input, next_i);
                            if let Some(value) = lookup(env, name) {
                                word.push_str(value);
                            }
                            i = end;
                        } else {
                            word.push('$');
                            i = next_i;
                        }
                    }
                } else {
                    word.push(c);
                    i = next_i;
                }
            }
            Quote::None => {
                if c == ' ' || c == '\t' || c == '\r' || c == '\n' {
                    if active {
                        words.push(std::mem::take(&mut word));
                        active = false;
                    }
                    i = next_i;
                } else if c == '\'' {
                    active = true;
                    quote = Quote::Single(i);
                    i = next_i;
                } else if c == '"' {
                    active = true;
                    quote = Quote::Double(i);
                    i = next_i;
                } else if c == '\\' {
                    let Some(next) = input[next_i..].chars().next() else {
                        return Err(SplitError::TrailingEscape { pos: i });
                    };
                    if next != '\n' {
                        active = true;
                        word.push(next);
                    }
                    i = next_i + next.len_utf8();
                } else if c == '$' {
                    if next_i < input.len() {
                        let following = input[next_i..].chars().next().unwrap();
                        if following == '{' {
                            active = true;
                            let (value, end) = parse_braced(input, i, env)?;
                            word.push_str(&value);
                            i = end;
                        } else if is_name_start(following) {
                            active = true;
                            let (name, end) = parse_name(input, next_i);
                            if let Some(value) = lookup(env, name) {
                                word.push_str(value);
                            }
                            i = end;
                        } else {
                            active = true;
                            word.push('$');
                            i = next_i;
                        }
                    } else {
                        active = true;
                        word.push('$');
                        i = next_i;
                    }
                } else {
                    active = true;
                    word.push(c);
                    i = next_i;
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

fn parse_name(input: &str, start: usize) -> (&str, usize) {
    let mut end = start;
    for c in input[start..].chars() {
        if is_name_continue(c) {
            end += c.len_utf8();
        } else {
            break;
        }
    }
    (&input[start..end], end)
}

fn parse_braced(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
) -> Result<(String, usize), SplitError> {
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

    let Some(close_pos) = close else {
        return Err(SplitError::UnterminatedExpansion { pos: dollar });
    };

    let contents = &input[content_start..close_pos];
    let mut name_end = 0;
    for c in contents.chars() {
        if is_name_continue(c) {
            name_end += c.len_utf8();
        } else {
            break;
        }
    }
    let name = &contents[..name_end];
    if name.is_empty()
        || !is_name_start(name.chars().next().unwrap())
        || !contents[name_end..].is_empty() && !contents[name_end..].starts_with(":-")
    {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }

    let suffix = &contents[name_end..];
    let value = if suffix.is_empty() {
        lookup(env, name).unwrap_or("").to_string()
    } else {
        let current = lookup(env, name).unwrap_or("");
        if !current.is_empty() {
            current.to_string()
        } else {
            decode_default(&suffix[2..])
        }
    };

    Ok((value, close_pos + 1))
}