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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JoinError {
    Nul { word: usize, pos: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Quote {
    None,
    Single(usize),
    Double(usize),
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_name_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn append_scalar(word: &mut Word, c: char, active_glob: bool) {
    let offset = word.text.len();
    word.text.push(c);
    if active_glob && matches!(c, '*' | '?' | '[') {
        word.glob.push(offset);
    }
}

fn append_string(word: &mut Word, value: &str, active_glob: bool) {
    for c in value.chars() {
        append_scalar(word, c, active_glob);
    }
}

fn lookup<'a>(env: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| (*key == name).then_some(*value))
}

fn parse_braced(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
) -> Result<(usize, String), SplitError> {
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
    if chars.is_empty() || !is_name_start(chars[0].1) {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }

    let mut name_end_char = 1;
    while name_end_char < chars.len() && is_name_continue(chars[name_end_char].1) {
        name_end_char += 1;
    }
    let name_end = if name_end_char == chars.len() {
        contents.len()
    } else {
        chars[name_end_char].0
    };
    let name = &contents[..name_end];

    if name_end == contents.len() {
        return Ok((
            close + 1,
            lookup(env, name).unwrap_or("").to_string(),
        ));
    }

    if !contents[name_end..].starts_with(":-") {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }

    let default_start = name_end + 2;
    let selected = match lookup(env, name) {
        Some(value) if !value.is_empty() => value.to_string(),
        _ => {
            let text = &contents[default_start..];
            let mut decoded = String::new();
            let mut i = 0;
            while i < text.len() {
                let c = text[i..].chars().next().unwrap();
                if c == '\\' {
                    i += 1;
                    if i < text.len() {
                        let next = text[i..].chars().next().unwrap();
                        decoded.push(next);
                        i += next.len_utf8();
                    } else {
                        decoded.push('\\');
                    }
                } else {
                    decoded.push(c);
                    i += c.len_utf8();
                }
            }
            decoded
        }
    };

    Ok((close + 1, selected))
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<Word>, SplitError> {
    if let Some(pos) = input.find('\0') {
        return Err(SplitError::Nul { pos });
    }

    let mut words = Vec::new();
    let mut word = Word {
        text: String::new(),
        glob: Vec::new(),
    };
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
                    append_scalar(&mut word, c, false);
                }
                i = next_i;
            }
            Quote::Double(open) => {
                if c == '"' {
                    quote = Quote::None;
                    i = next_i;
                } else if c == '\\' {
                    if next_i >= input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let n = input[next_i..].chars().next().unwrap();
                    let after_n = next_i + n.len_utf8();
                    if matches!(n, '"' | '\\' | '$') {
                        append_scalar(&mut word, n, false);
                        active = true;
                        i = after_n;
                    } else if n == '\n' {
                        i = after_n;
                    } else {
                        append_scalar(&mut word, '\\', false);
                        active = true;
                        i = next_i;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if next_i < input.len() {
                        let n = input[next_i..].chars().next().unwrap();
                        if n == '(' {
                            return Err(SplitError::CommandSubstitution { pos: i });
                        }
                    }
                    if next_i < input.len() {
                        let n = input[next_i..].chars().next().unwrap();
                        if is_name_start(n) {
                            let mut end = next_i + n.len_utf8();
                            while end < input.len() {
                                let d = input[end..].chars().next().unwrap();
                                if is_name_continue(d) {
                                    end += d.len_utf8();
                                } else {
                                    break;
                                }
                            }
                            let value = lookup(env, &input[next_i..end]).unwrap_or("");
                            append_string(&mut word, value, false);
                            active = true;
                            i = end;
                        } else if n == '{' {
                            let (end, value) = parse_braced(input, i, env)?;
                            append_string(&mut word, &value, false);
                            active = true;
                            i = end;
                        } else {
                            append_scalar(&mut word, '$', false);
                            active = true;
                            i = next_i;
                        }
                    } else {
                        append_scalar(&mut word, '$', false);
                        active = true;
                        i = next_i;
                    }
                } else {
                    append_scalar(&mut word, c, false);
                    active = true;
                    i = next_i;
                }
                let _ = open;
            }
            Quote::None => {
                if matches!(c, ' ' | '\t' | '\r' | '\n') {
                    if active {
                        words.push(word);
                        word = Word {
                            text: String::new(),
                            glob: Vec::new(),
                        };
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
                    if next_i >= input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let n = input[next_i..].chars().next().unwrap();
                    let after_n = next_i + n.len_utf8();
                    if n == '\n' {
                        i = after_n;
                    } else {
                        append_scalar(&mut word, n, false);
                        active = true;
                        i = after_n;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if next_i < input.len() {
                        let n = input[next_i..].chars().next().unwrap();
                        if n == '(' {
                            return Err(SplitError::CommandSubstitution { pos: i });
                        }
                    }
                    if next_i < input.len() {
                        let n = input[next_i..].chars().next().unwrap();
                        if is_name_start(n) {
                            let mut end = next_i + n.len_utf8();
                            while end < input.len() {
                                let d = input[end..].chars().next().unwrap();
                                if is_name_continue(d) {
                                    end += d.len_utf8();
                                } else {
                                    break;
                                }
                            }
                            let value = lookup(env, &input[next_i..end]).unwrap_or("");
                            append_string(&mut word, value, true);
                            active = true;
                            i = end;
                        } else if n == '{' {
                            let (end, value) = parse_braced(input, i, env)?;
                            append_string(&mut word, &value, true);
                            active = true;
                            i = end;
                        } else {
                            append_scalar(&mut word, '$', true);
                            active = true;
                            i = next_i;
                        }
                    } else {
                        append_scalar(&mut word, '$', true);
                        active = true;
                        i = next_i;
                    }
                } else {
                    append_scalar(&mut word, c, true);
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

fn bare_candidate(word: &str) -> Option<String> {
    if word.is_empty() || word.contains('\n') {
        return None;
    }
    let mut out = String::new();
    for c in word.chars() {
        if c.is_ascii_alphanumeric()
            || matches!(c, '_' | '-' | '.' | '/' | ':' | '=' | ',' | '+' | '@' | '%')
        {
            out.push(c);
        } else {
            out.push('\\');
            out.push(c);
        }
    }
    Some(out)
}

fn single_candidate(word: &str) -> Option<String> {
    if word.contains('\'') {
        None
    } else {
        Some(format!("'{}'", word))
    }
}

fn double_candidate(word: &str) -> Option<String> {
    if word.contains('`') {
        return None;
    }
    let mut out = String::new();
    out.push('"');
    for c in word.chars() {
        if matches!(c, '\\' | '"' | '$') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    Some(out)
}

pub fn join(words: &[String]) -> Result<String, JoinError> {
    for (word_index, word) in words.iter().enumerate() {
        if let Some(pos) = word.find('\0') {
            return Err(JoinError::Nul {
                word: word_index,
                pos,
            });
        }
    }

    let mut encoded = Vec::with_capacity(words.len());
    for word in words {
        let mut candidates = Vec::new();
        if let Some(candidate) = bare_candidate(word) {
            candidates.push((candidate, 0usize));
        }
        if let Some(candidate) = single_candidate(word) {
            candidates.push((candidate, 1usize));
        }
        if let Some(candidate) = double_candidate(word) {
            candidates.push((candidate, 2usize));
        }

        if candidates.is_empty() {
            let mut fallback = String::new();
            fallback.push('\'');
            for c in word.chars() {
                if c == '\'' {
                    fallback.push('\'');
                    fallback.push('\\');
                    fallback.push('\'');
                    fallback.push('\'');
                } else {
                    fallback.push(c);
                }
            }
            fallback.push('\'');
            encoded.push(fallback);
        } else {
            candidates.sort_by(|a, b| {
                a.0.len()
                    .cmp(&b.0.len())
                    .then_with(|| a.1.cmp(&b.1))
            });
            encoded.push(candidates.remove(0).0);
        }
    }

    Ok(encoded.join(" "))
}