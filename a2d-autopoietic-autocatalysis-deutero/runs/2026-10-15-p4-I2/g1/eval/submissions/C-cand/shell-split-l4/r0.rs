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
enum Mode {
    Bare,
    Single(usize),
    Double(usize),
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_name_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn push_scalar(word: &mut Word, c: char, active_glob: bool) {
    if active_glob && matches!(c, '*' | '?' | '[') {
        word.glob.push(word.text.len());
    }
    word.text.push(c);
}

fn push_string(word: &mut Word, value: &str, active_glob: bool) {
    for c in value.chars() {
        push_scalar(word, c, active_glob);
    }
}

fn find_env<'a>(name: &str, env: &'a [(&str, &str)]) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| if *key == name { Some(*value) } else { None })
}

fn parse_braced(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
) -> Result<(usize, String), SplitError> {
    let bytes = input.as_bytes();
    let mut close = dollar + 2;
    while close < bytes.len() {
        if bytes[close] == b'\\' {
            close += 1;
            if close < bytes.len() {
                let c = input[close..].chars().next().unwrap();
                close += c.len_utf8();
            }
        } else if bytes[close] == b'}' {
            break;
        } else {
            let c = input[close..].chars().next().unwrap();
            close += c.len_utf8();
        }
    }

    if close >= bytes.len() {
        return Err(SplitError::UnterminatedExpansion { pos: dollar });
    }

    let content = &input[dollar + 2..close];
    let chars: Vec<(usize, char)> = content.char_indices().collect();
    if chars.is_empty() || !is_name_start(chars[0].1) {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }

    let mut name_end = chars[0].0 + chars[0].1.len_utf8();
    let mut i = 1;
    while i < chars.len() && is_name_continue(chars[i].1) {
        name_end = chars[i].0 + chars[i].1.len_utf8();
        i += 1;
    }

    let name = &content[..name_end];
    let rest = &content[name_end..];
    let selected = if rest.is_empty() {
        find_env(name, env).unwrap_or("").to_string()
    } else if let Some(default) = rest.strip_prefix(":-") {
        match find_env(name, env) {
            Some(value) if !value.is_empty() => value.to_string(),
            _ => decode_default(default),
        }
    } else {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    };

    Ok((close + 1, selected))
}

fn decode_default(text: &str) -> String {
    let mut result = String::new();
    let mut i = 0;
    while i < text.len() {
        let c = text[i..].chars().next().unwrap();
        if c == '\\' {
            i += 1;
            if i < text.len() {
                let next = text[i..].chars().next().unwrap();
                result.push(next);
                i += next.len_utf8();
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
    let mut mode = Mode::Bare;
    let mut i = 0;

    while i < input.len() {
        let c = input[i..].chars().next().unwrap();
        let next_i = i + c.len_utf8();

        match mode {
            Mode::Single(open) => {
                if c == '\'' {
                    mode = Mode::Bare;
                } else {
                    push_scalar(&mut word, c, false);
                }
                i = next_i;
                let _ = open;
            }
            Mode::Double(open) => {
                if c == '"' {
                    mode = Mode::Bare;
                    i = next_i;
                } else if c == '\\' {
                    if next_i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let n = input[next_i..].chars().next().unwrap();
                    let after = next_i + n.len_utf8();
                    if matches!(n, '"' | '\\' | '$' | '\n') {
                        if n != '\n' {
                            push_scalar(&mut word, n, false);
                        }
                        i = after;
                    } else {
                        push_scalar(&mut word, '\\', false);
                        i = next_i;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if input[next_i..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }
                    if input[next_i..].starts_with('{') {
                        let (end, value) = parse_braced(input, i, env)?;
                        push_string(&mut word, &value, false);
                        i = end;
                    } else if next_i < input.len()
                        && is_name_start(input[next_i..].chars().next().unwrap())
                    {
                        let mut end = next_i;
                        while end < input.len() {
                            let ch = input[end..].chars().next().unwrap();
                            if !is_name_continue(ch) {
                                break;
                            }
                            end += ch.len_utf8();
                        }
                        let value = find_env(&input[next_i..end], env).unwrap_or("");
                        push_string(&mut word, value, false);
                        i = end;
                    } else {
                        push_scalar(&mut word, '$', false);
                        i = next_i;
                    }
                } else {
                    push_scalar(&mut word, c, false);
                    i = next_i;
                }
                let _ = open;
            }
            Mode::Bare => {
                if matches!(c, ' ' | '\t' | '\r' | '\n') {
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
                    mode = Mode::Single(i);
                    i = next_i;
                } else if c == '"' {
                    active = true;
                    mode = Mode::Double(i);
                    i = next_i;
                } else if c == '\\' {
                    if next_i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let n = input[next_i..].chars().next().unwrap();
                    let after = next_i + n.len_utf8();
                    if n != '\n' {
                        active = true;
                        push_scalar(&mut word, n, false);
                    }
                    i = after;
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if input[next_i..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }
                    if input[next_i..].starts_with('{') {
                        let (end, value) = parse_braced(input, i, env)?;
                        active = true;
                        push_string(&mut word, &value, true);
                        i = end;
                    } else if next_i < input.len()
                        && is_name_start(input[next_i..].chars().next().unwrap())
                    {
                        let mut end = next_i;
                        while end < input.len() {
                            let ch = input[end..].chars().next().unwrap();
                            if !is_name_continue(ch) {
                                break;
                            }
                            end += ch.len_utf8();
                        }
                        let value = find_env(&input[next_i..end], env).unwrap_or("");
                        active = true;
                        push_string(&mut word, value, true);
                        i = end;
                    } else {
                        active = true;
                        push_scalar(&mut word, '$', true);
                        i = next_i;
                    }
                } else {
                    active = true;
                    push_scalar(&mut word, c, true);
                    i = next_i;
                }
            }
        }
    }

    match mode {
        Mode::Single(pos) => return Err(SplitError::UnterminatedSingle { pos }),
        Mode::Double(pos) => return Err(SplitError::UnterminatedDouble { pos }),
        Mode::Bare => {}
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
        if c.is_ascii_alphabetic()
            || c.is_ascii_digit()
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
    let mut result = String::with_capacity(word.len() + 2);
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
        let mut candidates: Vec<(usize, usize, String)> = Vec::new();
        if let Some(candidate) = bare_candidate(word) {
            candidates.push((candidate.len(), 0, candidate));
        }
        if let Some(candidate) = single_candidate(word) {
            candidates.push((candidate.len(), 1, candidate));
        }
        if let Some(candidate) = double_candidate(word) {
            candidates.push((candidate.len(), 2, candidate));
        }

        if candidates.is_empty() {
            let mut fallback = String::new();
            fallback.push('\'');
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
            encoded.push(fallback);
        } else {
            candidates.sort_by_key(|(len, priority, _)| (*len, *priority));
            encoded.push(candidates.remove(0).2);
        }
    }

    Ok(encoded.join(" "))
}

fn main() {}