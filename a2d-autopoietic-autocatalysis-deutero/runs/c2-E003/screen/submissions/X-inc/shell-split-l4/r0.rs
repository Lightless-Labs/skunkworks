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
    Single,
    Double,
}

fn is_name_start(c: char) -> bool {
    c == '_' || c.is_ascii_alphabetic()
}

fn is_name_continue(c: char) -> bool {
    is_name_start(c) || c.is_ascii_digit()
}

fn env_value<'a>(name: &str, env: &'a [(&str, &str)]) -> Option<&'a str> {
    env.iter().find(|(key, _)| *key == name).map(|(_, value)| *value)
}

fn append_literal(word: &mut Word, c: char, wildcard_active: bool) {
    if wildcard_active && matches!(c, '*' | '?' | '[') {
        word.glob.push(word.text.len());
    }
    word.text.push(c);
}

fn append_expansion(word: &mut Word, value: &str, wildcard_active: bool) {
    for c in value.chars() {
        append_literal(word, c, wildcard_active);
    }
}

fn decode_default(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();

    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            } else {
                out.push('\\');
            }
        } else {
            out.push(c);
        }
    }

    out
}

fn parse_expansion(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
) -> Result<(usize, String), SplitError> {
    let after_open = dollar + 2;
    let mut cursor = after_open;
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

    let close = close.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
    let contents = &input[after_open..close];

    let mut name_end = 0;
    let mut content_chars = contents.char_indices();
    match content_chars.next() {
        Some((_, c)) if is_name_start(c) => {
            name_end = c.len_utf8();
            for (offset, c) in content_chars {
                if is_name_continue(c) {
                    name_end = offset + c.len_utf8();
                } else {
                    break;
                }
            }
        }
        _ => return Err(SplitError::InvalidExpansion { pos: dollar }),
    }

    let name = &contents[..name_end];
    let remainder = &contents[name_end..];
    let selected = if remainder.is_empty() {
        env_value(name, env).unwrap_or("").to_string()
    } else if let Some(default_text) = remainder.strip_prefix(":-") {
        match env_value(name, env) {
            Some(value) if !value.is_empty() => value.to_string(),
            _ => decode_default(default_text),
        }
    } else {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    };

    Ok((close + 1, selected))
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<Word>, SplitError> {
    if let Some(pos) = input.as_bytes().iter().position(|&byte| byte == 0) {
        return Err(SplitError::Nul { pos });
    }

    let mut words = Vec::new();
    let mut current = Word {
        text: String::new(),
        glob: Vec::new(),
    };
    let mut active = false;
    let mut quote = Quote::None;
    let mut quote_open = 0usize;
    let mut cursor = 0usize;

    while cursor < input.len() {
        let c = input[cursor..].chars().next().unwrap();
        let width = c.len_utf8();

        match quote {
            Quote::Single => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    current.text.push(c);
                }
                cursor += width;
            }
            Quote::Double => {
                if c == '"' {
                    quote = Quote::None;
                    cursor += width;
                } else if c == '\\' {
                    let escape_pos = cursor;
                    cursor += width;
                    if cursor >= input.len() {
                        return Err(SplitError::TrailingEscape { pos: escape_pos });
                    }
                    let next = input[cursor..].chars().next().unwrap();
                    match next {
                        '"' | '\\' | '$' => {
                            current.text.push(next);
                            cursor += next.len_utf8();
                        }
                        '\n' => {
                            cursor += next.len_utf8();
                        }
                        _ => current.text.push('\\'),
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: cursor });
                } else if c == '$' {
                    let next_pos = cursor + width;
                    if input[next_pos..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: cursor });
                    }
                    if input[next_pos..].starts_with('{') {
                        let (end, value) = parse_expansion(input, cursor, env)?;
                        append_expansion(&mut current, &value, false);
                        active = true;
                        cursor = end;
                    } else if let Some(next) = input[next_pos..].chars().next() {
                        if is_name_start(next) {
                            let mut end = next_pos + next.len_utf8();
                            while end < input.len() {
                                let ch = input[end..].chars().next().unwrap();
                                if is_name_continue(ch) {
                                    end += ch.len_utf8();
                                } else {
                                    break;
                                }
                            }
                            let name = &input[next_pos..end];
                            append_expansion(
                                &mut current,
                                env_value(name, env).unwrap_or(""),
                                false,
                            );
                            active = true;
                            cursor = end;
                        } else {
                            current.text.push('$');
                            cursor += width;
                        }
                    } else {
                        current.text.push('$');
                        cursor += width;
                    }
                } else {
                    current.text.push(c);
                    cursor += width;
                }
            }
            Quote::None => {
                if matches!(c, ' ' | '\t' | '\r' | '\n') {
                    if active {
                        words.push(current);
                        current = Word {
                            text: String::new(),
                            glob: Vec::new(),
                        };
                        active = false;
                    }
                    cursor += width;
                } else if c == '\'' {
                    quote = Quote::Single;
                    quote_open = cursor;
                    active = true;
                    cursor += width;
                } else if c == '"' {
                    quote = Quote::Double;
                    quote_open = cursor;
                    active = true;
                    cursor += width;
                } else if c == '\\' {
                    let escape_pos = cursor;
                    cursor += width;
                    if cursor >= input.len() {
                        return Err(SplitError::TrailingEscape { pos: escape_pos });
                    }
                    let next = input[cursor..].chars().next().unwrap();
                    cursor += next.len_utf8();
                    if next == '\n' {
                        continue;
                    }
                    append_literal(&mut current, next, false);
                    active = true;
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: cursor });
                } else if c == '$' {
                    let next_pos = cursor + width;
                    if input[next_pos..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: cursor });
                    }
                    if input[next_pos..].starts_with('{') {
                        let (end, value) = parse_expansion(input, cursor, env)?;
                        append_expansion(&mut current, &value, true);
                        active = true;
                        cursor = end;
                    } else if let Some(next) = input[next_pos..].chars().next() {
                        if is_name_start(next) {
                            let mut end = next_pos + next.len_utf8();
                            while end < input.len() {
                                let ch = input[end..].chars().next().unwrap();
                                if is_name_continue(ch) {
                                    end += ch.len_utf8();
                                } else {
                                    break;
                                }
                            }
                            let name = &input[next_pos..end];
                            append_expansion(
                                &mut current,
                                env_value(name, env).unwrap_or(""),
                                true,
                            );
                            active = true;
                            cursor = end;
                        } else {
                            append_literal(&mut current, '$', true);
                            active = true;
                            cursor += width;
                        }
                    } else {
                        append_literal(&mut current, '$', true);
                        active = true;
                        cursor += width;
                    }
                } else {
                    append_literal(&mut current, c, true);
                    active = true;
                    cursor += width;
                }
            }
        }
    }

    match quote {
        Quote::Single => return Err(SplitError::UnterminatedSingle { pos: quote_open }),
        Quote::Double => return Err(SplitError::UnterminatedDouble { pos: quote_open }),
        Quote::None => {}
    }

    if active {
        words.push(current);
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
        Some(format!("'{word}'"))
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

fn fallback_candidate(word: &str) -> String {
    let mut out = String::new();
    out.push('\'');
    for c in word.chars() {
        if c == '\'' {
            out.push('\'');
            out.push('\\');
            out.push('\'');
            out.push('\'');
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

pub fn join(words: &[String]) -> Result<String, JoinError> {
    for (word_index, word) in words.iter().enumerate() {
        if let Some(pos) = word.as_bytes().iter().position(|&byte| byte == 0) {
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
            candidates.push((0usize, candidate));
        }
        if let Some(candidate) = single_candidate(word) {
            candidates.push((1usize, candidate));
        }
        if let Some(candidate) = double_candidate(word) {
            candidates.push((2usize, candidate));
        }

        let result = if candidates.is_empty() {
            fallback_candidate(word)
        } else {
            candidates
                .into_iter()
                .min_by_key(|(priority, candidate)| (candidate.len(), *priority))
                .unwrap()
                .1
        };
        encoded.push(result);
    }

    Ok(encoded.join(" "))
}