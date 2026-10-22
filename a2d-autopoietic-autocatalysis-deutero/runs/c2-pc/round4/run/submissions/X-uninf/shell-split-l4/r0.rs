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
    c.is_ascii_alphabetic() || c == '_'
}

fn is_name_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn env_value<'a>(env: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| (*key == name).then_some(*value))
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

fn next_char(input: &str, pos: usize) -> Option<(char, usize)> {
    input[pos..]
        .chars()
        .next()
        .map(|c| (c, pos + c.len_utf8()))
}

fn parse_expansion(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
) -> Result<(usize, String), SplitError> {
    let after_dollar = dollar + 1;
    let mut cursor = after_dollar + 1; // after '{'
    let mut close = None;

    while cursor < input.len() {
        let (c, end) = next_char(input, cursor).unwrap();
        if c == '\\' {
            if end < input.len() {
                cursor = next_char(input, end).unwrap().1;
            } else {
                cursor = end;
            }
        } else if c == '}' {
            close = Some(cursor);
            break;
        } else {
            cursor = end;
        }
    }

    let close = close.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
    let contents = &input[after_dollar + 1..close];
    let bytes = contents.as_bytes();

    let mut name_end = 0;
    if bytes.first().copied().map(|b| b.is_ascii_alphabetic() || b == b'_') != Some(true) {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }
    name_end += 1;
    while name_end < bytes.len()
        && (bytes[name_end].is_ascii_alphanumeric() || bytes[name_end] == b'_')
    {
        name_end += 1;
    }

    let name = &contents[..name_end];
    let value = if name_end == bytes.len() {
        env_value(env, name).unwrap_or("").to_string()
    } else if bytes.get(name_end..name_end + 2) == Some(b":-") {
        let default_start = name_end + 2;
        let selected = env_value(env, name).filter(|value| !value.is_empty());
        if let Some(value) = selected {
            value.to_string()
        } else {
            let default = &contents[default_start..];
            let mut decoded = String::new();
            let mut i = 0;
            while i < default.len() {
                let (c, end) = next_char(default, i).unwrap();
                if c == '\\' && end < default.len() {
                    let (escaped, escaped_end) = next_char(default, end).unwrap();
                    decoded.push(escaped);
                    i = escaped_end;
                } else {
                    decoded.push(c);
                    i = end;
                }
            }
            decoded
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

    let mut words = Vec::new();
    let mut word = Word {
        text: String::new(),
        glob: Vec::new(),
    };
    let mut active = false;
    let mut quote = Quote::None;
    let mut quote_start = 0;
    let mut i = 0;

    while i < input.len() {
        let (c, end) = next_char(input, i).unwrap();

        match quote {
            Quote::Single => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    word.text.push(c);
                }
                i = end;
            }
            Quote::Double => {
                if c == '"' {
                    quote = Quote::None;
                    i = end;
                } else if c == '\\' {
                    if end == input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let (next, next_end) = next_char(input, end).unwrap();
                    if matches!(next, '"' | '\\' | '$' | '\n') {
                        if next != '\n' {
                            word.text.push(next);
                        }
                        i = next_end;
                    } else {
                        word.text.push('\\');
                        i = end;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if end < input.len() {
                        let (next, _) = next_char(input, end).unwrap();
                        if next == '(' {
                            return Err(SplitError::CommandSubstitution { pos: i });
                        }
                        if next == '{' {
                            let (next_i, value) = parse_expansion(input, i, env)?;
                            active = true;
                            append_expansion(&mut word, &value, false);
                            i = next_i;
                            continue;
                        }
                        if is_name_start(next) {
                            let mut name_end = end;
                            while name_end < input.len() {
                                let (ch, ch_end) = next_char(input, name_end).unwrap();
                                if is_name_continue(ch) {
                                    name_end = ch_end;
                                } else {
                                    break;
                                }
                            }
                            let name = &input[end..name_end];
                            active = true;
                            append_expansion(
                                &mut word,
                                env_value(env, name).unwrap_or(""),
                                false,
                            );
                            i = name_end;
                            continue;
                        }
                    }
                    word.text.push('$');
                    i = end;
                } else {
                    word.text.push(c);
                    i = end;
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
                    i = end;
                } else if c == '\'' {
                    active = true;
                    quote = Quote::Single;
                    quote_start = i;
                    i = end;
                } else if c == '"' {
                    active = true;
                    quote = Quote::Double;
                    quote_start = i;
                    i = end;
                } else if c == '\\' {
                    if end == input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let (next, next_end) = next_char(input, end).unwrap();
                    if next != '\n' {
                        active = true;
                        word.text.push(next);
                    }
                    i = next_end;
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if end < input.len() {
                        let (next, _) = next_char(input, end).unwrap();
                        if next == '(' {
                            return Err(SplitError::CommandSubstitution { pos: i });
                        }
                        if next == '{' {
                            let (next_i, value) = parse_expansion(input, i, env)?;
                            active = true;
                            append_expansion(&mut word, &value, true);
                            i = next_i;
                            continue;
                        }
                        if is_name_start(next) {
                            let mut name_end = end;
                            while name_end < input.len() {
                                let (ch, ch_end) = next_char(input, name_end).unwrap();
                                if is_name_continue(ch) {
                                    name_end = ch_end;
                                } else {
                                    break;
                                }
                            }
                            let name = &input[end..name_end];
                            active = true;
                            append_expansion(
                                &mut word,
                                env_value(env, name).unwrap_or(""),
                                true,
                            );
                            i = name_end;
                            continue;
                        }
                    }
                    active = true;
                    word.text.push('$');
                    i = end;
                } else {
                    active = true;
                    append_literal(&mut word, c, true);
                    i = end;
                }
            }
        }
    }

    match quote {
        Quote::Single => return Err(SplitError::UnterminatedSingle { pos: quote_start }),
        Quote::Double => return Err(SplitError::UnterminatedDouble { pos: quote_start }),
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
        if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | ':' | '=' | ',' | '+' | '@' | '%') {
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
            candidates.push((0usize, candidate));
        }
        if let Some(candidate) = single_candidate(word) {
            candidates.push((1usize, candidate));
        }
        if let Some(candidate) = double_candidate(word) {
            candidates.push((2usize, candidate));
        }

        let choice = if candidates.is_empty() {
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
        } else {
            candidates.sort_by(|a, b| {
                a.1.len()
                    .cmp(&b.1.len())
                    .then_with(|| a.0.cmp(&b.0))
            });
            candidates.remove(0).1
        };
        encoded.push(choice);
    }

    Ok(encoded.join(" "))
}