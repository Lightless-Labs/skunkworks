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
    Single { start: usize },
    Double { start: usize },
}

fn is_name_start(c: char) -> bool {
    c == '_' || c.is_ascii_alphabetic()
}

fn is_name_char(c: char) -> bool {
    is_name_start(c) || c.is_ascii_digit()
}

fn env_value<'a>(env: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| if *key == name { Some(*value) } else { None })
}

fn parse_braced(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
) -> Result<(String, usize), SplitError> {
    let bytes = input.as_bytes();
    let mut close = dollar + 2;

    while close < bytes.len() {
        if bytes[close] == b'\\' {
            close += 1;
            if close < bytes.len() {
                let c = input[close..].chars().next().unwrap();
                close += c.len_utf8();
            }
        } else if bytes[close] == b'}' {
            break;
        } else {
            let c = input[close..].chars().next().unwrap();
            close += c.len_utf8();
        }
    }

    if close >= bytes.len() {
        return Err(SplitError::UnterminatedExpansion { pos: dollar });
    }

    let contents = &input[dollar + 2..close];
    let chars: Vec<(usize, char)> = contents.char_indices().collect();
    let mut name_end = 0;

    if chars.is_empty() || !is_name_start(chars[0].1) {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }

    name_end = chars[0].0 + chars[0].1.len_utf8();
    for &(offset, c) in chars.iter().skip(1) {
        if is_name_char(c) {
            name_end = offset + c.len_utf8();
        } else {
            break;
        }
    }

    let name = &contents[..name_end];
    let rest = &contents[name_end..];

    if rest.is_empty() {
        Ok((env_value(env, name).unwrap_or("").to_string(), close + 1))
    } else if let Some(default) = rest.strip_prefix(":-") {
        match env_value(env, name) {
            Some(value) if !value.is_empty() => Ok((value.to_string(), close + 1)),
            _ => {
                let mut decoded = String::new();
                let mut i = 0;
                while i < default.len() {
                    let c = default[i..].chars().next().unwrap();
                    if c == '\\' {
                        i += 1;
                        if i < default.len() {
                            let next = default[i..].chars().next().unwrap();
                            decoded.push(next);
                            i += next.len_utf8();
                        }
                    } else {
                        decoded.push(c);
                        i += c.len_utf8();
                    }
                }
                Ok((decoded, close + 1))
            }
        }
    } else {
        Err(SplitError::InvalidExpansion { pos: dollar })
    }
}

fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
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
        let c = input[i..].chars().next().unwrap();
        let width = c.len_utf8();

        match quote {
            Quote::Single { .. } => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    word.push(c);
                }
                i += width;
            }
            Quote::Double { .. } => {
                if c == '"' {
                    quote = Quote::None;
                    i += width;
                } else if c == '\\' {
                    let pos = i;
                    i += 1;
                    if i >= input.len() {
                        return Err(SplitError::TrailingEscape { pos });
                    }
                    let next = input[i..].chars().next().unwrap();
                    match next {
                        '"' | '\\' | '$' => {
                            word.push(next);
                            active = true;
                            i += next.len_utf8();
                        }
                        '\n' => {
                            i += 1;
                        }
                        _ => {
                            word.push('\\');
                            active = true;
                        }
                    }
                } else if c == '$' {
                    if input[i..].starts_with("${") {
                        let (value, next) = parse_braced(input, i, env)?;
                        word.push_str(&value);
                        active = true;
                        i = next;
                    } else {
                        let next_i = i + width;
                        if next_i < input.len() {
                            let next = input[next_i..].chars().next().unwrap();
                            if is_name_start(next) {
                                let mut end = next_i + next.len_utf8();
                                while end < input.len() {
                                    let ch = input[end..].chars().next().unwrap();
                                    if is_name_char(ch) {
                                        end += ch.len_utf8();
                                    } else {
                                        break;
                                    }
                                }
                                let name = &input[next_i..end];
                                word.push_str(env_value(env, name).unwrap_or(""));
                                active = true;
                                i = end;
                            } else {
                                word.push('$');
                                active = true;
                                i += width;
                            }
                        } else {
                            word.push('$');
                            active = true;
                            i += width;
                        }
                    }
                } else {
                    word.push(c);
                    active = true;
                    i += width;
                }
            }
            Quote::None => {
                if is_separator(c) {
                    if active {
                        words.push(std::mem::take(&mut word));
                        active = false;
                    }
                    i += width;
                } else if c == '\'' {
                    quote = Quote::Single { start: i };
                    active = true;
                    i += width;
                } else if c == '"' {
                    quote = Quote::Double { start: i };
                    active = true;
                    i += width;
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
                    if input[i..].starts_with("${") {
                        let (value, next) = parse_braced(input, i, env)?;
                        word.push_str(&value);
                        active = true;
                        i = next;
                    } else {
                        let next_i = i + width;
                        if next_i < input.len() {
                            let next = input[next_i..].chars().next().unwrap();
                            if is_name_start(next) {
                                let mut end = next_i + next.len_utf8();
                                while end < input.len() {
                                    let ch = input[end..].chars().next().unwrap();
                                    if is_name_char(ch) {
                                        end += ch.len_utf8();
                                    } else {
                                        break;
                                    }
                                }
                                let name = &input[next_i..end];
                                word.push_str(env_value(env, name).unwrap_or(""));
                                active = true;
                                i = end;
                            } else {
                                word.push('$');
                                active = true;
                                i += width;
                            }
                        } else {
                            word.push('$');
                            active = true;
                            i += width;
                        }
                    }
                } else {
                    word.push(c);
                    active = true;
                    i += width;
                }
            }
        }
    }

    match quote {
        Quote::Single { start } => {
            return Err(SplitError::UnterminatedSingle { pos: start });
        }
        Quote::Double { start } => {
            return Err(SplitError::UnterminatedDouble { pos: start });
        }
        Quote::None => {}
    }

    if active {
        words.push(word);
    }

    Ok(words)
}

fn main() {}