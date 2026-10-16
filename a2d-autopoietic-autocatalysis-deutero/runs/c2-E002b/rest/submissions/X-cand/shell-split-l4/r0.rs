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

fn env_value<'a>(name: &str, env: &'a [(&str, &str)]) -> Option<&'a str> {
    env.iter().find(|(key, _)| *key == name).map(|(_, value)| *value)
}

fn append_text(word: &mut Word, text: &str, glob_active: bool) {
    for c in text.chars() {
        if glob_active && matches!(c, '*' | '?' | '[') {
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

fn find_braced_end(input: &str, start: usize) -> Option<usize> {
    let mut it = input[start..].char_indices().peekable();
    while let Some((offset, c)) = it.next() {
        if c == '\\' {
            it.next();
        } else if c == '}' {
            return Some(start + offset);
        }
    }
    None
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<Word>, SplitError> {
    if let Some(pos) = input.find('\0') {
        return Err(SplitError::Nul { pos });
    }

    let mut words = Vec::new();
    let mut current = Word {
        text: String::new(),
        glob: Vec::new(),
    };
    let mut active = false;
    let mut quote = Quote::None;
    let mut i = 0;

    while i < input.len() {
        let c = input[i..].chars().next().unwrap();
        let len = c.len_utf8();

        match quote {
            Quote::Single(_) => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    current.text.push(c);
                }
                i += len;
            }
            Quote::Double(_) => {
                match c {
                    '"' => {
                        quote = Quote::None;
                        i += len;
                    }
                    '\\' => {
                        let esc_pos = i;
                        i += 1;
                        if i == input.len() {
                            return Err(SplitError::TrailingEscape { pos: esc_pos });
                        }
                        let next = input[i..].chars().next().unwrap();
                        let next_len = next.len_utf8();
                        if matches!(next, '"' | '\\' | '$' | '\n') {
                            if next != '\n' {
                                current.text.push(next);
                            }
                            i += next_len;
                        } else {
                            current.text.push('\\');
                        }
                    }
                    '$' => {
                        let dollar = i;
                        let after = i + 1;
                        if after < input.len() {
                            let next = input[after..].chars().next().unwrap();
                            if next == '(' {
                                return Err(SplitError::CommandSubstitution { pos: dollar });
                            }
                            if next == '{' {
                                let content_start = after + 1;
                                let Some(close) = find_braced_end(input, content_start) else {
                                    return Err(SplitError::UnterminatedExpansion { pos: dollar });
                                };
                                let content = &input[content_start..close];
                                let (name, default) = parse_braced_content(content)
                                    .ok_or(SplitError::InvalidExpansion { pos: dollar })?;
                                active = true;
                                if let Some(default_text) = default {
                                    match env_value(name, env) {
                                        Some(value) if !value.is_empty() => {
                                            append_text(&mut current, value, false);
                                        }
                                        _ => {
                                            let decoded = decode_default(default_text);
                                            append_text(&mut current, &decoded, false);
                                        }
                                    }
                                } else if let Some(value) = env_value(name, env) {
                                    append_text(&mut current, value, false);
                                }
                                i = close + 1;
                            } else if is_name_start(next) {
                                let name_start = after;
                                let mut end = after + next.len_utf8();
                                while end < input.len() {
                                    let ch = input[end..].chars().next().unwrap();
                                    if !is_name_char(ch) {
                                        break;
                                    }
                                    end += ch.len_utf8();
                                }
                                active = true;
                                let name = &input[name_start..end];
                                if let Some(value) = env_value(name, env) {
                                    append_text(&mut current, value, false);
                                }
                                i = end;
                            } else {
                                current.text.push('$');
                                i += 1;
                            }
                        } else {
                            current.text.push('$');
                            i += 1;
                        }
                    }
                    '`' => return Err(SplitError::CommandSubstitution { pos: i }),
                    _ => {
                        current.text.push(c);
                        i += len;
                    }
                }
            }
            Quote::None => {
                match c {
                    ' ' | '\t' | '\r' | '\n' => {
                        if active {
                            words.push(current);
                            current = Word {
                                text: String::new(),
                                glob: Vec::new(),
                            };
                            active = false;
                        }
                        i += len;
                    }
                    '\'' => {
                        active = true;
                        quote = Quote::Single(i);
                        i += len;
                    }
                    '"' => {
                        active = true;
                        quote = Quote::Double(i);
                        i += len;
                    }
                    '\\' => {
                        let esc_pos = i;
                        i += 1;
                        if i == input.len() {
                            return Err(SplitError::TrailingEscape { pos: esc_pos });
                        }
                        let next = input[i..].chars().next().unwrap();
                        let next_len = next.len_utf8();
                        if next == '\n' {
                            i += next_len;
                        } else {
                            active = true;
                            current.text.push(next);
                            i += next_len;
                        }
                    }
                    '$' => {
                        let dollar = i;
                        let after = i + 1;
                        if after < input.len() {
                            let next = input[after..].chars().next().unwrap();
                            if next == '(' {
                                return Err(SplitError::CommandSubstitution { pos: dollar });
                            }
                            if next == '{' {
                                let content_start = after + 1;
                                let Some(close) = find_braced_end(input, content_start) else {
                                    return Err(SplitError::UnterminatedExpansion { pos: dollar });
                                };
                                let content = &input[content_start..close];
                                let (name, default) = parse_braced_content(content)
                                    .ok_or(SplitError::InvalidExpansion { pos: dollar })?;
                                active = true;
                                if let Some(default_text) = default {
                                    match env_value(name, env) {
                                        Some(value) if !value.is_empty() => {
                                            append_text(&mut current, value, true);
                                        }
                                        _ => {
                                            let decoded = decode_default(default_text);
                                            append_text(&mut current, &decoded, true);
                                        }
                                    }
                                } else if let Some(value) = env_value(name, env) {
                                    append_text(&mut current, value, true);
                                }
                                i = close + 1;
                            } else if is_name_start(next) {
                                let name_start = after;
                                let mut end = after + next.len_utf8();
                                while end < input.len() {
                                    let ch = input[end..].chars().next().unwrap();
                                    if !is_name_char(ch) {
                                        break;
                                    }
                                    end += ch.len_utf8();
                                }
                                active = true;
                                let name = &input[name_start..end];
                                if let Some(value) = env_value(name, env) {
                                    append_text(&mut current, value, true);
                                }
                                i = end;
                            } else {
                                active = true;
                                current.text.push('$');
                                i += 1;
                            }
                        } else {
                            active = true;
                            current.text.push('$');
                            i += 1;
                        }
                    }
                    '`' => return Err(SplitError::CommandSubstitution { pos: i }),
                    _ => {
                        active = true;
                        if matches!(c, '*' | '?' | '[') {
                            current.glob.push(current.text.len());
                        }
                        current.text.push(c);
                        i += len;
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
        words.push(current);
    }
    Ok(words)
}

fn parse_braced_content(content: &str) -> Option<(&str, Option<&str>)> {
    let mut name_end = 0;
    for (idx, c) in content.char_indices() {
        if (idx == 0 && is_name_start(c)) || (idx > 0 && is_name_char(c)) {
            name_end = idx + c.len_utf8();
        } else {
            break;
        }
    }
    if name_end == 0 {
        return None;
    }

    let name = &content[..name_end];
    let rest = &content[name_end..];
    if rest.is_empty() {
        Some((name, None))
    } else if let Some(default) = rest.strip_prefix(":-") {
        Some((name, Some(default)))
    } else {
        None
    }
}

fn bare_candidate(word: &str) -> Option<String> {
    if word.is_empty() || word.contains('\n') {
        return None;
    }
    let mut encoded = String::new();
    for c in word.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | ':' | '=' | ',' | '+' | '@' | '%') {
            encoded.push(c);
        } else {
            encoded.push('\\');
            encoded.push(c);
        }
    }
    Some(encoded)
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
    let mut encoded = String::from("\"");
    for c in word.chars() {
        if matches!(c, '\\' | '"' | '$') {
            encoded.push('\\');
        }
        encoded.push(c);
    }
    encoded.push('"');
    Some(encoded)
}

fn fallback_candidate(word: &str) -> String {
    let mut encoded = String::from("'");
    for c in word.chars() {
        if c == '\'' {
            encoded.push_str("'\\''");
        } else {
            encoded.push(c);
        }
    }
    encoded.push('\'');
    encoded
}

fn encode_word(word: &str) -> String {
    let candidates = [
        bare_candidate(word).map(|s| (0usize, s)),
        single_candidate(word).map(|s| (1usize, s)),
        double_candidate(word).map(|s| (2usize, s)),
    ];

    candidates
        .into_iter()
        .flatten()
        .min_by_key(|(priority, value)| (value.len(), *priority))
        .map(|(_, value)| value)
        .unwrap_or_else(|| fallback_candidate(word))
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

    Ok(words
        .iter()
        .map(|word| encode_word(word))
        .collect::<Vec<_>>()
        .join(" "))
}