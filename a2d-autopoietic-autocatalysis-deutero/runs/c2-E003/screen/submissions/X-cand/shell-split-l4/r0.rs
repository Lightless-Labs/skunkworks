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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Quote {
    None,
    Single(usize),
    Double(usize),
}

fn is_name_start(c: char) -> bool {
    c == '_' || c.is_ascii_alphabetic()
}

fn is_name_char(c: char) -> bool {
    is_name_start(c) || c.is_ascii_digit()
}

fn lookup<'a>(name: &str, env: &'a [(&str, &str)]) -> Option<&'a str> {
    env.iter().find(|(key, _)| *key == name).map(|(_, value)| *value)
}

fn append_text(word: &mut Word, value: &str, active_glob: bool) {
    for c in value.chars() {
        if active_glob && matches!(c, '*' | '?' | '[') {
            word.glob.push(word.text.len());
        }
        word.text.push(c);
    }
}

fn decode_default(text: &str) -> String {
    let mut result = String::new();
    let mut chars = text.char_indices().peekable();
    while let Some((_, c)) = chars.next() {
        if c == '\\' {
            if let Some((_, next)) = chars.next() {
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

fn parse_braced(input: &str, dollar: usize, env: &[(&str, &str)]) -> Result<(String, usize), SplitError> {
    let content_start = dollar + 2;
    let mut close = None;
    let mut cursor = content_start;

    while cursor < input.len() {
        let c = input[cursor..].chars().next().unwrap();
        if c == '\\' {
            cursor += c.len_utf8();
            if cursor < input.len() {
                cursor += input[cursor..].chars().next().unwrap().len_utf8();
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
    let mut iter = contents.char_indices();
    match iter.next() {
        Some((_, c)) if is_name_start(c) => name_end = c.len_utf8(),
        _ => return Err(SplitError::InvalidExpansion { pos: dollar }),
    }

    for (offset, c) in contents[name_end..].char_indices() {
        if is_name_char(c) {
            name_end += c.len_utf8();
        } else {
            let _ = offset;
            break;
        }
    }

    let name = &contents[..name_end];
    let rest = &contents[name_end..];
    let value = if rest.is_empty() {
        lookup(name, env).unwrap_or("").to_string()
    } else if let Some(default) = rest.strip_prefix(":-") {
        match lookup(name, env) {
            Some(v) if !v.is_empty() => v.to_string(),
            _ => decode_default(default),
        }
    } else {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    };

    Ok((value, close + 1))
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
    let mut cursor = 0;

    while cursor < input.len() {
        let c = input[cursor..].chars().next().unwrap();
        let next_cursor = cursor + c.len_utf8();

        match quote {
            Quote::Single(_) => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    word.text.push(c);
                }
                cursor = next_cursor;
            }
            Quote::Double(_) => {
                if c == '"' {
                    quote = Quote::None;
                    cursor = next_cursor;
                } else if c == '\\' {
                    let slash_pos = cursor;
                    if next_cursor >= input.len() {
                        return Err(SplitError::TrailingEscape { pos: slash_pos });
                    }
                    let escaped = input[next_cursor..].chars().next().unwrap();
                    let after_escaped = next_cursor + escaped.len_utf8();
                    if matches!(escaped, '"' | '\\' | '$') {
                        word.text.push(escaped);
                        active = true;
                        cursor = after_escaped;
                    } else if escaped == '\n' {
                        cursor = after_escaped;
                    } else {
                        word.text.push('\\');
                        active = true;
                        cursor = next_cursor;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: cursor });
                } else if c == '$' {
                    if input[next_cursor..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: cursor });
                    }
                    if input[next_cursor..].starts_with('{') {
                        let (value, end) = parse_braced(input, cursor, env)?;
                        append_text(&mut word, &value, false);
                        active = true;
                        cursor = end;
                    } else if next_cursor < input.len()
                        && is_name_start(input[next_cursor..].chars().next().unwrap())
                    {
                        let mut end = next_cursor;
                        while end < input.len() {
                            let ch = input[end..].chars().next().unwrap();
                            if !is_name_char(ch) {
                                break;
                            }
                            end += ch.len_utf8();
                        }
                        let name = &input[next_cursor..end];
                        append_text(&mut word, lookup(name, env).unwrap_or(""), false);
                        active = true;
                        cursor = end;
                    } else {
                        word.text.push('$');
                        active = true;
                        cursor = next_cursor;
                    }
                } else {
                    word.text.push(c);
                    active = true;
                    cursor = next_cursor;
                }
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
                    cursor = next_cursor;
                } else if c == '\'' {
                    quote = Quote::Single(cursor);
                    active = true;
                    cursor = next_cursor;
                } else if c == '"' {
                    quote = Quote::Double(cursor);
                    active = true;
                    cursor = next_cursor;
                } else if c == '\\' {
                    if next_cursor >= input.len() {
                        return Err(SplitError::TrailingEscape { pos: cursor });
                    }
                    let escaped = input[next_cursor..].chars().next().unwrap();
                    let after_escaped = next_cursor + escaped.len_utf8();
                    if escaped != '\n' {
                        word.text.push(escaped);
                        active = true;
                    }
                    cursor = after_escaped;
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: cursor });
                } else if c == '$' {
                    if input[next_cursor..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: cursor });
                    }
                    if input[next_cursor..].starts_with('{') {
                        let (value, end) = parse_braced(input, cursor, env)?;
                        append_text(&mut word, &value, true);
                        active = true;
                        cursor = end;
                    } else if next_cursor < input.len()
                        && is_name_start(input[next_cursor..].chars().next().unwrap())
                    {
                        let mut end = next_cursor;
                        while end < input.len() {
                            let ch = input[end..].chars().next().unwrap();
                            if !is_name_char(ch) {
                                break;
                            }
                            end += ch.len_utf8();
                        }
                        let name = &input[next_cursor..end];
                        append_text(&mut word, lookup(name, env).unwrap_or(""), true);
                        active = true;
                        cursor = end;
                    } else {
                        word.text.push('$');
                        active = true;
                        cursor = next_cursor;
                    }
                } else {
                    append_text(&mut word, &c.to_string(), true);
                    active = true;
                    cursor = next_cursor;
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
        if c.is_ascii_alphanumeric() || "_-./:=,+@%".contains(c) {
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
        None
    } else {
        let mut out = String::from("\"");
        for c in word.chars() {
            if matches!(c, '\\' | '"' | '$') {
                out.push('\\');
            }
            out.push(c);
        }
        out.push('"');
        Some(out)
    }
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
            candidates.push((0u8, candidate));
        }
        if let Some(candidate) = single_candidate(word) {
            candidates.push((1u8, candidate));
        }
        if let Some(candidate) = double_candidate(word) {
            candidates.push((2u8, candidate));
        }

        let chosen = if let Some((_, candidate)) = candidates
            .into_iter()
            .min_by_key(|(priority, candidate)| (candidate.len(), *priority))
        {
            candidate
        } else {
            let mut fallback = String::from("'");
            for c in word.chars() {
                if c == '\'' {
                    fallback.push_str("'\\''");
                } else {
                    fallback.push(c);
                }
            }
            fallback.push('\'');
            fallback
        };
        encoded.push(chosen);
    }

    Ok(encoded.join(" "))
}