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
                match c {
                    '"' => {
                        quote = Quote::None;
                        i = next_i;
                    }
                    '\\' => {
                        if next_i == input.len() {
                            return Err(SplitError::TrailingEscape { pos: i });
                        }
                        let escaped = input[next_i..].chars().next().unwrap();
                        let after = next_i + escaped.len_utf8();
                        match escaped {
                            '"' | '\\' | '$' => {
                                word.push(escaped);
                                active = true;
                                i = after;
                            }
                            '\n' => {
                                i = after;
                            }
                            _ => {
                                word.push('\\');
                                active = true;
                                i = next_i;
                            }
                        }
                    }
                    '$' => {
                        active = true;
                        let (text, end) = expand(input, i, env)?;
                        word.push_str(&text);
                        i = end;
                    }
                    _ => {
                        word.push(c);
                        active = true;
                        i = next_i;
                    }
                }
            }
            Quote::None => {
                match c {
                    ' ' | '\t' | '\r' | '\n' => {
                        if active {
                            words.push(std::mem::take(&mut word));
                            active = false;
                        }
                        i = next_i;
                    }
                    '\'' => {
                        active = true;
                        quote = Quote::Single(i);
                        i = next_i;
                    }
                    '"' => {
                        active = true;
                        quote = Quote::Double(i);
                        i = next_i;
                    }
                    '\\' => {
                        if next_i == input.len() {
                            return Err(SplitError::TrailingEscape { pos: i });
                        }
                        let escaped = input[next_i..].chars().next().unwrap();
                        let after = next_i + escaped.len_utf8();
                        if escaped == '\n' {
                            i = after;
                        } else {
                            word.push(escaped);
                            active = true;
                            i = after;
                        }
                    }
                    '$' => {
                        let (text, end) = expand(input, i, env)?;
                        word.push_str(&text);
                        active = true;
                        i = end;
                    }
                    _ => {
                        word.push(c);
                        active = true;
                        i = next_i;
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

fn expand(input: &str, dollar: usize, env: &[(&str, &str)]) -> Result<(String, usize), SplitError> {
    let start = dollar + 1;
    if start >= input.len() {
        return Ok(("$".to_string(), start));
    }

    let c = input[start..].chars().next().unwrap();
    if c == '{' {
        let content_start = start + 1;
        let mut i = content_start;
        let mut close = None;

        while i < input.len() {
            let ch = input[i..].chars().next().unwrap();
            let next = i + ch.len_utf8();
            if ch == '\\' {
                if next < input.len() {
                    let skipped = input[next..].chars().next().unwrap();
                    i = next + skipped.len_utf8();
                } else {
                    i = next;
                }
            } else if ch == '}' {
                close = Some(i);
                break;
            } else {
                i = next;
            }
        }

        let close = close.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
        let contents = &input[content_start..close];

        let name_end = contents
            .char_indices()
            .find_map(|(idx, ch)| {
                if is_name_continue(ch) {
                    None
                } else {
                    Some(idx)
                }
            })
            .unwrap_or(contents.len());

        let name = &contents[..name_end];
        if name.is_empty() || !is_name_start(name.chars().next().unwrap()) {
            return Err(SplitError::InvalidExpansion { pos: dollar });
        }

        let suffix = &contents[name_end..];
        let default = if suffix.is_empty() {
            None
        } else if let Some(text) = suffix.strip_prefix(":-") {
            Some(text)
        } else {
            return Err(SplitError::InvalidExpansion { pos: dollar });
        };

        let value = lookup(env, name);
        let result = match default {
            None => value.unwrap_or("").to_string(),
            Some(text) => match value {
                Some(v) if !v.is_empty() => v.to_string(),
                _ => decode_default(text),
            },
        };
        Ok((result, close + 1))
    } else if is_name_start(c) {
        let mut end = start + c.len_utf8();
        while end < input.len() {
            let ch = input[end..].chars().next().unwrap();
            if !is_name_continue(ch) {
                break;
            }
            end += ch.len_utf8();
        }
        let name = &input[start..end];
        Ok((lookup(env, name).unwrap_or("").to_string(), end))
    } else {
        Ok(("$".to_string(), start))
    }
}

fn is_name_start(c: char) -> bool {
    c == '_' || c.is_ascii_alphabetic()
}

fn is_name_continue(c: char) -> bool {
    is_name_start(c) || c.is_ascii_digit()
}

fn lookup<'a>(env: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| if *key == name { Some(*value) } else { None })
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