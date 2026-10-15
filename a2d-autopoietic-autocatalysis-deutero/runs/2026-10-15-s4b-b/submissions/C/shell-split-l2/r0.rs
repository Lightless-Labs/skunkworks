#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitError {
    Nul { pos: usize },
    TrailingEscape { pos: usize },
    UnterminatedSingle { pos: usize },
    UnterminatedDouble { pos: usize },
    UnterminatedExpansion { pos: usize },
    InvalidExpansion { pos: usize },
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
    c.is_ascii_alphanumeric() || c == '_'
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
            Quote::Double(open_pos) => {
                if c == '"' {
                    quote = Quote::None;
                    i = next_i;
                } else if c == '\\' {
                    if next_i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let next = input[next_i..].chars().next().unwrap();
                    let after_next = next_i + next.len_utf8();
                    match next {
                        '"' | '\\' | '$' => {
                            word.push(next);
                            active = true;
                            i = after_next;
                        }
                        '\n' => {
                            active = true;
                            i = after_next;
                        }
                        _ => {
                            word.push('\\');
                            active = true;
                            i = next_i;
                        }
                    }
                } else if c == '$' {
                    let dollar = i;
                    if next_i == input.len() {
                        word.push('$');
                        active = true;
                        i = next_i;
                    } else {
                        let next = input[next_i..].chars().next().unwrap();
                        if is_name_start(next) {
                            let mut end = next_i;
                            while end < input.len() {
                                let ch = input[end..].chars().next().unwrap();
                                if !is_name_char(ch) {
                                    break;
                                }
                                end += ch.len_utf8();
                            }
                            let value = lookup(&input[next_i..end], env).unwrap_or("");
                            word.push_str(value);
                            active = true;
                            i = end;
                        } else if next == '{' {
                            let (end, value) = parse_braced(input, dollar, next_i + 1, env)?;
                            word.push_str(&value);
                            active = true;
                            i = end;
                        } else {
                            word.push('$');
                            active = true;
                            i = next_i;
                        }
                    }
                } else {
                    word.push(c);
                    active = true;
                    i = next_i;
                }
                let _ = open_pos;
            }
            Quote::None => {
                if c == ' ' || c == '\t' || c == '\r' || c == '\n' {
                    if active {
                        words.push(std::mem::take(&mut word));
                        active = false;
                    }
                    i = next_i;
                } else if c == '\'' {
                    quote = Quote::Single(i);
                    active = true;
                    i = next_i;
                } else if c == '"' {
                    quote = Quote::Double(i);
                    active = true;
                    i = next_i;
                } else if c == '\\' {
                    if next_i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let next = input[next_i..].chars().next().unwrap();
                    let after_next = next_i + next.len_utf8();
                    if next == '\n' {
                        i = after_next;
                    } else {
                        word.push(next);
                        active = true;
                        i = after_next;
                    }
                } else if c == '$' {
                    let dollar = i;
                    if next_i == input.len() {
                        word.push('$');
                        active = true;
                        i = next_i;
                    } else {
                        let next = input[next_i..].chars().next().unwrap();
                        if is_name_start(next) {
                            let mut end = next_i;
                            while end < input.len() {
                                let ch = input[end..].chars().next().unwrap();
                                if !is_name_char(ch) {
                                    break;
                                }
                                end += ch.len_utf8();
                            }
                            word.push_str(lookup(&input[next_i..end], env).unwrap_or(""));
                            active = true;
                            i = end;
                        } else if next == '{' {
                            let (end, value) = parse_braced(input, dollar, next_i + 1, env)?;
                            word.push_str(&value);
                            active = true;
                            i = end;
                        } else {
                            word.push('$');
                            active = true;
                            i = next_i;
                        }
                    }
                } else {
                    word.push(c);
                    active = true;
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

fn parse_braced(
    input: &str,
    dollar: usize,
    content_start: usize,
    env: &[(&str, &str)],
) -> Result<(usize, String), SplitError> {
    let mut cursor = content_start;
    let mut close = None;

    while cursor < input.len() {
        let c = input[cursor..].chars().next().unwrap();
        if c == '\\' {
            cursor += 1;
            if cursor < input.len() {
                let escaped = input[cursor..].chars().next().unwrap();
                cursor += escaped.len_utf8();
            }
        } else if c == '}' {
            close = Some(cursor);
            break;
        } else {
            cursor += c.len_utf8();
        }
    }

    let close = close.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
    let contents = &input[content_start..close];

    let mut name_end = 0;
    let mut chars = contents.char_indices();
    if let Some((_, first)) = chars.next() {
        if is_name_start(first) {
            name_end = first.len_utf8();
            while name_end < contents.len() {
                let c = contents[name_end..].chars().next().unwrap();
                if !is_name_char(c) {
                    break;
                }
                name_end += c.len_utf8();
            }
        }
    }

    if name_end == 0 {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }

    let name = &contents[..name_end];
    let remainder = &contents[name_end..];

    let value = if remainder.is_empty() {
        lookup(name, env).unwrap_or("").to_string()
    } else if let Some(default) = remainder.strip_prefix(":-") {
        match lookup(name, env) {
            Some(value) if !value.is_empty() => value.to_string(),
            _ => decode_default(default),
        }
    } else {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    };

    Ok((close + 1, value))
}

fn main() {}