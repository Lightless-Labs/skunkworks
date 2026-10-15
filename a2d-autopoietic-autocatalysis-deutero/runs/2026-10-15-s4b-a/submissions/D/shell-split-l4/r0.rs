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

fn append(text: &mut String, glob: &mut Vec<usize>, value: &str, active: bool) {
    for c in value.chars() {
        if active && matches!(c, '*' | '?' | '[') {
            glob.push(text.len());
        }
        text.push(c);
    }
}

fn find_brace_end(input: &str, mut i: usize) -> Option<usize> {
    while i < input.len() {
        let c = input[i..].chars().next().unwrap();
        if c == '\\' {
            i += c.len_utf8();
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
    let mut out = String::new();
    let mut i = 0;
    while i < text.len() {
        let c = text[i..].chars().next().unwrap();
        if c == '\\' {
            i += 1;
            if i < text.len() {
                let next = text[i..].chars().next().unwrap();
                out.push(next);
                i += next.len_utf8();
            } else {
                out.push('\\');
            }
        } else {
            out.push(c);
            i += c.len_utf8();
        }
    }
    out
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<Word>, SplitError> {
    if let Some(pos) = input.as_bytes().iter().position(|&b| b == 0) {
        return Err(SplitError::Nul { pos });
    }

    let mut words = Vec::new();
    let mut text = String::new();
    let mut glob = Vec::new();
    let mut active_word = false;
    let mut quote = Quote::None;
    let mut quote_pos = 0;
    let mut i = 0;

    while i < input.len() {
        let c = input[i..].chars().next().unwrap();
        let next_i = i + c.len_utf8();

        match quote {
            Quote::Single => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    text.push(c);
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
                    let n = input[next_i..].chars().next().unwrap();
                    if matches!(n, '"' | '\\' | '$' | '\n') {
                        if n != '\n' {
                            append(&mut text, &mut glob, &n.to_string(), false);
                        }
                        i = next_i + n.len_utf8();
                    } else {
                        text.push('\\');
                        i = next_i;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if input[next_i..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }
                    if input[next_i..].starts_with('{') {
                        let end = match find_brace_end(input, next_i + 1) {
                            Some(end) => end,
                            None => return Err(SplitError::UnterminatedExpansion { pos: i }),
                        };
                        let body = &input[next_i + 1..end];
                        let (name, default) = if let Some(k) = body.find(":-") {
                            (&body[..k], Some(&body[k + 2..]))
                        } else {
                            (body, None)
                        };
                        if name.is_empty()
                            || !name.chars().next().map(is_name_start).unwrap_or(false)
                            || !name.chars().all(is_name_continue)
                        {
                            return Err(SplitError::InvalidExpansion { pos: i });
                        }
                        let value = env.iter().find(|(k, _)| *k == name).map(|(_, v)| *v);
                        let selected = match (default, value) {
                            (Some(d), Some(v)) if !v.is_empty() => v.to_string(),
                            (Some(d), _) => decode_default(d),
                            (None, Some(v)) => v.to_string(),
                            (None, None) => String::new(),
                        };
                        append(&mut text, &mut glob, &selected, false);
                        active_word = true;
                        i = end + 1;
                    } else if next_i < input.len()
                        && input[next_i..].chars().next().map(is_name_start).unwrap_or(false)
                    {
                        let mut end = next_i;
                        while end < input.len() {
                            let ch = input[end..].chars().next().unwrap();
                            if !is_name_continue(ch) {
                                break;
                            }
                            end += ch.len_utf8();
                        }
                        let name = &input[next_i..end];
                        if let Some((_, value)) = env.iter().find(|(k, _)| *k == name) {
                            append(&mut text, &mut glob, value, false);
                        }
                        active_word = true;
                        i = end;
                    } else {
                        text.push('$');
                        i = next_i;
                    }
                } else {
                    text.push(c);
                    i = next_i;
                }
            }
            Quote::None => {
                if matches!(c, ' ' | '\t' | '\r' | '\n') {
                    if active_word {
                        words.push(Word {
                            text: std::mem::take(&mut text),
                            glob: std::mem::take(&mut glob),
                        });
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
                    let n = input[next_i..].chars().next().unwrap();
                    if n == '\n' {
                        i = next_i + n.len_utf8();
                    } else {
                        append(&mut text, &mut glob, &n.to_string(), false);
                        active_word = true;
                        i = next_i + n.len_utf8();
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if input[next_i..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }
                    if input[next_i..].starts_with('{') {
                        let end = match find_brace_end(input, next_i + 1) {
                            Some(end) => end,
                            None => return Err(SplitError::UnterminatedExpansion { pos: i }),
                        };
                        let body = &input[next_i + 1..end];
                        let (name, default) = if let Some(k) = body.find(":-") {
                            (&body[..k], Some(&body[k + 2..]))
                        } else {
                            (body, None)
                        };
                        if name.is_empty()
                            || !name.chars().next().map(is_name_start).unwrap_or(false)
                            || !name.chars().all(is_name_continue)
                        {
                            return Err(SplitError::InvalidExpansion { pos: i });
                        }
                        let value = env.iter().find(|(k, _)| *k == name).map(|(_, v)| *v);
                        let selected = match (default, value) {
                            (Some(_), Some(v)) if !v.is_empty() => v.to_string(),
                            (Some(d), _) => decode_default(d),
                            (None, Some(v)) => v.to_string(),
                            (None, None) => String::new(),
                        };
                        append(&mut text, &mut glob, &selected, true);
                        active_word = true;
                        i = end + 1;
                    } else if next_i < input.len()
                        && input[next_i..].chars().next().map(is_name_start).unwrap_or(false)
                    {
                        let mut end = next_i;
                        while end < input.len() {
                            let ch = input[end..].chars().next().unwrap();
                            if !is_name_continue(ch) {
                                break;
                            }
                            end += ch.len_utf8();
                        }
                        let name = &input[next_i..end];
                        if let Some((_, value)) = env.iter().find(|(k, _)| *k == name) {
                            append(&mut text, &mut glob, value, true);
                        }
                        active_word = true;
                        i = end;
                    } else {
                        text.push('$');
                        active_word = true;
                        i = next_i;
                    }
                } else {
                    append(&mut text, &mut glob, &c.to_string(), true);
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
        words.push(Word { text, glob });
    }
    Ok(words)
}

fn bare_candidate(word: &str) -> Option<String> {
    if word.is_empty() || word.contains('\n') {
        return None;
    }
    let mut out = String::new();
    for c in word.chars() {
        if c.is_ascii_alphanumeric() || "_-./:=,+@%".contains(c) {
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
    Some(format!("'{word}'"))
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
        if let Some(pos) = word.as_bytes().iter().position(|&b| b == 0) {
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
        let best = candidates
            .iter()
            .enumerate()
            .filter_map(|(priority, candidate)| {
                candidate
                    .as_ref()
                    .map(|s| (priority, s))
            })
            .min_by_key(|(priority, s)| (s.len(), *priority));

        let chosen = if let Some((_, value)) = best {
            value.clone()
        } else {
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
        };
        encoded.push(chosen);
    }
    Ok(encoded.join(" "))
}