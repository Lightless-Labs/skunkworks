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

fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_name_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
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

fn lookup_env<'a>(env: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| (*key == name).then_some(*value))
}

fn char_at(input: &str, pos: usize) -> Option<char> {
    input.get(pos..)?.chars().next()
}

fn advance(input: &str, pos: &mut usize) -> Option<char> {
    let c = char_at(input, *pos)?;
    *pos += c.len_utf8();
    Some(c)
}

fn decode_default(text: &str) -> String {
    let mut result = String::new();
    let mut pos = 0;
    while let Some(c) = advance(text, &mut pos) {
        if c == '\\' {
            if let Some(next) = advance(text, &mut pos) {
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

fn parse_expansion(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
) -> Result<(String, usize), SplitError> {
    let mut pos = dollar + 2; // "${"
    let mut close = None;

    while pos < input.len() {
        let c = char_at(input, pos).expect("valid UTF-8 boundary");
        if c == '\\' {
            pos += 1;
            if pos < input.len() {
                let next = char_at(input, pos).expect("valid UTF-8 boundary");
                pos += next.len_utf8();
            }
        } else if c == '}' {
            close = Some(pos);
            break;
        } else {
            pos += c.len_utf8();
        }
    }

    let close = close.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
    let contents = &input[dollar + 2..close];

    let mut name_end = 0;
    let mut chars = contents.char_indices();
    match chars.next() {
        Some((_, first)) if is_name_start(first) => {
            name_end = first.len_utf8();
        }
        _ => return Err(SplitError::InvalidExpansion { pos: dollar }),
    }

    for (offset, c) in contents[name_end..].char_indices() {
        if is_name_continue(c) {
            name_end += c.len_utf8();
        } else {
            let _ = offset;
            break;
        }
    }

    let name = &contents[..name_end];
    let rest = &contents[name_end..];

    let value = if rest.is_empty() {
        lookup_env(env, name).unwrap_or("").to_string()
    } else if let Some(default) = rest.strip_prefix(":-") {
        match lookup_env(env, name) {
            Some(value) if !value.is_empty() => value.to_string(),
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
    let mut current = Word {
        text: String::new(),
        glob: Vec::new(),
    };
    let mut active = false;
    let mut quote = Quote::None;
    let mut pos = 0;

    while pos < input.len() {
        let c = char_at(input, pos).expect("valid UTF-8 boundary");

        match quote {
            Quote::Single(_) => {
                pos += c.len_utf8();
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    current.text.push(c);
                }
            }
            Quote::Double(_) => {
                if c == '"' {
                    pos += 1;
                    quote = Quote::None;
                } else if c == '\\' {
                    let slash = pos;
                    pos += 1;
                    if pos >= input.len() {
                        return Err(SplitError::TrailingEscape { pos: slash });
                    }
                    let next = char_at(input, pos).expect("valid UTF-8 boundary");
                    if matches!(next, '"' | '\\' | '$' | '\n') {
                        pos += next.len_utf8();
                        if next != '\n' {
                            current.text.push(next);
                        }
                    } else {
                        current.text.push('\\');
                    }
                } else if c == '$' {
                    if char_at(input, pos + 1) == Some('(') {
                        return Err(SplitError::CommandSubstitution { pos });
                    }
                    if char_at(input, pos + 1) == Some('{') {
                        active = true;
                        let dollar = pos;
                        let (value, end) = parse_expansion(input, pos, env)?;
                        append_expansion(&mut current, &value, false);
                        pos = end;
                    } else if char_at(input, pos + 1).is_some_and(is_name_start) {
                        active = true;
                        pos += 1;
                        let start = pos;
                        while let Some(next) = char_at(input, pos) {
                            if is_name_continue(next) {
                                pos += next.len_utf8();
                            } else {
                                break;
                            }
                        }
                        append_expansion(
                            &mut current,
                            lookup_env(env, &input[start..pos]).unwrap_or(""),
                            false,
                        );
                    } else {
                        current.text.push('$');
                        pos += 1;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos });
                } else {
                    current.text.push(c);
                    pos += c.len_utf8();
                }
            }
            Quote::None => {
                if is_separator(c) {
                    pos += c.len_utf8();
                    if active {
                        words.push(current);
                        current = Word {
                            text: String::new(),
                            glob: Vec::new(),
                        };
                        active = false;
                    }
                } else if c == '\'' {
                    active = true;
                    quote = Quote::Single(pos);
                    pos += 1;
                } else if c == '"' {
                    active = true;
                    quote = Quote::Double(pos);
                    pos += 1;
                } else if c == '\\' {
                    let slash = pos;
                    pos += 1;
                    if pos >= input.len() {
                        return Err(SplitError::TrailingEscape { pos: slash });
                    }
                    let next = char_at(input, pos).expect("valid UTF-8 boundary");
                    pos += next.len_utf8();
                    if next != '\n' {
                        active = true;
                        current.text.push(next);
                    }
                } else if c == '$' {
                    if char_at(input, pos + 1) == Some('(') {
                        return Err(SplitError::CommandSubstitution { pos });
                    }
                    if char_at(input, pos + 1) == Some('{') {
                        active = true;
                        let (value, end) = parse_expansion(input, pos, env)?;
                        append_expansion(&mut current, &value, true);
                        pos = end;
                    } else if char_at(input, pos + 1).is_some_and(is_name_start) {
                        active = true;
                        pos += 1;
                        let start = pos;
                        while let Some(next) = char_at(input, pos) {
                            if is_name_continue(next) {
                                pos += next.len_utf8();
                            } else {
                                break;
                            }
                        }
                        append_expansion(
                            &mut current,
                            lookup_env(env, &input[start..pos]).unwrap_or(""),
                            true,
                        );
                    } else {
                        current.text.push('$');
                        pos += 1;
                        active = true;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos });
                } else {
                    active = true;
                    append_literal(&mut current, c, true);
                    pos += c.len_utf8();
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
        words.push(current);
    }
    Ok(words)
}

fn bare_candidate(word: &str) -> Option<String> {
    if word.is_empty() || word.contains('\n') {
        return None;
    }
    let mut result = String::new();
    for c in word.chars() {
        if c.is_ascii_alphanumeric() || "_-./:=,+@%".contains(c) {
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

fn fallback_candidate(word: &str) -> String {
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
        if let Some(pos) = word.find('\0') {
            return Err(JoinError::Nul {
                word: word_index,
                pos,
            });
        }
    }

    let mut encoded = Vec::with_capacity(words.len());
    for word in words {
        let candidates = [
            bare_candidate(word),
            single_candidate(word),
            double_candidate(word),
        ];
        let mut best: Option<(usize, String)> = None;
        for candidate in candidates.into_iter().flatten() {
            let len = candidate.len();
            if best.as_ref().is_none_or(|(best_len, _)| len < *best_len) {
                best = Some((len, candidate));
            }
        }

        let value = best
            .map(|(_, value)| value)
            .unwrap_or_else(|| fallback_candidate(word));
        encoded.push(value);
    }

    Ok(encoded.join(" "))
}