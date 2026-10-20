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

fn is_sep(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_name_continue(c: char) -> bool {
    is_name_start(c) || c.is_ascii_digit()
}

fn first_env<'a>(env: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    env.iter().find_map(|(k, v)| if *k == name { Some(*v) } else { None })
}

fn append_literal(word: &mut Word, c: char, active: bool) {
    let at = word.text.len();
    word.text.push(c);
    if active && matches!(c, '*' | '?' | '[') {
        word.glob.push(at);
    }
}

fn append_data(word: &mut Word, text: &str, active: bool) {
    for c in text.chars() {
        append_literal(word, c, active);
    }
}

fn decode_default(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.char_indices().peekable();
    while let Some((_, c)) = it.next() {
        if c == '\\' {
            if let Some((_, next)) = it.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn expand_braced(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
) -> Result<(usize, String), SplitError> {
    let bytes = input.as_bytes();
    let mut i = dollar + 2;
    let mut close = None;
    while i < bytes.len() {
        let c = input[i..].chars().next().unwrap();
        if c == '\\' {
            i += 1;
            if i < bytes.len() {
                let n = input[i..].chars().next().unwrap();
                i += n.len_utf8();
            }
        } else if c == '}' {
            close = Some(i);
            break;
        } else {
            i += c.len_utf8();
        }
    }
    let close = close.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
    let content = &input[dollar + 2..close];

    let mut chars = content.char_indices();
    let first = chars.next().map(|(_, c)| c).ok_or(SplitError::InvalidExpansion { pos: dollar })?;
    if !is_name_start(first) {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }
    let mut name_end = first.len_utf8();
    for (idx, c) in chars {
        if is_name_continue(c) {
            name_end = idx + c.len_utf8();
        } else {
            break;
        }
    }
    let name = &content[..name_end];
    let rest = &content[name_end..];
    let value = if rest.is_empty() {
        first_env(env, name).unwrap_or("").to_string()
    } else if let Some(default) = rest.strip_prefix(":-") {
        match first_env(env, name) {
            Some(v) if !v.is_empty() => v.to_string(),
            _ => decode_default(default),
        }
    } else {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    };
    Ok((close + 1, value))
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<Word>, SplitError> {
    if let Some(pos) = input.find('\0') {
        return Err(SplitError::Nul { pos });
    }

    let mut result = Vec::new();
    let mut word = Word { text: String::new(), glob: Vec::new() };
    let mut active_word = false;
    let mut quote = Quote::None;
    let mut quote_start = 0usize;
    let mut i = 0usize;

    while i < input.len() {
        let c = input[i..].chars().next().unwrap();
        let next_i = i + c.len_utf8();

        match quote {
            Quote::Single => {
                if c == '\'' {
                    quote = Quote::None;
                    i = next_i;
                } else {
                    append_literal(&mut word, c, false);
                    i = next_i;
                }
            }
            Quote::Double => {
                if c == '"' {
                    quote = Quote::None;
                    i = next_i;
                } else if c == '\\' {
                    let slash = i;
                    if next_i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: slash });
                    }
                    let d = input[next_i..].chars().next().unwrap();
                    let after = next_i + d.len_utf8();
                    if matches!(d, '"' | '\\' | '$' | '\n') {
                        if d != '\n' {
                            append_literal(&mut word, d, false);
                        }
                        i = after;
                    } else {
                        append_literal(&mut word, '\\', false);
                        i = next_i;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if next_i < input.len() {
                        let d = input[next_i..].chars().next().unwrap();
                        if d == '(' {
                            return Err(SplitError::CommandSubstitution { pos: i });
                        }
                    }
                    if next_i < input.len() && input.as_bytes()[next_i] == b'{' {
                        active_word = true;
                        let (end, value) = expand_braced(input, i, env)?;
                        append_data(&mut word, &value, false);
                        i = end;
                    } else if next_i < input.len() {
                        let d = input[next_i..].chars().next().unwrap();
                        if is_name_start(d) {
                            let mut end = next_i + d.len_utf8();
                            while end < input.len() {
                                let x = input[end..].chars().next().unwrap();
                                if is_name_continue(x) {
                                    end += x.len_utf8();
                                } else {
                                    break;
                                }
                            }
                            active_word = true;
                            let name = &input[next_i..end];
                            let value = first_env(env, name).unwrap_or("");
                            append_data(&mut word, value, false);
                            i = end;
                        } else {
                            append_literal(&mut word, '$', false);
                            i = next_i;
                        }
                    } else {
                        append_literal(&mut word, '$', false);
                        i = next_i;
                    }
                } else {
                    append_literal(&mut word, c, false);
                    i = next_i;
                }
            }
            Quote::None => {
                if is_sep(c) {
                    if active_word {
                        result.push(word);
                        word = Word { text: String::new(), glob: Vec::new() };
                        active_word = false;
                    }
                    i = next_i;
                } else if c == '\'' {
                    active_word = true;
                    quote = Quote::Single;
                    quote_start = i;
                    i = next_i;
                } else if c == '"' {
                    active_word = true;
                    quote = Quote::Double;
                    quote_start = i;
                    i = next_i;
                } else if c == '\\' {
                    if next_i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let d = input[next_i..].chars().next().unwrap();
                    let after = next_i + d.len_utf8();
                    if d == '\n' {
                        i = after;
                    } else {
                        active_word = true;
                        append_literal(&mut word, d, false);
                        i = after;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if next_i < input.len() {
                        let d = input[next_i..].chars().next().unwrap();
                        if d == '(' {
                            return Err(SplitError::CommandSubstitution { pos: i });
                        }
                    }
                    if next_i < input.len() && input.as_bytes()[next_i] == b'{' {
                        active_word = true;
                        let (end, value) = expand_braced(input, i, env)?;
                        append_data(&mut word, &value, true);
                        i = end;
                    } else if next_i < input.len() {
                        let d = input[next_i..].chars().next().unwrap();
                        if is_name_start(d) {
                            let mut end = next_i + d.len_utf8();
                            while end < input.len() {
                                let x = input[end..].chars().next().unwrap();
                                if is_name_continue(x) {
                                    end += x.len_utf8();
                                } else {
                                    break;
                                }
                            }
                            active_word = true;
                            let name = &input[next_i..end];
                            let value = first_env(env, name).unwrap_or("");
                            append_data(&mut word, value, true);
                            i = end;
                        } else {
                            active_word = true;
                            append_literal(&mut word, '$', true);
                            i = next_i;
                        }
                    } else {
                        active_word = true;
                        append_literal(&mut word, '$', true);
                        i = next_i;
                    }
                } else {
                    active_word = true;
                    append_literal(&mut word, c, true);
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
    if active_word {
        result.push(word);
    }
    Ok(result)
}
