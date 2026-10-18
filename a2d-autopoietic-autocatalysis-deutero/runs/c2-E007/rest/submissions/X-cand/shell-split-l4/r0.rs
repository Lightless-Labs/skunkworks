use std::fmt::Write;

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
    c.is_ascii_alphabetic() || c == '_'
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

fn append_literal(word: &mut Word, c: char, active_glob: bool) {
    if active_glob && matches!(c, '*' | '?' | '[') {
        word.glob.push(word.text.len());
    }
    word.text.push(c);
}

fn append_expansion(word: &mut Word, value: &str, active_glob: bool) {
    for c in value.chars() {
        append_literal(word, c, active_glob);
    }
}

fn first_env<'a>(env: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| (*key == name).then_some(*value))
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

fn parse_expansion(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
) -> Result<(usize, String), SplitError> {
    let after_open = dollar + 2;
    let mut i = after_open;
    let mut close = None;

    while i < input.len() {
        let c = input[i..].chars().next().unwrap();
        if c == '\\' {
            i += c.len_utf8();
            if i < input.len() {
                let next = input[i..].chars().next().unwrap();
                i += next.len_utf8();
            }
        } else if c == '}' {
            close = Some(i);
            break;
        } else {
            i += c.len_utf8();
        }
    }

    let close = close.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
    let content = &input[after_open..close];

    let name_len = content
        .char_indices()
        .take_while(|(_, c)| is_name_char(*c))
        .map(|(offset, c)| offset + c.len_utf8())
        .last()
        .unwrap_or(0);

    let name = &content[..name_len];
    let valid_name = !name.is_empty()
        && name.chars().next().is_some_and(is_name_start)
        && name.chars().all(is_name_char);

    if !valid_name {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }

    let rest = &content[name_len..];
    let value = if rest.is_empty() {
        first_env(env, name).unwrap_or("").to_string()
    } else if let Some(default) = rest.strip_prefix(":-") {
        match first_env(env, name) {
            Some(value) if !value.is_empty() => value.to_string(),
            _ => decode_default(default),
        }
    } else {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    };

    Ok((close + 1, value))
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<Word>, SplitError> {
    if let Some(pos) = input.as_bytes().iter().position(|byte| *byte == 0) {
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
                    append_literal(&mut word, c, false);
                }
                i = next_i;
            }
            Quote::Double(open) => {
                if c == '"' {
                    quote = Quote::None;
                    i = next_i;
                } else if c == '\\' {
                    if next_i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let next = input[next_i..].chars().next().unwrap();
                    let after_next = next_i + next.len_utf8();
                    if matches!(next, '"' | '\\' | '$') {
                        append_literal(&mut word, next, false);
                        i = after_next;
                    } else if next == '\n' {
                        i = after_next;
                    } else {
                        append_literal(&mut word, '\\', false);
                        i = next_i;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if input[next_i..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }
                    if input[next_i..].starts_with('{') {
                        let (after, value) = parse_expansion(input, i, env)?;
                        append_expansion(&mut word, &value, false);
                        active = true;
                        i = after;
                    } else if next_i < input.len() {
                        let next = input[next_i..].chars().next().unwrap();
                        if is_name_start(next) {
                            let mut end = next_i + next.len_utf8();
                            while end < input.len() {
                                let ch = input[end..].chars().next().unwrap();
                                if !is_name_char(ch) {
                                    break;
                                }
                                end += ch.len_utf8();
                            }
                            let name = &input[next_i..end];
                            append_expansion(
                                &mut word,
                                first_env(env, name).unwrap_or(""),
                                false,
                            );
                            active = true;
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
                let _ = open;
            }
            Quote::None => {
                if is_separator(c) {
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
                    if next_i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let next = input[next_i..].chars().next().unwrap();
                    if next == '\n' {
                        i = next_i + next.len_utf8();
                    } else {
                        append_literal(&mut word, next, false);
                        active = true;
                        i = next_i + next.len_utf8();
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if input[next_i..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }
                    if input[next_i..].starts_with('{') {
                        let (after, value) = parse_expansion(input, i, env)?;
                        append_expansion(&mut word, &value, true);
                        active = true;
                        i = after;
                    } else if next_i < input.len() {
                        let next = input[next_i..].chars().next().unwrap();
                        if is_name_start(next) {
                            let mut end = next_i + next.len_utf8();
                            while end < input.len() {
                                let ch = input[end..].chars().next().unwrap();
                                if !is_name_char(ch) {
                                    break;
                                }
                                end += ch.len_utf8();
                            }
                            let name = &input[next_i..end];
                            append_expansion(
                                &mut word,
                                first_env(env, name).unwrap_or(""),
                                true,
                            );
                            active = true;
                            i = end;
                        } else {
                            append_literal(&mut word, '$', false);
                            active = true;
                            i = next_i;
                        }
                    } else {
                        append_literal(&mut word, '$', false);
                        active = true;
                        i = next_i;
                    }
                } else {
                    append_literal(&mut word, c, true);
                    active = true;
                    i = next_i;
                }
            }
        }
    }

    match quote {
        Quote::Single(open) => return Err(SplitError::UnterminatedSingle { pos: open }),
        Quote::Double(open) => return Err(SplitError::UnterminatedDouble { pos: open }),
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

    let mut result = String::new();
    for c in word.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | ':' | '=' | ',' | '+' | '@' | '%') {
            result.push(c);
        } else {
            result.push('\\');
            result.push(c);
        }
    }
    Some(result)
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

    let mut result = String::from("\"");
    for c in word.chars() {
        if matches!(c, '\\' | '"' | '$') {
            result.push('\\');
        }
        result.push(c);
    }
    result.push('"');
    Some(result)
}

fn encode_word(word: &str) -> String {
    let mut candidates: Vec<(usize, String)> = Vec::new();

    if let Some(candidate) = bare_candidate(word) {
        candidates.push((0, candidate));
    }
    if let Some(candidate) = single_candidate(word) {
        candidates.push((1, candidate));
    }
    if let Some(candidate) = double_candidate(word) {
        candidates.push((2, candidate));
    }

    if let Some((_, candidate)) = candidates
        .into_iter()
        .min_by_key(|(priority, candidate)| (candidate.len(), *priority))
    {
        return candidate;
    }

    let mut result = String::from("'");
    for c in word.chars() {
        if c == '\'' {
            result.push('\'');
            result.push('\\');
            result.push('\'');
            result.push('\'');
        } else {
            result.push(c);
        }
    }
    result.push('\'');
    result
}

pub fn join(words: &[String]) -> Result<String, JoinError> {
    for (word_index, word) in words.iter().enumerate() {
        if let Some(pos) = word.as_bytes().iter().position(|byte| *byte == 0) {
            return Err(JoinError::Nul {
                word: word_index,
                pos,
            });
        }
    }

    let mut result = String::new();
    for (index, word) in words.iter().enumerate() {
        if index != 0 {
            result.push(' ');
        }
        result.push_str(&encode_word(word));
    }
    Ok(result)
}