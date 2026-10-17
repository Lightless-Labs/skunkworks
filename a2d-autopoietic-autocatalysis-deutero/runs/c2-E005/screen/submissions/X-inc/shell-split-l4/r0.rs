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
    is_name_start(c) || c.is_ascii_digit()
}

fn lookup<'a>(env: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
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

fn find_braced_end(input: &str, mut index: usize) -> Option<usize> {
    while index < input.len() {
        let c = input[index..].chars().next()?;
        if c == '\\' {
            index += c.len_utf8();
            if index < input.len() {
                let next = input[index..].chars().next()?;
                index += next.len_utf8();
            }
        } else if c == '}' {
            return Some(index);
        } else {
            index += c.len_utf8();
        }
    }
    None
}

fn expand_braced(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
    word: &mut Word,
    active_glob: bool,
) -> Result<usize, SplitError> {
    let contents_start = dollar + 2;
    let close = find_braced_end(input, contents_start)
        .ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
    let contents = &input[contents_start..close];

    let name_end = contents
        .char_indices()
        .find_map(|(i, c)| (!is_name_continue(c)).then_some(i))
        .unwrap_or(contents.len());

    let name = &contents[..name_end];
    if name.is_empty()
        || !name.chars().next().is_some_and(is_name_start)
        || !name.chars().all(is_name_continue)
    {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }

    let default = if name_end == contents.len() {
        None
    } else if contents[name_end..].starts_with(":-") {
        Some(&contents[name_end + 2..])
    } else {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    };

    let value = match (lookup(env, name), default) {
        (Some(value), None) => value.to_string(),
        (Some(value), Some(_)) if !value.is_empty() => value.to_string(),
        (_, Some(text)) => decode_default(text),
        (None, None) => String::new(),
    };

    append_expansion(word, &value, active_glob);
    Ok(close + 1)
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
    let mut quote_open = 0;
    let mut index = 0;

    while index < input.len() {
        let c = input[index..].chars().next().unwrap();
        let next_index = index + c.len_utf8();

        match quote {
            Quote::Single => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    append_literal(&mut current, c, false);
                }
                index = next_index;
            }
            Quote::Double => {
                if c == '"' {
                    quote = Quote::None;
                    index = next_index;
                } else if c == '\\' {
                    let escape_pos = index;
                    if next_index >= input.len() {
                        return Err(SplitError::TrailingEscape { pos: escape_pos });
                    }
                    let next = input[next_index..].chars().next().unwrap();
                    let after_next = next_index + next.len_utf8();
                    if matches!(next, '"' | '\\' | '$' | '\n') {
                        if next == '$' {
                            append_literal(&mut current, '$', false);
                            index = after_next;
                        } else if next != '\n' {
                            append_literal(&mut current, next, false);
                            index = after_next;
                        } else {
                            index = after_next;
                        }
                    } else {
                        append_literal(&mut current, '\\', false);
                        index = next_index;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: index });
                } else if c == '$' {
                    if input[next_index..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: index });
                    }
                    if input[next_index..].starts_with('{') {
                        index = expand_braced(input, index, env, &mut current, false)?;
                    } else if next_index < input.len()
                        && input[next_index..]
                            .chars()
                            .next()
                            .is_some_and(is_name_start)
                    {
                        let mut end = next_index;
                        while end < input.len() {
                            let ch = input[end..].chars().next().unwrap();
                            if !is_name_continue(ch) {
                                break;
                            }
                            end += ch.len_utf8();
                        }
                        let name = &input[next_index..end];
                        let value = lookup(env, name).unwrap_or("");
                        append_expansion(&mut current, value, false);
                        index = end;
                    } else {
                        append_literal(&mut current, '$', false);
                        index = next_index;
                    }
                } else {
                    append_literal(&mut current, c, false);
                    index = next_index;
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
                    index = next_index;
                } else if c == '\'' {
                    quote = Quote::Single;
                    quote_open = index;
                    active = true;
                    index = next_index;
                } else if c == '"' {
                    quote = Quote::Double;
                    quote_open = index;
                    active = true;
                    index = next_index;
                } else if c == '\\' {
                    if next_index >= input.len() {
                        return Err(SplitError::TrailingEscape { pos: index });
                    }
                    let next = input[next_index..].chars().next().unwrap();
                    let after_next = next_index + next.len_utf8();
                    if next != '\n' {
                        active = true;
                        append_literal(&mut current, next, false);
                    }
                    index = after_next;
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: index });
                } else if c == '$' {
                    if input[next_index..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: index });
                    }
                    if input[next_index..].starts_with('{') {
                        active = true;
                        index = expand_braced(input, index, env, &mut current, true)?;
                    } else if next_index < input.len()
                        && input[next_index..]
                            .chars()
                            .next()
                            .is_some_and(is_name_start)
                    {
                        let mut end = next_index;
                        while end < input.len() {
                            let ch = input[end..].chars().next().unwrap();
                            if !is_name_continue(ch) {
                                break;
                            }
                            end += ch.len_utf8();
                        }
                        let name = &input[next_index..end];
                        let value = lookup(env, name).unwrap_or("");
                        append_expansion(&mut current, value, true);
                        active = true;
                        index = end;
                    } else {
                        append_literal(&mut current, '$', true);
                        active = true;
                        index = next_index;
                    }
                } else {
                    append_literal(&mut current, c, true);
                    active = true;
                    index = next_index;
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
        return None;
    }
    Some(format!("'{word}'"))
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

        let candidate = if candidates.is_empty() {
            fallback_candidate(word)
        } else {
            candidates
                .into_iter()
                .min_by_key(|(candidate, priority)| (candidate.len(), *priority))
                .unwrap()
                .0
        };
        encoded.push(candidate);
    }

    Ok(encoded.join(" "))
}