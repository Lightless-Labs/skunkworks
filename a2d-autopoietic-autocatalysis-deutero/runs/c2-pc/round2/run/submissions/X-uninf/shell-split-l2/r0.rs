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
                    let Some(n) = input[next_i..].chars().next() else {
                        return Err(SplitError::TrailingEscape { pos: i });
                    };
                    let after_n = next_i + n.len_utf8();
                    if matches!(n, '"' | '\\' | '$') {
                        word.push(n);
                        active = true;
                        i = after_n;
                    } else if n == '\n' {
                        i = after_n;
                    } else {
                        word.push('\\');
                        active = true;
                        i = next_i;
                    }
                } else if c == '$' {
                    if let Some((value, end)) = expand(input, i, env)? {
                        word.push_str(&value);
                        active = true;
                        i = end;
                    } else {
                        word.push('$');
                        active = true;
                        i = next_i;
                    }
                } else {
                    word.push(c);
                    active = true;
                    i = next_i;
                }
            }
            Quote::None => {
                if matches!(c, ' ' | '\t' | '\r' | '\n') {
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
                    let Some(n) = input[next_i..].chars().next() else {
                        return Err(SplitError::TrailingEscape { pos: i });
                    };
                    let after_n = next_i + n.len_utf8();
                    if n != '\n' {
                        word.push(n);
                        active = true;
                    }
                    i = after_n;
                } else if c == '$' {
                    if let Some((value, end)) = expand(input, i, env)? {
                        word.push_str(&value);
                        active = true;
                        i = end;
                    } else {
                        word.push('$');
                        active = true;
                        i = next_i;
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

fn expand(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
) -> Result<Option<(String, usize)>, SplitError> {
    let after_dollar = dollar + 1;
    let Some(c) = input[after_dollar..].chars().next() else {
        return Ok(None);
    };

    if c == '{' {
        let content_start = after_dollar + 1;
        let mut scan = content_start;
        let mut close = None;

        while scan < input.len() {
            let ch = input[scan..].chars().next().unwrap();
            let next = scan + ch.len_utf8();
            if ch == '\\' {
                if next < input.len() {
                    let escaped = input[next..].chars().next().unwrap();
                    scan = next + escaped.len_utf8();
                } else {
                    scan = next;
                }
            } else if ch == '}' {
                close = Some(scan);
                break;
            } else {
                scan = next;
            }
        }

        let Some(close_pos) = close else {
            return Err(SplitError::UnterminatedExpansion { pos: dollar });
        };

        let contents = &input[content_start..close_pos];
        let (name, default) = parse_braced_contents(contents)
            .ok_or(SplitError::InvalidExpansion { pos: dollar })?;

        let value = lookup(env, name);
        let result = match default {
            Some(text) if value.is_empty() => decode_default(text),
            _ => value.to_string(),
        };
        Ok(Some((result, close_pos + 1)))
    } else if is_name_start(c) {
        let mut end = after_dollar + c.len_utf8();
        while end < input.len() {
            let ch = input[end..].chars().next().unwrap();
            if is_name_continue(ch) {
                end += ch.len_utf8();
            } else {
                break;
            }
        }
        let name = &input[after_dollar..end];
        Ok(Some((lookup(env, name).to_string(), end)))
    } else {
        Ok(None)
    }
}

fn parse_braced_contents(contents: &str) -> Option<(&str, Option<&str>)> {
    let name_end = contents
        .char_indices()
        .find_map(|(i, c)| if !is_name_continue(c) { Some(i) } else { None })
        .unwrap_or(contents.len());

    let name = &contents[..name_end];
    if name.is_empty() || !name.chars().next().is_some_and(is_name_start) {
        return None;
    }
    if !name.chars().all(is_name_continue) {
        return None;
    }

    let rest = &contents[name_end..];
    if rest.is_empty() {
        Some((name, None))
    } else if let Some(text) = rest.strip_prefix(":-") {
        Some((name, Some(text)))
    } else {
        None
    }
}

fn decode_default(text: &str) -> String {
    let mut result = String::new();
    let mut i = 0;
    while i < text.len() {
        let c = text[i..].chars().next().unwrap();
        let next = i + c.len_utf8();
        if c == '\\' && next < text.len() {
            let escaped = text[next..].chars().next().unwrap();
            result.push(escaped);
            i = next + escaped.len_utf8();
        } else {
            result.push(c);
            i = next;
        }
    }
    result
}

fn lookup<'a>(env: &'a [(&str, &str)], name: &str) -> &'a str {
    env.iter()
        .find_map(|(key, value)| (*key == name).then_some(*value))
        .unwrap_or("")
}

fn is_name_start(c: char) -> bool {
    c == '_' || c.is_ascii_alphabetic()
}

fn is_name_continue(c: char) -> bool {
    is_name_start(c) || c.is_ascii_digit()
}