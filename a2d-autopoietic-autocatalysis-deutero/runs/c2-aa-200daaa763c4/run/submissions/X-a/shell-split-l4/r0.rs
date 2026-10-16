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

fn lookup<'a>(name: &str, env: &'a [(&str, &str)]) -> Option<&'a str> {
    env.iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| *value)
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

fn decode_default(text: &str) -> String {
    let mut result = String::new();
    let mut chars = text.chars();

    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                result.push(next);
            }
        } else {
            result.push(c);
        }
    }

    result
}

fn find_braced_end(input: &str, mut i: usize) -> Option<usize> {
    while i < input.len() {
        let c = input[i..].chars().next()?;
        let next = i + c.len_utf8();

        if c == '\\' {
            if next >= input.len() {
                return None;
            }
            let escaped = input[next..].chars().next()?;
            i = next + escaped.len_utf8();
        } else if c == '}' {
            return Some(i);
        } else {
            i = next;
        }
    }

    None
}

fn parse_braced<'input>(
    input: &'input str,
    dollar: usize,
) -> Result<(usize, &'input str, Option<&'input str>), SplitError> {
    let content_start = dollar + 2;
    let end = find_braced_end(input, content_start)
        .ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
    let contents = &input[content_start..end];

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
    if remainder.is_empty() {
        Ok((end + 1, name, None))
    } else if let Some(default) = remainder.strip_prefix(":-") {
        Ok((end + 1, name, Some(default)))
    } else {
        Err(SplitError::InvalidExpansion { pos: dollar })
    }
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
    let mut i = 0;

    while i < input.len() {
        let c = input[i..].chars().next().unwrap();
        let next_i = i + c.len_utf8();

        match quote {
            Quote::Single(_) => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    append_literal(&mut word, c, false);
                }
                i = next_i;
            }
            Quote::Double(_) => {
                if c == '"' {
                    quote = Quote::None;
                    i = next_i;
                } else if c == '\\' {
                    if next_i >= input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let escaped = input[next_i..].chars().next().unwrap();
                    let after_escape = next_i + escaped.len_utf8();
                    if matches!(escaped, '"' | '\\' | '$') {
                        append_literal(&mut word, escaped, false);
                        i = after_escape;
                    } else if escaped == '\n' {
                        i = after_escape;
                    } else {
                        append_literal(&mut word, '\\', false);
                        i = next_i;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if input.get(next_i..).is_some_and(|rest| rest.starts_with('(')) {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }

                    if next_i < input.len() {
                        let following = input[next_i..].chars().next().unwrap();
                        if following == '{' {
                            let (end, name, default) = parse_braced(input, i)?;
                            let value = match (default, lookup(name, env)) {
                                (Some(_), Some(value)) if !value.is_empty() => value.to_string(),
                                (Some(default), _) => decode_default(default),
                                (None, Some(value)) => value.to_string(),
                                (None, None) => String::new(),
                            };
                            active = true;
                            append_expansion(&mut word, &value, false);
                            i = end;
                        } else if is_name_start(following) {
                            let mut end = next_i;
                            while end < input.len() {
                                let ch = input[end..].chars().next().unwrap();
                                if is_name_continue(ch) {
                                    end += ch.len_utf8();
                                } else {
                                    break;
                                }
                            }
                            let value = lookup(&input[next_i..end], env).unwrap_or("");
                            active = true;
                            append_expansion(&mut word, value, false);
                            i = end;
                        } else {
                            append_literal(&mut word, '$', false);
                            i = next_i;
                        }
                    } else {
                        append_literal(&mut word, '$', false);
                        i = next_i;
                    }
                } else {
                    append_literal(&mut word, c, false);
                    i = next_i;
                }
            }
            Quote::None => {
                if is_separator(c) {
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
                    quote = Quote::Single(i);
                    active = true;
                    i = next_i;
                } else if c == '"' {
                    quote = Quote::Double(i);
                    active = true;
                    i = next_i;
                } else if c == '\\' {
                    if next_i >= input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let escaped = input[next_i..].chars().next().unwrap();
                    let after_escape = next_i + escaped.len_utf8();
                    if escaped != '\n' {
                        active = true;
                        append_literal(&mut word, escaped, false);
                    }
                    i = after_escape;
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if input.get(next_i..).is_some_and(|rest| rest.starts_with('(')) {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }

                    if next_i < input.len() {
                        let following = input[next_i..].chars().next().unwrap();
                        if following == '{' {
                            let (end, name, default) = parse_braced(input, i)?;
                            let value = match (default, lookup(name, env)) {
                                (Some(_), Some(value)) if !value.is_empty() => value.to_string(),
                                (Some(default), _) => decode_default(default),
                                (None, Some(value)) => value.to_string(),
                                (None, None) => String::new(),
                            };
                            active = true;
                            append_expansion(&mut word, &value, true);
                            i = end;
                        } else if is_name_start(following) {
                            let mut end = next_i;
                            while end < input.len() {
                                let ch = input[end..].chars().next().unwrap();
                                if is_name_continue(ch) {
                                    end += ch.len_utf8();
                                } else {
                                    break;
                                }
                            }
                            let value = lookup(&input[next_i..end], env).unwrap_or("");
                            active = true;
                            append_expansion(&mut word, value, true);
                            i = end;
                        } else {
                            active = true;
                            append_literal(&mut word, '$', true);
                            i = next_i;
                        }
                    } else {
                        active = true;
                        append_literal(&mut word, '$', true);
                        i = next_i;
                    }
                } else {
                    active = true;
                    append_literal(&mut word, c, true);
                    i = next_i;
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

    let mut encoded_words = Vec::with_capacity(words.len());

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

        if candidates.is_empty() {
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
            encoded_words.push(fallback);
        } else {
            candidates.sort_by(|a, b| {
                a.0.len()
                    .cmp(&b.0.len())
                    .then_with(|| a.1.cmp(&b.1))
            });
            encoded_words.push(candidates.remove(0).0);
        }
    }

    Ok(encoded_words.join(" "))
}