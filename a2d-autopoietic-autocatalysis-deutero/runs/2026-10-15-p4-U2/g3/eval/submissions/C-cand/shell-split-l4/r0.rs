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

fn append_char(word: &mut Word, c: char, active_glob: bool) {
    let offset = word.text.len();
    word.text.push(c);
    if active_glob && matches!(c, '*' | '?' | '[') {
        word.glob.push(offset);
    }
}

fn lookup<'a>(name: &str, env: &'a [(&str, &str)]) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| if *key == name { Some(*value) } else { None })
}

fn decode_default(text: &str) -> String {
    let mut decoded = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                decoded.push(next);
            }
        } else {
            decoded.push(c);
        }
    }
    decoded
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<Word>, SplitError> {
    if let Some(pos) = input.as_bytes().iter().position(|&b| b == 0) {
        return Err(SplitError::Nul { pos });
    }

    let mut words = Vec::new();
    let mut current = Word {
        text: String::new(),
        glob: Vec::new(),
    };
    let mut active = false;
    let mut quote = Quote::None;
    let mut quote_start = 0usize;
    let mut i = 0usize;

    while i < input.len() {
        let c = input[i..].chars().next().expect("valid UTF-8 cursor");
        let next_i = i + c.len_utf8();

        match quote {
            Quote::Single => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    append_char(&mut current, c, false);
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
                    let n = input[next_i..].chars().next().expect("valid UTF-8 cursor");
                    let after_n = next_i + n.len_utf8();
                    if matches!(n, '"' | '\\' | '$') {
                        append_char(&mut current, n, false);
                        i = after_n;
                    } else if n == '\n' {
                        i = after_n;
                    } else {
                        append_char(&mut current, '\\', false);
                        i = next_i;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if next_i < input.len() && input[next_i..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }
                    active = true;
                    if next_i < input.len() {
                        let n = input[next_i..].chars().next().expect("valid UTF-8 cursor");
                        if is_name_start(n) {
                            let mut end = next_i + n.len_utf8();
                            while end < input.len() {
                                let ch = input[end..].chars().next().expect("valid UTF-8 cursor");
                                if !is_name_continue(ch) {
                                    break;
                                }
                                end += ch.len_utf8();
                            }
                            let value = lookup(&input[next_i..end], env).unwrap_or("");
                            for ch in value.chars() {
                                append_char(&mut current, ch, false);
                            }
                            i = end;
                        } else if n == '{' {
                            let dollar = i;
                            let content_start = next_i + 1;
                            let mut scan = content_start;
                            let mut close = None;
                            while scan < input.len() {
                                let ch = input[scan..].chars().next().expect("valid UTF-8 cursor");
                                let after = scan + ch.len_utf8();
                                if ch == '\\' {
                                    if after < input.len() {
                                        let escaped = input[after..].chars().next().expect("valid UTF-8 cursor");
                                        scan = after + escaped.len_utf8();
                                    } else {
                                        scan = after;
                                    }
                                } else if ch == '}' {
                                    close = Some(scan);
                                    break;
                                } else {
                                    scan = after;
                                }
                            }
                            let Some(close_pos) = close else {
                                return Err(SplitError::UnterminatedExpansion { pos: dollar });
                            };
                            let content = &input[content_start..close_pos];
                            let name_end = content
                                .char_indices()
                                .find_map(|(idx, ch)| {
                                    if is_name_continue(ch) {
                                        None
                                    } else {
                                        Some(idx)
                                    }
                                })
                                .unwrap_or(content.len());
                            let name = &content[..name_end];
                            if name.is_empty()
                                || !name.chars().next().is_some_and(is_name_start)
                            {
                                return Err(SplitError::InvalidExpansion { pos: dollar });
                            }
                            let default = if name_end == content.len() {
                                None
                            } else if content[name_end..].starts_with(":-") {
                                Some(&content[name_end + 2..])
                            } else {
                                return Err(SplitError::InvalidExpansion { pos: dollar });
                            };
                            let selected = match (lookup(name, env), default) {
                                (Some(value), Some(text)) if value.is_empty() => {
                                    decode_default(text)
                                }
                                (Some(value), _) => value.to_string(),
                                (None, Some(text)) => decode_default(text),
                                (None, None) => String::new(),
                            };
                            for ch in selected.chars() {
                                append_char(&mut current, ch, false);
                            }
                            i = close_pos + 1;
                        } else {
                            append_char(&mut current, '$', false);
                            i = next_i;
                        }
                    } else {
                        append_char(&mut current, '$', false);
                        i = next_i;
                    }
                } else {
                    append_char(&mut current, c, false);
                    i = next_i;
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
                    i = next_i;
                } else if c == '\'' {
                    quote = Quote::Single;
                    quote_start = i;
                    active = true;
                    i = next_i;
                } else if c == '"' {
                    quote = Quote::Double;
                    quote_start = i;
                    active = true;
                    i = next_i;
                } else if c == '\\' {
                    if next_i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let n = input[next_i..].chars().next().expect("valid UTF-8 cursor");
                    let after_n = next_i + n.len_utf8();
                    if n == '\n' {
                        i = after_n;
                    } else {
                        active = true;
                        append_char(&mut current, n, false);
                        i = after_n;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if next_i < input.len() && input[next_i..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }
                    active = true;
                    if next_i < input.len() {
                        let n = input[next_i..].chars().next().expect("valid UTF-8 cursor");
                        if is_name_start(n) {
                            let mut end = next_i + n.len_utf8();
                            while end < input.len() {
                                let ch = input[end..].chars().next().expect("valid UTF-8 cursor");
                                if !is_name_continue(ch) {
                                    break;
                                }
                                end += ch.len_utf8();
                            }
                            let value = lookup(&input[next_i..end], env).unwrap_or("");
                            for ch in value.chars() {
                                append_char(&mut current, ch, true);
                            }
                            i = end;
                        } else if n == '{' {
                            let dollar = i;
                            let content_start = next_i + 1;
                            let mut scan = content_start;
                            let mut close = None;
                            while scan < input.len() {
                                let ch = input[scan..].chars().next().expect("valid UTF-8 cursor");
                                let after = scan + ch.len_utf8();
                                if ch == '\\' {
                                    if after < input.len() {
                                        let escaped = input[after..].chars().next().expect("valid UTF-8 cursor");
                                        scan = after + escaped.len_utf8();
                                    } else {
                                        scan = after;
                                    }
                                } else if ch == '}' {
                                    close = Some(scan);
                                    break;
                                } else {
                                    scan = after;
                                }
                            }
                            let Some(close_pos) = close else {
                                return Err(SplitError::UnterminatedExpansion { pos: dollar });
                            };
                            let content = &input[content_start..close_pos];
                            let name_end = content
                                .char_indices()
                                .find_map(|(idx, ch)| {
                                    if is_name_continue(ch) {
                                        None
                                    } else {
                                        Some(idx)
                                    }
                                })
                                .unwrap_or(content.len());
                            let name = &content[..name_end];
                            if name.is_empty()
                                || !name.chars().next().is_some_and(is_name_start)
                            {
                                return Err(SplitError::InvalidExpansion { pos: dollar });
                            }
                            let default = if name_end == content.len() {
                                None
                            } else if content[name_end..].starts_with(":-") {
                                Some(&content[name_end + 2..])
                            } else {
                                return Err(SplitError::InvalidExpansion { pos: dollar });
                            };
                            let selected = match (lookup(name, env), default) {
                                (Some(value), Some(text)) if value.is_empty() => {
                                    decode_default(text)
                                }
                                (Some(value), _) => value.to_string(),
                                (None, Some(text)) => decode_default(text),
                                (None, None) => String::new(),
                            };
                            for ch in selected.chars() {
                                append_char(&mut current, ch, true);
                            }
                            i = close_pos + 1;
                        } else {
                            append_char(&mut current, '$', true);
                            i = next_i;
                        }
                    } else {
                        append_char(&mut current, '$', true);
                        i = next_i;
                    }
                } else {
                    active = true;
                    append_char(&mut current, c, true);
                    i = next_i;
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

fn fallback_candidate(word: &str) -> String {
    let mut out = String::from("'");
    for c in word.chars() {
        if c == '\'' {
            out.push('\'');
            out.push('\\');
            out.push('\'');
            out.push('\'');
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
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

        let chosen = if candidates.is_empty() {
            fallback_candidate(word)
        } else {
            candidates
                .into_iter()
                .min_by_key(|(candidate, priority)| (candidate.len(), *priority))
                .expect("nonempty candidate list")
                .0
        };
        encoded.push(chosen);
    }

    Ok(encoded.join(" "))
}

fn main() {}