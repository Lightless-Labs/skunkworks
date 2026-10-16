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

fn is_name_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

fn append_literal(word: &mut Word, c: char, active_glob: bool) {
    let offset = word.text.len();
    word.text.push(c);
    if active_glob && matches!(c, '*' | '?' | '[') {
        word.glob.push(offset);
    }
}

fn append_expansion(word: &mut Word, value: &str, active_glob: bool) {
    for c in value.chars() {
        append_literal(word, c, active_glob);
    }
}

fn find_unescaped_closing_brace(input: &str, start: usize) -> Option<usize> {
    let mut i = start;
    while i < input.len() {
        let c = input[i..].chars().next().unwrap();
        if c == '\\' {
            i += c.len_utf8();
            if i < input.len() {
                let next = input[i..].chars().next().unwrap();
                i += next.len_utf8();
            }
        } else if c == '}' {
            return Some(i);
        } else {
            i += c.len_utf8();
        }
    }
    None
}

fn decode_default(text: &str) -> String {
    let mut result = String::new();
    let mut i = 0;
    while i < text.len() {
        let c = text[i..].chars().next().unwrap();
        i += c.len_utf8();
        if c == '\\' && i < text.len() {
            let next = text[i..].chars().next().unwrap();
            i += next.len_utf8();
            result.push(next);
        } else {
            result.push(c);
        }
    }
    result
}

fn lookup_env<'a>(name: &str, env: &'a [(&str, &str)]) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| if *key == name { Some(*value) } else { None })
}

fn parse_expansion(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
) -> Result<(usize, String), SplitError> {
    let after_dollar = dollar + 1;
    let first = input[after_dollar..].chars().next().unwrap();

    if first == '{' {
        let content_start = after_dollar + 1;
        let close = find_unescaped_closing_brace(input, content_start)
            .ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
        let contents = &input[content_start..close];

        let mut name_end = 0;
        for c in contents.chars() {
            if is_name_continue(c) {
                name_end += c.len_utf8();
            } else {
                break;
            }
        }

        if name_end == 0 {
            return Err(SplitError::InvalidExpansion { pos: dollar });
        }
        let name = &contents[..name_end];
        let mut name_chars = name.chars();
        if !name_chars.next().is_some_and(is_name_start)
            || !name_chars.all(is_name_continue)
        {
            return Err(SplitError::InvalidExpansion { pos: dollar });
        }

        let remainder = &contents[name_end..];
        let value = if remainder.is_empty() {
            lookup_env(name, env).unwrap_or("").to_string()
        } else if let Some(default) = remainder.strip_prefix(":-") {
            match lookup_env(name, env) {
                Some(value) if !value.is_empty() => value.to_string(),
                _ => decode_default(default),
            }
        } else {
            return Err(SplitError::InvalidExpansion { pos: dollar });
        };

        Ok((close + 1, value))
    } else if is_name_start(first) {
        let mut end = after_dollar;
        while end < input.len() {
            let c = input[end..].chars().next().unwrap();
            if !is_name_continue(c) {
                break;
            }
            end += c.len_utf8();
        }
        let name = &input[after_dollar..end];
        Ok((end, lookup_env(name, env).unwrap_or("").to_string()))
    } else {
        unreachable!()
    }
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
        let c_len = c.len_utf8();

        match quote {
            Quote::Single(_) => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    append_literal(&mut current, c, false);
                }
                i += c_len;
            }
            Quote::Double(_) => {
                if c == '"' {
                    quote = Quote::None;
                    i += c_len;
                } else if c == '\\' {
                    let slash = i;
                    i += c_len;
                    if i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: slash });
                    }
                    let next = input[i..].chars().next().unwrap();
                    if matches!(next, '"' | '\\' | '$' | '\n') {
                        if next != '\n' {
                            append_literal(&mut current, next, false);
                        }
                        i += next.len_utf8();
                    } else {
                        append_literal(&mut current, '\\', false);
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    let dollar = i;
                    let next_pos = i + c_len;
                    if next_pos < input.len() {
                        let next = input[next_pos..].chars().next().unwrap();
                        if next == '(' {
                            return Err(SplitError::CommandSubstitution { pos: dollar });
                        }
                        if is_name_start(next) || next == '{' {
                            let (end, value) = parse_expansion(input, dollar, env)?;
                            append_expansion(&mut current, &value, false);
                            active = true;
                            i = end;
                        } else {
                            append_literal(&mut current, '$', false);
                            i += c_len;
                        }
                    } else {
                        append_literal(&mut current, '$', false);
                        i += c_len;
                    }
                } else {
                    append_literal(&mut current, c, false);
                    i += c_len;
                }
            }
            Quote::None => {
                if is_separator(c) {
                    if active {
                        words.push(current);
                        current = Word {
                            text: String::new(),
                            glob: Vec::new(),
                        };
                        active = false;
                    }
                    i += c_len;
                } else if c == '\'' {
                    active = true;
                    quote = Quote::Single(i);
                    i += c_len;
                } else if c == '"' {
                    active = true;
                    quote = Quote::Double(i);
                    i += c_len;
                } else if c == '\\' {
                    let slash = i;
                    i += c_len;
                    if i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: slash });
                    }
                    let next = input[i..].chars().next().unwrap();
                    if next != '\n' {
                        append_literal(&mut current, next, false);
                        active = true;
                    }
                    i += next.len_utf8();
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    let dollar = i;
                    let next_pos = i + c_len;
                    if next_pos < input.len() {
                        let next = input[next_pos..].chars().next().unwrap();
                        if next == '(' {
                            return Err(SplitError::CommandSubstitution { pos: dollar });
                        }
                        if is_name_start(next) || next == '{' {
                            let (end, value) = parse_expansion(input, dollar, env)?;
                            append_expansion(&mut current, &value, true);
                            active = true;
                            i = end;
                        } else {
                            append_literal(&mut current, '$', true);
                            active = true;
                            i += c_len;
                        }
                    } else {
                        append_literal(&mut current, '$', true);
                        active = true;
                        i += c_len;
                    }
                } else {
                    append_literal(&mut current, c, true);
                    active = true;
                    i += c_len;
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
        return None;
    }
    let mut out = String::with_capacity(word.len() + 2);
    out.push('\'');
    out.push_str(word);
    out.push('\'');
    Some(out)
}

fn double_candidate(word: &str) -> Option<String> {
    if word.contains('`') {
        return None;
    }
    let mut out = String::with_capacity(word.len() + 2);
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

        let chosen = if candidates.is_empty() {
            fallback_candidate(word)
        } else {
            candidates
                .into_iter()
                .min_by_key(|(candidate, priority)| (candidate.len(), *priority))
                .unwrap()
                .0
        };
        encoded.push(chosen);
    }

    Ok(encoded.join(" "))
}