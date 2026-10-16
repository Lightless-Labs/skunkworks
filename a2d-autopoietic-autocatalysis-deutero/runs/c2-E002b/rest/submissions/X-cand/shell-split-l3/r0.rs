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

fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

fn append_scalar(word: &mut Word, c: char, active_glob: bool) {
    if active_glob && matches!(c, '*' | '?' | '[') {
        word.glob.push(word.text.len());
    }
    word.text.push(c);
}

fn append_string(word: &mut Word, value: &str, active_glob: bool) {
    for c in value.chars() {
        append_scalar(word, c, active_glob);
    }
}

fn find_braced_close(input: &str, mut i: usize) -> Option<usize> {
    while i < input.len() {
        let c = input[i..].chars().next()?;
        if c == '\\' {
            i += c.len_utf8();
            if i < input.len() {
                let next = input[i..].chars().next()?;
                i += next.len_utf8();
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
        i += c.len_utf8();
        if c == '\\' && i < text.len() {
            let next = text[i..].chars().next().unwrap();
            i += next.len_utf8();
            out.push(next);
        } else {
            out.push(c);
        }
    }
    out
}

fn lookup<'a>(name: &str, env: &'a [(&str, &str)]) -> Option<&'a str> {
    env.iter().find(|(key, _)| *key == name).map(|(_, value)| *value)
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
                    append_scalar(&mut word, c, false);
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
                    let next = input[next_i..].chars().next().unwrap();
                    let after_next = next_i + next.len_utf8();
                    if matches!(next, '"' | '\\' | '$' | '\n') {
                        if next != '\n' {
                            append_scalar(&mut word, next, false);
                        }
                        i = after_next;
                    } else {
                        append_scalar(&mut word, '\\', false);
                        i = next_i;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if next_i < input.len() && input[next_i..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }
                    if next_i < input.len() && input[next_i..].starts_with('{') {
                        let content_start = next_i + 1;
                        let close = find_braced_close(input, content_start)
                            .ok_or(SplitError::UnterminatedExpansion { pos: i })?;
                        let contents = &input[content_start..close];
                        let name_end = contents
                            .char_indices()
                            .find(|(_, ch)| !is_name_continue(*ch))
                            .map(|(idx, _)| idx)
                            .unwrap_or(contents.len());
                        let name = &contents[..name_end];
                        if name.is_empty() || !is_name_start(name.chars().next().unwrap()) {
                            return Err(SplitError::InvalidExpansion { pos: i });
                        }
                        let suffix = &contents[name_end..];
                        let value = if suffix.is_empty() {
                            lookup(name, env).unwrap_or("")
                        } else if let Some(default) = suffix.strip_prefix(":-") {
                            match lookup(name, env) {
                                Some(v) if !v.is_empty() => v,
                                _ => {
                                    let decoded = decode_default(default);
                                    append_string(&mut word, &decoded, false);
                                    active = true;
                                    i = close + 1;
                                    continue;
                                }
                            }
                        } else {
                            return Err(SplitError::InvalidExpansion { pos: i });
                        };
                        append_string(&mut word, value, false);
                        active = true;
                        i = close + 1;
                    } else if next_i < input.len() {
                        let first = input[next_i..].chars().next().unwrap();
                        if is_name_start(first) {
                            let mut end = next_i + first.len_utf8();
                            while end < input.len() {
                                let ch = input[end..].chars().next().unwrap();
                                if !is_name_continue(ch) {
                                    break;
                                }
                                end += ch.len_utf8();
                            }
                            let value = lookup(&input[next_i..end], env).unwrap_or("");
                            append_string(&mut word, value, false);
                            active = true;
                            i = end;
                        } else {
                            append_scalar(&mut word, '$', false);
                            i = next_i;
                        }
                    } else {
                        append_scalar(&mut word, '$', false);
                        i = next_i;
                    }
                } else {
                    append_scalar(&mut word, c, false);
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
                } else if c == '\'' || c == '"' {
                    quote = if c == '\'' { Quote::Single } else { Quote::Double };
                    quote_pos = i;
                    active = true;
                    i = next_i;
                } else if c == '\\' {
                    if next_i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let next = input[next_i..].chars().next().unwrap();
                    let after_next = next_i + next.len_utf8();
                    if next != '\n' {
                        append_scalar(&mut word, next, false);
                        active = true;
                    }
                    i = after_next;
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if next_i < input.len() && input[next_i..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }
                    if next_i < input.len() && input[next_i..].starts_with('{') {
                        let content_start = next_i + 1;
                        let close = find_braced_close(input, content_start)
                            .ok_or(SplitError::UnterminatedExpansion { pos: i })?;
                        let contents = &input[content_start..close];
                        let name_end = contents
                            .char_indices()
                            .find(|(_, ch)| !is_name_continue(*ch))
                            .map(|(idx, _)| idx)
                            .unwrap_or(contents.len());
                        let name = &contents[..name_end];
                        if name.is_empty() || !is_name_start(name.chars().next().unwrap()) {
                            return Err(SplitError::InvalidExpansion { pos: i });
                        }
                        let suffix = &contents[name_end..];
                        let value = if suffix.is_empty() {
                            lookup(name, env).unwrap_or("")
                        } else if let Some(default) = suffix.strip_prefix(":-") {
                            match lookup(name, env) {
                                Some(v) if !v.is_empty() => v,
                                _ => {
                                    let decoded = decode_default(default);
                                    append_string(&mut word, &decoded, true);
                                    active = true;
                                    i = close + 1;
                                    continue;
                                }
                            }
                        } else {
                            return Err(SplitError::InvalidExpansion { pos: i });
                        };
                        append_string(&mut word, value, true);
                        active = true;
                        i = close + 1;
                    } else if next_i < input.len() {
                        let first = input[next_i..].chars().next().unwrap();
                        if is_name_start(first) {
                            let mut end = next_i + first.len_utf8();
                            while end < input.len() {
                                let ch = input[end..].chars().next().unwrap();
                                if !is_name_continue(ch) {
                                    break;
                                }
                                end += ch.len_utf8();
                            }
                            let value = lookup(&input[next_i..end], env).unwrap_or("");
                            append_string(&mut word, value, true);
                            active = true;
                            i = end;
                        } else {
                            append_scalar(&mut word, '$', true);
                            active = true;
                            i = next_i;
                        }
                    } else {
                        append_scalar(&mut word, '$', true);
                        active = true;
                        i = next_i;
                    }
                } else {
                    append_scalar(&mut word, c, true);
                    active = true;
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

    if active {
        words.push(word);
    }
    Ok(words)
}