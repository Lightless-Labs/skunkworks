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
    Bare,
    Single,
    Double,
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

fn append(text: &mut String, glob: &mut Vec<usize>, c: char, active: bool) {
    if active && matches!(c, '*' | '?' | '[') {
        glob.push(text.len());
    }
    text.push(c);
}

fn append_str(text: &mut String, glob: &mut Vec<usize>, value: &str, active: bool) {
    for c in value.chars() {
        append(text, glob, c, active);
    }
}

fn env_value<'a>(env: &'a [(&str, &str)], name: &str) -> &'a str {
    env.iter()
        .find_map(|(key, value)| (*key == name).then_some(*value))
        .unwrap_or("")
}

fn char_at(input: &str, pos: usize) -> Option<char> {
    input.get(pos..)?.chars().next()
}

fn next_pos(input: &str, pos: usize) -> usize {
    pos + char_at(input, pos).map(char::len_utf8).unwrap_or(0)
}

fn expand_braced(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
    text: &mut String,
    glob: &mut Vec<usize>,
    active: bool,
) -> Result<usize, SplitError> {
    let mut close = dollar + 2;
    let mut found = None;

    while close < input.len() {
        let c = char_at(input, close).unwrap();
        if c == '\\' {
            let after_slash = next_pos(input, close);
            if after_slash < input.len() {
                close = next_pos(input, after_slash);
            } else {
                close = after_slash;
            }
        } else if c == '}' {
            found = Some(close);
            break;
        } else {
            close = next_pos(input, close);
        }
    }

    let close = found.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
    let contents = &input[dollar + 2..close];
    let chars: Vec<(usize, char)> = contents
        .char_indices()
        .map(|(i, c)| (i, c))
        .collect();

    if chars.is_empty() || !is_name_start(chars[0].1) {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }

    let mut name_end_index = 1;
    while name_end_index < chars.len() && is_name_continue(chars[name_end_index].1) {
        name_end_index += 1;
    }

    let name_end = if name_end_index < chars.len() {
        chars[name_end_index].0
    } else {
        contents.len()
    };
    let name = &contents[..name_end];

    if name_end == contents.len() {
        append_str(text, glob, env_value(env, name), active);
        return Ok(next_pos(input, close));
    }

    if !contents[name_end..].starts_with(":-") {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }

    let default_start = name_end + 2;
    let selected = env_value(env, name);
    if selected.is_empty() {
        let default = &contents[default_start..];
        let mut decoded = String::new();
        let mut i = 0;
        while i < default.len() {
            let c = char_at(default, i).unwrap();
            if c == '\\' {
                let after_slash = next_pos(default, i);
                if after_slash < default.len() {
                    let escaped = char_at(default, after_slash).unwrap();
                    decoded.push(escaped);
                    i = next_pos(default, after_slash);
                } else {
                    decoded.push('\\');
                    i = after_slash;
                }
            } else {
                decoded.push(c);
                i = next_pos(default, i);
            }
        }
        append_str(text, glob, &decoded, active);
    } else {
        append_str(text, glob, selected, active);
    }

    Ok(next_pos(input, close))
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<Word>, SplitError> {
    if let Some(pos) = input.find('\0') {
        return Err(SplitError::Nul { pos });
    }

    let mut words = Vec::new();
    let mut text = String::new();
    let mut glob = Vec::new();
    let mut active_word = false;
    let mut quote = Quote::Bare;
    let mut quote_start = 0;
    let mut pos = 0;

    while pos < input.len() {
        let c = char_at(input, pos).unwrap();

        match quote {
            Quote::Bare => {
                if is_separator(c) {
                    if active_word {
                        words.push(Word { text, glob });
                        text = String::new();
                        glob = Vec::new();
                        active_word = false;
                    }
                    pos = next_pos(input, pos);
                } else if c == '\'' {
                    active_word = true;
                    quote = Quote::Single;
                    quote_start = pos;
                    pos = next_pos(input, pos);
                } else if c == '"' {
                    active_word = true;
                    quote = Quote::Double;
                    quote_start = pos;
                    pos = next_pos(input, pos);
                } else if c == '\\' {
                    let slash = pos;
                    let next = next_pos(input, pos);
                    if next == input.len() {
                        return Err(SplitError::TrailingEscape { pos: slash });
                    }
                    let escaped = char_at(input, next).unwrap();
                    if escaped == '\n' {
                        pos = next_pos(input, next);
                    } else {
                        active_word = true;
                        append(&mut text, &mut glob, escaped, false);
                        pos = next_pos(input, next);
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos });
                } else if c == '$' {
                    let next = next_pos(input, pos);
                    if char_at(input, next) == Some('(') {
                        return Err(SplitError::CommandSubstitution { pos });
                    }
                    if char_at(input, next) == Some('{') {
                        active_word = true;
                        pos = expand_braced(input, pos, env, &mut text, &mut glob, true)?;
                    } else if char_at(input, next).is_some_and(is_name_start) {
                        active_word = true;
                        let mut end = next;
                        while let Some(ch) = char_at(input, end) {
                            if !is_name_continue(ch) {
                                break;
                            }
                            end = next_pos(input, end);
                        }
                        append_str(&mut text, &mut glob, env_value(env, &input[next..end]), true);
                        pos = end;
                    } else {
                        active_word = true;
                        append(&mut text, &mut glob, '$', true);
                        pos = next;
                    }
                } else {
                    active_word = true;
                    append(&mut text, &mut glob, c, true);
                    pos = next_pos(input, pos);
                }
            }
            Quote::Single => {
                if c == '\'' {
                    quote = Quote::Bare;
                    pos = next_pos(input, pos);
                } else {
                    append(&mut text, &mut glob, c, false);
                    pos = next_pos(input, pos);
                }
            }
            Quote::Double => {
                if c == '"' {
                    quote = Quote::Bare;
                    pos = next_pos(input, pos);
                } else if c == '\\' {
                    let slash = pos;
                    let next = next_pos(input, pos);
                    if next == input.len() {
                        return Err(SplitError::TrailingEscape { pos: slash });
                    }
                    let escaped = char_at(input, next).unwrap();
                    if matches!(escaped, '"' | '\\' | '$') {
                        append(&mut text, &mut glob, escaped, false);
                        pos = next_pos(input, next);
                    } else if escaped == '\n' {
                        pos = next_pos(input, next);
                    } else {
                        append(&mut text, &mut glob, '\\', false);
                        pos = next;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos });
                } else if c == '$' {
                    let next = next_pos(input, pos);
                    if char_at(input, next) == Some('(') {
                        return Err(SplitError::CommandSubstitution { pos });
                    }
                    if char_at(input, next) == Some('{') {
                        active_word = true;
                        pos = expand_braced(input, pos, env, &mut text, &mut glob, false)?;
                    } else if char_at(input, next).is_some_and(is_name_start) {
                        active_word = true;
                        let mut end = next;
                        while let Some(ch) = char_at(input, end) {
                            if !is_name_continue(ch) {
                                break;
                            }
                            end = next_pos(input, end);
                        }
                        append_str(&mut text, &mut glob, env_value(env, &input[next..end]), false);
                        pos = end;
                    } else {
                        append(&mut text, &mut glob, '$', false);
                        pos = next;
                    }
                } else {
                    append(&mut text, &mut glob, c, false);
                    pos = next_pos(input, pos);
                }
            }
        }
    }

    match quote {
        Quote::Single => return Err(SplitError::UnterminatedSingle { pos: quote_start }),
        Quote::Double => return Err(SplitError::UnterminatedDouble { pos: quote_start }),
        Quote::Bare => {}
    }

    if active_word {
        words.push(Word { text, glob });
    }

    Ok(words)
}

fn bare_candidate(word: &str) -> Option<String> {
    if word.is_empty() || word.contains('\n') {
        return None;
    }
    let mut result = String::new();
    for c in word.chars() {
        if c.is_ascii_alphanumeric()
            || matches!(c, '_' | '-' | '.' | '/' | ':' | '=' | ',' | '+' | '@' | '%')
        {
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
            bare_candidate(word).map(|s| (0usize, s)),
            single_candidate(word).map(|s| (1usize, s)),
            double_candidate(word).map(|s| (2usize, s)),
        ];

        let best = candidates
            .into_iter()
            .flatten()
            .min_by_key(|(priority, candidate)| (candidate.len(), *priority));

        let encoded_word = match best {
            Some((_, candidate)) => candidate,
            None => {
                let mut fallback = String::from("'");
                for c in word.chars() {
                    if c == '\'' {
                        fallback.push('\'');
                        fallback.push('\\');
                        fallback.push('\'');
                        fallback.push('\'');
                    } else {
                        fallback.push(c);
                    }
                }
                fallback.push('\'');
                fallback
            }
        };

        encoded.push(encoded_word);
    }

    Ok(encoded.join(" "))
}