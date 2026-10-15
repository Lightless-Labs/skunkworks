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

fn lookup<'a>(env: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| (*key == name).then_some(*value))
}

fn decode_default(text: &str) -> String {
    let mut result = String::new();
    let mut iter = text.char_indices().peekable();

    while let Some((_, c)) = iter.next() {
        if c == '\\' {
            if let Some((_, escaped)) = iter.next() {
                result.push(escaped);
            }
        } else {
            result.push(c);
        }
    }

    result
}

fn parse_braced(
    input: &str,
    dollar: usize,
    after_open: usize,
) -> Result<(usize, String), SplitError> {
    let mut cursor = after_open;
    let mut closing = None;

    while cursor < input.len() {
        let c = input[cursor..].chars().next().unwrap();
        if c == '\\' {
            cursor += 1;
            if cursor < input.len() {
                cursor += input[cursor..].chars().next().unwrap().len_utf8();
            }
        } else if c == '}' {
            closing = Some(cursor);
            break;
        } else {
            cursor += c.len_utf8();
        }
    }

    let close_pos = closing.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
    let contents = &input[after_open..close_pos];

    let mut name_end = 0;
    let mut chars = contents.char_indices();
    let first = chars.next().map(|(_, c)| c);
    if !first.map(is_name_start).unwrap_or(false) {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }

    name_end = first.unwrap().len_utf8();
    for (offset, c) in chars {
        if is_name_continue(c) {
            name_end = offset + c.len_utf8();
        } else {
            break;
        }
    }

    let name = &contents[..name_end];
    let remainder = &contents[name_end..];

    let value = if remainder.is_empty() {
        lookup(&[], name).unwrap_or("").to_string()
    } else if let Some(default_text) = remainder.strip_prefix(":-") {
        decode_default(default_text)
    } else {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    };

    Ok((close_pos + 1, value))
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
    let mut quote_start = 0;
    let mut cursor = 0;

    while cursor < input.len() {
        let c = input[cursor..].chars().next().unwrap();
        let next_cursor = cursor + c.len_utf8();

        match quote {
            Quote::Single => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    append_literal(&mut current, c, false);
                }
                cursor = next_cursor;
            }
            Quote::Double => {
                if c == '"' {
                    quote = Quote::None;
                    cursor = next_cursor;
                } else if c == '\\' {
                    if next_cursor >= input.len() {
                        return Err(SplitError::TrailingEscape { pos: cursor });
                    }
                    let escaped = input[next_cursor..].chars().next().unwrap();
                    match escaped {
                        '"' | '\\' | '$' => {
                            append_literal(&mut current, escaped, false);
                            cursor = next_cursor + escaped.len_utf8();
                        }
                        '\n' => {
                            cursor = next_cursor + 1;
                        }
                        _ => {
                            append_literal(&mut current, '\\', false);
                            cursor = next_cursor;
                        }
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: cursor });
                } else if c == '$' {
                    if input[next_cursor..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: cursor });
                    }
                    if input[next_cursor..].starts_with('{') {
                        let (after, value) = parse_braced(input, cursor, next_cursor + 1)?;
                        append_expansion(&mut current, &value, false);
                        active = true;
                        cursor = after;
                    } else if let Some(name_start) = input[next_cursor..].chars().next() {
                        if is_name_start(name_start) {
                            let mut end = next_cursor + name_start.len_utf8();
                            while end < input.len() {
                                let ch = input[end..].chars().next().unwrap();
                                if is_name_continue(ch) {
                                    end += ch.len_utf8();
                                } else {
                                    break;
                                }
                            }
                            let value = lookup(env, &input[next_cursor..end]).unwrap_or("");
                            append_expansion(&mut current, value, false);
                            active = true;
                            cursor = end;
                        } else {
                            append_literal(&mut current, '$', false);
                            cursor = next_cursor;
                        }
                    } else {
                        append_literal(&mut current, '$', false);
                        cursor = next_cursor;
                    }
                } else {
                    append_literal(&mut current, c, false);
                    cursor = next_cursor;
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
                    cursor = next_cursor;
                } else if c == '\'' {
                    quote = Quote::Single;
                    quote_start = cursor;
                    active = true;
                    cursor = next_cursor;
                } else if c == '"' {
                    quote = Quote::Double;
                    quote_start = cursor;
                    active = true;
                    cursor = next_cursor;
                } else if c == '\\' {
                    if next_cursor >= input.len() {
                        return Err(SplitError::TrailingEscape { pos: cursor });
                    }
                    let escaped = input[next_cursor..].chars().next().unwrap();
                    if escaped != '\n' {
                        append_literal(&mut current, escaped, false);
                        active = true;
                    }
                    cursor = next_cursor + escaped.len_utf8();
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: cursor });
                } else if c == '$' {
                    if input[next_cursor..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: cursor });
                    }
                    if input[next_cursor..].starts_with('{') {
                        let (after, value) = parse_braced(input, cursor, next_cursor + 1)?;
                        append_expansion(&mut current, &value, true);
                        active = true;
                        cursor = after;
                    } else if let Some(name_start) = input[next_cursor..].chars().next() {
                        if is_name_start(name_start) {
                            let mut end = next_cursor + name_start.len_utf8();
                            while end < input.len() {
                                let ch = input[end..].chars().next().unwrap();
                                if is_name_continue(ch) {
                                    end += ch.len_utf8();
                                } else {
                                    break;
                                }
                            }
                            let value = lookup(env, &input[next_cursor..end]).unwrap_or("");
                            append_expansion(&mut current, value, true);
                            active = true;
                            cursor = end;
                        } else {
                            append_literal(&mut current, '$', true);
                            active = true;
                            cursor = next_cursor;
                        }
                    } else {
                        append_literal(&mut current, '$', true);
                        active = true;
                        cursor = next_cursor;
                    }
                } else {
                    append_literal(&mut current, c, true);
                    active = true;
                    cursor = next_cursor;
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

fn encode_word(word: &str) -> String {
    let candidates = [
        bare_candidate(word).map(|s| (s, 0usize)),
        single_candidate(word).map(|s| (s, 1usize)),
        double_candidate(word).map(|s| (s, 2usize)),
    ];

    if let Some((encoded, _)) = candidates
        .into_iter()
        .flatten()
        .min_by_key(|(encoded, priority)| (encoded.len(), *priority))
    {
        return encoded;
    }

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

fn main() {}