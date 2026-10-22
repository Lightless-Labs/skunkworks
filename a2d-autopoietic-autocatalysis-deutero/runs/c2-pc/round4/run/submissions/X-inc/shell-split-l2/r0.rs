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

fn lookup<'a>(name: &str, env: &'a [(&str, &str)]) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| (*key == name).then_some(*value))
}

fn append_expansion(
    input: &str,
    bytes: &[u8],
    dollar: usize,
    env: &[(&str, &str)],
    out: &mut String,
) -> Result<usize, SplitError> {
    let after_dollar = dollar + 1;
    if after_dollar < bytes.len() && bytes[after_dollar] == b'{' {
        let mut close = after_dollar + 1;
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

        if close == bytes.len() {
            return Err(SplitError::UnterminatedExpansion { pos: dollar });
        }

        let contents = &input[after_dollar + 1..close];
        let content_bytes = contents.as_bytes();
        let mut name_end = 0;
        if !contents.is_empty() {
            let first = contents.chars().next().unwrap();
            if is_name_start(first) {
                name_end = first.len_utf8();
                while name_end < content_bytes.len() {
                    let c = contents[name_end..].chars().next().unwrap();
                    if is_name_continue(c) {
                        name_end += c.len_utf8();
                    } else {
                        break;
                    }
                }
            }
        }

        if name_end == 0 {
            return Err(SplitError::InvalidExpansion { pos: dollar });
        }

        let name = &contents[..name_end];
        let rest = &contents[name_end..];
        if rest.is_empty() {
            out.push_str(lookup(name, env).unwrap_or(""));
        } else if let Some(default) = rest.strip_prefix(":-") {
            let value = lookup(name, env).unwrap_or("");
            if value.is_empty() {
                let mut i = 0;
                while i < default.len() {
                    let c = default[i..].chars().next().unwrap();
                    if c == '\\' {
                        i += 1;
                        if i < default.len() {
                            let escaped = default[i..].chars().next().unwrap();
                            out.push(escaped);
                            i += escaped.len_utf8();
                        } else {
                            out.push('\\');
                        }
                    } else {
                        out.push(c);
                        i += c.len_utf8();
                    }
                }
            } else {
                out.push_str(value);
            }
        } else {
            return Err(SplitError::InvalidExpansion { pos: dollar });
        }

        Ok(close + 1)
    } else if after_dollar < bytes.len() {
        let first = input[after_dollar..].chars().next().unwrap();
        if is_name_start(first) {
            let mut end = after_dollar + first.len_utf8();
            while end < bytes.len() {
                let c = input[end..].chars().next().unwrap();
                if is_name_continue(c) {
                    end += c.len_utf8();
                } else {
                    break;
                }
            }
            let name = &input[after_dollar..end];
            out.push_str(lookup(name, env).unwrap_or(""));
            Ok(end)
        } else {
            out.push('$');
            Ok(after_dollar)
        }
    } else {
        out.push('$');
        Ok(after_dollar)
    }
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<String>, SplitError> {
    if let Some(pos) = input.as_bytes().iter().position(|&b| b == 0) {
        return Err(SplitError::Nul { pos });
    }

    let bytes = input.as_bytes();
    let mut words = Vec::new();
    let mut word = String::new();
    let mut active = false;
    let mut quote = Quote::None;
    let mut i = 0;

    while i < bytes.len() {
        match quote {
            Quote::Single(_) => {
                let c = input[i..].chars().next().unwrap();
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    word.push(c);
                }
                i += c.len_utf8();
            }
            Quote::Double(_) => {
                let c = input[i..].chars().next().unwrap();
                if c == '"' {
                    quote = Quote::None;
                    i += 1;
                } else if c == '\\' {
                    let pos = i;
                    i += 1;
                    if i == bytes.len() {
                        return Err(SplitError::TrailingEscape { pos });
                    }
                    let next = input[i..].chars().next().unwrap();
                    if next == '"' || next == '\\' || next == '$' {
                        word.push(next);
                        i += next.len_utf8();
                    } else if next == '\n' {
                        i += 1;
                    } else {
                        word.push('\\');
                    }
                } else if c == '$' {
                    active = true;
                    i = append_expansion(input, bytes, i, env, &mut word)?;
                } else {
                    word.push(c);
                    i += c.len_utf8();
                }
            }
            Quote::None => {
                let c = input[i..].chars().next().unwrap();
                match c {
                    ' ' | '\t' | '\r' | '\n' => {
                        if active {
                            words.push(std::mem::take(&mut word));
                            active = false;
                        }
                        i += 1;
                    }
                    '\'' => {
                        active = true;
                        quote = Quote::Single(i);
                        i += 1;
                    }
                    '"' => {
                        active = true;
                        quote = Quote::Double(i);
                        i += 1;
                    }
                    '\\' => {
                        let pos = i;
                        i += 1;
                        if i == bytes.len() {
                            return Err(SplitError::TrailingEscape { pos });
                        }
                        let next = input[i..].chars().next().unwrap();
                        if next == '\n' {
                            i += 1;
                        } else {
                            active = true;
                            word.push(next);
                            i += next.len_utf8();
                        }
                    }
                    '$' => {
                        active = true;
                        i = append_expansion(input, bytes, i, env, &mut word)?;
                    }
                    _ => {
                        active = true;
                        word.push(c);
                        i += c.len_utf8();
                    }
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