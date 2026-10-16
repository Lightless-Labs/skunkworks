#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitError {
    Nul { pos: usize },
    TrailingEscape { pos: usize },
    UnterminatedSingle { pos: usize },
    UnterminatedDouble { pos: usize },
    UnterminatedExpansion { pos: usize },
    InvalidExpansion { pos: usize },
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<String>, SplitError> {
    let bytes = input.as_bytes();

    if let Some(pos) = bytes.iter().position(|&b| b == 0) {
        return Err(SplitError::Nul { pos });
    }

    fn is_separator(ch: char) -> bool {
        matches!(ch, ' ' | '\t' | '\r' | '\n')
    }

    fn is_name_start(ch: char) -> bool {
        ch == '_' || ch.is_ascii_alphabetic()
    }

    fn is_name_continue(ch: char) -> bool {
        is_name_start(ch) || ch.is_ascii_digit()
    }

    fn lookup<'a>(env: &'a [(&str, &str)], name: &str) -> &'a str {
        env.iter()
            .find_map(|(key, value)| (*key == name).then_some(*value))
            .unwrap_or("")
    }

    let mut words = Vec::new();
    let mut word = String::new();
    let mut active = false;
    let mut i = 0;

    while i < input.len() {
        let ch = input[i..].chars().next().unwrap();

        if is_separator(ch) {
            if active {
                words.push(std::mem::take(&mut word));
                active = false;
            }
            i += ch.len_utf8();
            continue;
        }

        match ch {
            '\'' => {
                let quote_pos = i;
                active = true;
                i += 1;
                loop {
                    if i >= input.len() {
                        return Err(SplitError::UnterminatedSingle { pos: quote_pos });
                    }
                    let c = input[i..].chars().next().unwrap();
                    if c == '\'' {
                        i += 1;
                        break;
                    }
                    word.push(c);
                    i += c.len_utf8();
                }
            }
            '"' => {
                let quote_pos = i;
                active = true;
                i += 1;
                loop {
                    if i >= input.len() {
                        return Err(SplitError::UnterminatedDouble { pos: quote_pos });
                    }
                    let c = input[i..].chars().next().unwrap();
                    match c {
                        '"' => {
                            i += 1;
                            break;
                        }
                        '\\' => {
                            let slash_pos = i;
                            i += 1;
                            if i >= input.len() {
                                return Err(SplitError::TrailingEscape { pos: slash_pos });
                            }
                            let next = input[i..].chars().next().unwrap();
                            if matches!(next, '"' | '\\' | '$') {
                                word.push(next);
                                i += next.len_utf8();
                            } else if next == '\n' {
                                i += 1;
                            } else {
                                word.push('\\');
                            }
                        }
                        '$' => {
                            let dollar_pos = i;
                            let next_pos = i + 1;
                            if next_pos < input.len() {
                                let next = input[next_pos..].chars().next().unwrap();
                                if is_name_start(next) {
                                    let mut end = next_pos + next.len_utf8();
                                    while end < input.len() {
                                        let c = input[end..].chars().next().unwrap();
                                        if !is_name_continue(c) {
                                            break;
                                        }
                                        end += c.len_utf8();
                                    }
                                    let name = &input[next_pos..end];
                                    word.push_str(lookup(env, name));
                                    active = true;
                                    i = end;
                                    continue;
                                } else if next == '{' {
                                    let mut close = next_pos + 1;
                                    let mut found = None;
                                    while close < input.len() {
                                        let c = input[close..].chars().next().unwrap();
                                        if c == '\\' {
                                            close += 1;
                                            if close < input.len() {
                                                let escaped = input[close..].chars().next().unwrap();
                                                close += escaped.len_utf8();
                                            }
                                        } else if c == '}' {
                                            found = Some(close);
                                            break;
                                        } else {
                                            close += c.len_utf8();
                                        }
                                    }
                                    let close = match found {
                                        Some(pos) => pos,
                                        None => {
                                            return Err(SplitError::UnterminatedExpansion {
                                                pos: dollar_pos,
                                            })
                                        }
                                    };
                                    let contents = &input[next_pos + 1..close];
                                    let (name, default) = if let Some(split_at) = contents.find(":-") {
                                        (&contents[..split_at], Some(&contents[split_at + 2..]))
                                    } else {
                                        (contents, None)
                                    };
                                    let valid_name = !name.is_empty()
                                        && name.chars().next().is_some_and(is_name_start)
                                        && name.chars().all(is_name_continue);
                                    if !valid_name {
                                        return Err(SplitError::InvalidExpansion { pos: dollar_pos });
                                    }
                                    let value = lookup(env, name);
                                    if let Some(default_text) = default {
                                        if value.is_empty() {
                                            let mut k = 0;
                                            while k < default_text.len() {
                                                let d = default_text[k..].chars().next().unwrap();
                                                if d == '\\' {
                                                    k += 1;
                                                    if k < default_text.len() {
                                                        let escaped =
                                                            default_text[k..].chars().next().unwrap();
                                                        word.push(escaped);
                                                        k += escaped.len_utf8();
                                                    } else {
                                                        word.push('\\');
                                                    }
                                                } else {
                                                    word.push(d);
                                                    k += d.len_utf8();
                                                }
                                            }
                                        } else {
                                            word.push_str(value);
                                        }
                                    } else {
                                        word.push_str(value);
                                    }
                                    active = true;
                                    i = close + 1;
                                    continue;
                                }
                            }
                            word.push('$');
                            active = true;
                            i += 1;
                        }
                        _ => {
                            word.push(c);
                            i += c.len_utf8();
                        }
                    }
                }
            }
            '\\' => {
                let slash_pos = i;
                i += 1;
                if i >= input.len() {
                    return Err(SplitError::TrailingEscape { pos: slash_pos });
                }
                let next = input[i..].chars().next().unwrap();
                if next == '\n' {
                    i += 1;
                } else {
                    word.push(next);
                    active = true;
                    i += next.len_utf8();
                }
            }
            '$' => {
                let dollar_pos = i;
                let next_pos = i + 1;
                if next_pos < input.len() {
                    let next = input[next_pos..].chars().next().unwrap();
                    if is_name_start(next) {
                        let mut end = next_pos + next.len_utf8();
                        while end < input.len() {
                            let c = input[end..].chars().next().unwrap();
                            if !is_name_continue(c) {
                                break;
                            }
                            end += c.len_utf8();
                        }
                        let name = &input[next_pos..end];
                        word.push_str(lookup(env, name));
                        active = true;
                        i = end;
                        continue;
                    } else if next == '{' {
                        let mut close = next_pos + 1;
                        let mut found = None;
                        while close < input.len() {
                            let c = input[close..].chars().next().unwrap();
                            if c == '\\' {
                                close += 1;
                                if close < input.len() {
                                    let escaped = input[close..].chars().next().unwrap();
                                    close += escaped.len_utf8();
                                }
                            } else if c == '}' {
                                found = Some(close);
                                break;
                            } else {
                                close += c.len_utf8();
                            }
                        }
                        let close = match found {
                            Some(pos) => pos,
                            None => {
                                return Err(SplitError::UnterminatedExpansion { pos: dollar_pos })
                            }
                        };
                        let contents = &input[next_pos + 1..close];
                        let (name, default) = if let Some(split_at) = contents.find(":-") {
                            (&contents[..split_at], Some(&contents[split_at + 2..]))
                        } else {
                            (contents, None)
                        };
                        let valid_name = !name.is_empty()
                            && name.chars().next().is_some_and(is_name_start)
                            && name.chars().all(is_name_continue);
                        if !valid_name {
                            return Err(SplitError::InvalidExpansion { pos: dollar_pos });
                        }
                        let value = lookup(env, name);
                        if let Some(default_text) = default {
                            if value.is_empty() {
                                let mut k = 0;
                                while k < default_text.len() {
                                    let d = default_text[k..].chars().next().unwrap();
                                    if d == '\\' {
                                        k += 1;
                                        if k < default_text.len() {
                                            let escaped =
                                                default_text[k..].chars().next().unwrap();
                                            word.push(escaped);
                                            k += escaped.len_utf8();
                                        } else {
                                            word.push('\\');
                                        }
                                    } else {
                                        word.push(d);
                                        k += d.len_utf8();
                                    }
                                }
                            } else {
                                word.push_str(value);
                            }
                        } else {
                            word.push_str(value);
                        }
                        active = true;
                        i = close + 1;
                        continue;
                    }
                }
                word.push('$');
                active = true;
                i += 1;
            }
            _ => {
                word.push(ch);
                active = true;
                i += ch.len_utf8();
            }
        }
    }

    if active {
        words.push(word);
    }
    Ok(words)
}