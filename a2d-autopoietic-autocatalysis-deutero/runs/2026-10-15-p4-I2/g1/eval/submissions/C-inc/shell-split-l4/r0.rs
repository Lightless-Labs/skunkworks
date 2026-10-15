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

fn next_char(input: &str, pos: usize) -> Option<(char, usize)> {
    input[pos..]
        .chars()
        .next()
        .map(|c| (c, pos + c.len_utf8()))
}

fn append_literal(word: &mut Word, c: char, active_glob: bool) {
    let offset = word.text.len();
    word.text.push(c);
    if active_glob && matches!(c, '*' | '?' | '[') {
        word.glob.push(offset);
    }
}

fn append_expansion(word: &mut Word, value: &str, active_glob: bool) {
    let base = word.text.len();
    word.text.push_str(value);
    if active_glob {
        for (offset, c) in value.char_indices() {
            if matches!(c, '*' | '?' | '[') {
                word.glob.push(base + offset);
            }
        }
    }
}

fn lookup<'a>(env: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| if *key == name { Some(*value) } else { None })
}

fn decode_default(text: &str) -> String {
    let mut out = String::new();
    let mut pos = 0;
    while pos < text.len() {
        let (c, next) = next_char(text, pos).unwrap();
        if c == '\\' {
            if let Some((escaped, end)) = next_char(text, next) {
                out.push(escaped);
                pos = end;
            } else {
                out.push('\\');
                pos = next;
            }
        } else {
            out.push(c);
            pos = next;
        }
    }
    out
}

fn parse_braced(input: &str, dollar: usize, env: &[(&str, &str)]) -> Result<(String, usize), SplitError> {
    let mut cursor = dollar + 2;
    let mut closing = None;

    while cursor < input.len() {
        let (c, next) = next_char(input, cursor).unwrap();
        if c == '\\' {
            if let Some((_, end)) = next_char(input, next) {
                cursor = end;
            } else {
                cursor = next;
            }
        } else if c == '}' {
            closing = Some(cursor);
            break;
        } else {
            cursor = next;
        }
    }

    let close = closing.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
    let contents = &input[dollar + 2..close];
    let bytes = contents.as_bytes();
    let mut name_end = 0;

    if bytes.is_empty() {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }

    let first = contents.chars().next().unwrap();
    if !is_name_start(first) {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }
    name_end += first.len_utf8();

    while name_end < contents.len() {
        let c = contents[name_end..].chars().next().unwrap();
        if !is_name_continue(c) {
            break;
        }
        name_end += c.len_utf8();
    }

    let name = &contents[..name_end];
    let remainder = &contents[name_end..];
    let value = if remainder.is_empty() {
        lookup(env, name).unwrap_or("").to_string()
    } else if let Some(default) = remainder.strip_prefix(":-") {
        match lookup(env, name) {
            Some(v) if !v.is_empty() => v.to_string(),
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
    let mut quote_start = 0;
    let mut pos = 0;

    while pos < input.len() {
        let (c, next) = next_char(input, pos).unwrap();

        match quote {
            Quote::Single => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    append_literal(&mut current, c, false);
                }
                pos = next;
            }
            Quote::Double => {
                if c == '"' {
                    quote = Quote::None;
                    pos = next;
                } else if c == '\\' {
                    let Some((escaped, end)) = next_char(input, next) else {
                        return Err(SplitError::TrailingEscape { pos });
                    };
                    if matches!(escaped, '"' | '\\' | '$' | '\n') {
                        if escaped != '\n' {
                            append_literal(&mut current, escaped, false);
                        }
                        pos = end;
                    } else {
                        append_literal(&mut current, '\\', false);
                        pos = next;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos });
                } else if c == '$' {
                    if input[next..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos });
                    }
                    if input[next..].starts_with('{') {
                        let (value, end) = parse_braced(input, pos, env)?;
                        append_expansion(&mut current, &value, false);
                        active = true;
                        pos = end;
                    } else if let Some((first, _)) = next_char(input, next) {
                        if is_name_start(first) {
                            let mut end = next;
                            while let Some((ch, after)) = next_char(input, end) {
                                if is_name_continue(ch) {
                                    end = after;
                                } else {
                                    break;
                                }
                            }
                            let value = lookup(env, &input[next..end]).unwrap_or("");
                            append_expansion(&mut current, value, false);
                            active = true;
                            pos = end;
                        } else {
                            append_literal(&mut current, '$', false);
                            active = true;
                            pos = next;
                        }
                    } else {
                        append_literal(&mut current, '$', false);
                        active = true;
                        pos = next;
                    }
                } else {
                    append_literal(&mut current, c, false);
                    pos = next;
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
                    pos = next;
                } else if c == '\'' {
                    quote = Quote::Single;
                    quote_start = pos;
                    active = true;
                    pos = next;
                } else if c == '"' {
                    quote = Quote::Double;
                    quote_start = pos;
                    active = true;
                    pos = next;
                } else if c == '\\' {
                    let Some((escaped, end)) = next_char(input, next) else {
                        return Err(SplitError::TrailingEscape { pos });
                    };
                    if escaped != '\n' {
                        append_literal(&mut current, escaped, false);
                        active = true;
                    }
                    pos = end;
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos });
                } else if c == '$' {
                    if input[next..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos });
                    }
                    if input[next..].starts_with('{') {
                        let (value, end) = parse_braced(input, pos, env)?;
                        append_expansion(&mut current, &value, true);
                        active = true;
                        pos = end;
                    } else if let Some((first, _)) = next_char(input, next) {
                        if is_name_start(first) {
                            let mut end = next;
                            while let Some((ch, after)) = next_char(input, end) {
                                if is_name_continue(ch) {
                                    end = after;
                                } else {
                                    break;
                                }
                            }
                            let value = lookup(env, &input[next..end]).unwrap_or("");
                            append_expansion(&mut current, value, true);
                            active = true;
                            pos = end;
                        } else {
                            append_literal(&mut current, '$', false);
                            active = true;
                            pos = next;
                        }
                    } else {
                        append_literal(&mut current, '$', false);
                        active = true;
                        pos = next;
                    }
                } else {
                    append_literal(&mut current, c, true);
                    active = true;
                    pos = next;
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
        Some(format!("'{}'", word))
    }
}

fn double_candidate(word: &str) -> Option<String> {
    if word.contains('`') {
        return None;
    }
    let mut out = String::new();
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

fn encode_word(word: &str) -> String {
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

    if candidates.is_empty() {
        let mut fallback = String::new();
        fallback.push('\'');
        for c in word.chars() {
            if c == '\'' {
                fallback.push_str("'\\''");
            } else {
                fallback.push(c);
            }
        }
        fallback.push('\'');
        return fallback;
    }

    candidates.sort_by_key(|(candidate, priority)| (candidate.len(), *priority));
    candidates.remove(0).0
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