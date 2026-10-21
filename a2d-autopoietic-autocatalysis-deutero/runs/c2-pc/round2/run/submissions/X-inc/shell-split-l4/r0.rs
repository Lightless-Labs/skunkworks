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

fn is_name_char(c: char) -> bool {
    is_name_start(c) || c.is_ascii_digit()
}

fn lookup<'a>(name: &str, env: &'a [(&str, &str)]) -> &'a str {
    env.iter()
        .find_map(|(key, value)| if *key == name { Some(*value) } else { None })
        .unwrap_or("")
}

fn append_literal(word: &mut Word, c: char, active: bool) {
    if active && matches!(c, '*' | '?' | '[') {
        word.glob.push(word.text.len());
    }
    word.text.push(c);
}

fn append_expansion(word: &mut Word, text: &str, active: bool) {
    for c in text.chars() {
        append_literal(word, c, active);
    }
}

fn find_braced_end(input: &str, start: usize) -> Option<usize> {
    let mut i = start;
    while i < input.len() {
        let c = input[i..].chars().next().unwrap();
        if c == '\\' {
            i += 1;
            if i < input.len() {
                i += input[i..].chars().next().unwrap().len_utf8();
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
        if c == '\\' {
            i += 1;
            if i < text.len() {
                let escaped = text[i..].chars().next().unwrap();
                result.push(escaped);
                i += escaped.len_utf8();
            } else {
                result.push('\\');
            }
        } else {
            result.push(c);
            i += c.len_utf8();
        }
    }
    result
}

fn expand_braced(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
) -> Result<(usize, String), SplitError> {
    let content_start = dollar + 2;
    let end = find_braced_end(input, content_start)
        .ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
    let content = &input[content_start..end];

    let name_end = content
        .char_indices()
        .find_map(|(i, c)| if !is_name_char(c) { Some(i) } else { None })
        .unwrap_or(content.len());

    let name = &content[..name_end];
    if name.is_empty() || !name.chars().next().map(is_name_start).unwrap_or(false) {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }

    let value = if name_end == content.len() {
        lookup(name, env).to_string()
    } else {
        let rest = &content[name_end..];
        if let Some(default) = rest.strip_prefix(":-") {
            let found = lookup(name, env);
            if found.is_empty() {
                decode_default(default)
            } else {
                found.to_string()
            }
        } else {
            return Err(SplitError::InvalidExpansion { pos: dollar });
        }
    };

    Ok((end + 1, value))
}

fn expand_unbraced(input: &str, dollar: usize, env: &[(&str, &str)]) -> Option<(usize, String)> {
    let start = dollar + 1;
    let first = input.get(start..)?.chars().next()?;
    if !is_name_start(first) {
        return None;
    }

    let mut end = start + first.len_utf8();
    while end < input.len() {
        let c = input[end..].chars().next().unwrap();
        if !is_name_char(c) {
            break;
        }
        end += c.len_utf8();
    }

    Some((end, lookup(&input[start..end], env).to_string()))
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
    let mut active_word = false;
    let mut quote = Quote::None;
    let mut quote_pos = 0usize;
    let mut i = 0usize;

    while i < input.len() {
        let c = input[i..].chars().next().unwrap();
        let next_i = i + c.len_utf8();

        match quote {
            Quote::Single => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    current.text.push(c);
                }
                i = next_i;
            }
            Quote::Double => {
                if c == '"' {
                    quote = Quote::None;
                    i = next_i;
                } else if c == '\\' {
                    if next_i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let escaped = input[next_i..].chars().next().unwrap();
                    let after_escaped = next_i + escaped.len_utf8();
                    if matches!(escaped, '"' | '\\' | '$') {
                        current.text.push(escaped);
                        i = after_escaped;
                    } else if escaped == '\n' {
                        i = after_escaped;
                    } else {
                        current.text.push('\\');
                        i = next_i;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if input[next_i..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }
                    if input[next_i..].starts_with('{') {
                        let (end, value) = expand_braced(input, i, env)?;
                        append_expansion(&mut current, &value, false);
                        active_word = true;
                        i = end;
                    } else if let Some((end, value)) = expand_unbraced(input, i, env) {
                        append_expansion(&mut current, &value, false);
                        active_word = true;
                        i = end;
                    } else {
                        current.text.push('$');
                        i = next_i;
                    }
                } else {
                    current.text.push(c);
                    i = next_i;
                }
            }
            Quote::None => {
                if matches!(c, ' ' | '\t' | '\r' | '\n') {
                    if active_word {
                        words.push(current);
                        current = Word {
                            text: String::new(),
                            glob: Vec::new(),
                        };
                        active_word = false;
                    }
                    i = next_i;
                } else if c == '\'' {
                    quote = Quote::Single;
                    quote_pos = i;
                    active_word = true;
                    i = next_i;
                } else if c == '"' {
                    quote = Quote::Double;
                    quote_pos = i;
                    active_word = true;
                    i = next_i;
                } else if c == '\\' {
                    if next_i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let escaped = input[next_i..].chars().next().unwrap();
                    let after_escaped = next_i + escaped.len_utf8();
                    if escaped == '\n' {
                        i = after_escaped;
                    } else {
                        append_literal(&mut current, escaped, false);
                        active_word = true;
                        i = after_escaped;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if input[next_i..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }
                    if input[next_i..].starts_with('{') {
                        let (end, value) = expand_braced(input, i, env)?;
                        append_expansion(&mut current, &value, true);
                        active_word = true;
                        i = end;
                    } else if let Some((end, value)) = expand_unbraced(input, i, env) {
                        append_expansion(&mut current, &value, true);
                        active_word = true;
                        i = end;
                    } else {
                        current.text.push('$');
                        active_word = true;
                        i = next_i;
                    }
                } else {
                    append_literal(&mut current, c, true);
                    active_word = true;
                    i = next_i;
                }
            }
        }
    }

    match quote {
        Quote::Single => return Err(SplitError::UnterminatedSingle { pos: quote_pos }),
        Quote::Double => return Err(SplitError::UnterminatedDouble { pos: quote_pos }),
        Quote::None => {}
    }

    if active_word {
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
    let mut result = String::new();
    result.push('\'');
    result.push_str(word);
    result.push('\'');
    Some(result)
}

fn double_candidate(word: &str) -> Option<String> {
    if word.contains('`') {
        return None;
    }
    let mut result = String::new();
    result.push('"');
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
    let mut result = String::new();
    result.push('\'');
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
            candidates.push((0u8, candidate));
        }
        if let Some(candidate) = single_candidate(word) {
            candidates.push((1u8, candidate));
        }
        if let Some(candidate) = double_candidate(word) {
            candidates.push((2u8, candidate));
        }

        let selected = if candidates.is_empty() {
            fallback_candidate(word)
        } else {
            candidates
                .into_iter()
                .min_by_key(|(priority, candidate)| (candidate.len(), *priority))
                .unwrap()
                .1
        };
        encoded.push(selected);
    }

    Ok(encoded.join(" "))
}