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
    c.is_ascii_alphanumeric() || c == '_'
}

fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

fn append_text(word: &mut Word, s: &str, wild: bool) {
    for c in s.chars() {
        if wild && matches!(c, '*' | '?' | '[') {
            word.glob.push(word.text.len());
        }
        word.text.push(c);
    }
}

fn first_matching_env<'a>(env: &'a [(&'a str, &'a str)], name: &str) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| if *key == name { Some(*value) } else { None })
}

fn parse_braced(input: &str, dollar: usize) -> Result<(usize, String, Option<String>), SplitError> {
    let bytes = input.as_bytes();
    let mut close = None;
    let mut i = dollar + 2;

    while i < bytes.len() {
        let c = input[i..].chars().next().unwrap();
        if c == '\\' {
            i += 1;
            if i < bytes.len() {
                i += input[i..].chars().next().unwrap().len_utf8();
            }
        } else if c == '}' {
            close = Some(i);
            break;
        } else {
            i += c.len_utf8();
        }
    }

    let close = close.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
    let content = &input[dollar + 2..close];
    let chars: Vec<(usize, char)> = content.char_indices().collect();
    let mut n = 0usize;

    if chars.is_empty() || !is_name_start(chars[0].1) {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }
    n += 1;
    while n < chars.len() && is_name_char(chars[n].1) {
        n += 1;
    }

    let name_end = if n < chars.len() { chars[n].0 } else { content.len() };
    let name = content[..name_end].to_string();

    let default = if n == chars.len() {
        None
    } else {
        let rest = &content[name_end..];
        if !rest.starts_with(":-") {
            return Err(SplitError::InvalidExpansion { pos: dollar });
        }
        Some(rest[2..].to_string())
    };

    Ok((close + 1, name, default))
}

fn decode_default(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' {
            if let Some(next) = it.next() {
                out.push(next);
            } else {
                out.push('\\');
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<Word>, SplitError> {
    if let Some(pos) = input.as_bytes().iter().position(|&b| b == 0) {
        return Err(SplitError::Nul { pos });
    }

    let mut words = Vec::new();
    let mut word = Word { text: String::new(), glob: Vec::new() };
    let mut active = false;
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
                    append_text(&mut word, &input[i..next_i], false);
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
                    let d = input[next_i..].chars().next().unwrap();
                    let after = next_i + d.len_utf8();
                    if matches!(d, '"' | '\\' | '$' | '\n') {
                        if d != '\n' {
                            append_text(&mut word, &input[next_i..after], false);
                        }
                        i = after;
                    } else {
                        append_text(&mut word, "\\", false);
                        i = next_i;
                    }
                } else if c == '$' {
                    if input[next_i..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }
                    if input[next_i..].starts_with('{') {
                        let (end, name, default) = parse_braced(input, i)?;
                        active = true;
                        let value = first_matching_env(env, &name).unwrap_or("");
                        if let Some(text) = default {
                            if value.is_empty() {
                                let decoded = decode_default(&text);
                                append_text(&mut word, &decoded, false);
                            } else {
                                append_text(&mut word, value, false);
                            }
                        } else {
                            append_text(&mut word, value, false);
                        }
                        i = end;
                    } else if let Some(d) = input[next_i..].chars().next() {
                        if is_name_start(d) {
                            let mut end = next_i + d.len_utf8();
                            while end < input.len() {
                                let x = input[end..].chars().next().unwrap();
                                if is_name_char(x) {
                                    end += x.len_utf8();
                                } else {
                                    break;
                                }
                            }
                            let name = &input[next_i..end];
                            active = true;
                            append_text(&mut word, first_matching_env(env, name).unwrap_or(""), false);
                            i = end;
                        } else {
                            append_text(&mut word, "$", false);
                            i = next_i;
                        }
                    } else {
                        append_text(&mut word, "$", false);
                        i = next_i;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else {
                    append_text(&mut word, &input[i..next_i], false);
                    i = next_i;
                }
            }
            Quote::None => {
                if is_separator(c) {
                    if active {
                        words.push(word);
                        word = Word { text: String::new(), glob: Vec::new() };
                        active = false;
                    }
                    i = next_i;
                } else if c == '\'' {
                    active = true;
                    quote = Quote::Single;
                    quote_pos = i;
                    i = next_i;
                } else if c == '"' {
                    active = true;
                    quote = Quote::Double;
                    quote_pos = i;
                    i = next_i;
                } else if c == '\\' {
                    if next_i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let d = input[next_i..].chars().next().unwrap();
                    let after = next_i + d.len_utf8();
                    if d == '\n' {
                        i = after;
                    } else {
                        active = true;
                        append_text(&mut word, &input[next_i..after], false);
                        i = after;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if input[next_i..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }
                    if input[next_i..].starts_with('{') {
                        let (end, name, default) = parse_braced(input, i)?;
                        active = true;
                        let value = first_matching_env(env, &name).unwrap_or("");
                        if let Some(text) = default {
                            if value.is_empty() {
                                let decoded = decode_default(&text);
                                append_text(&mut word, &decoded, true);
                            } else {
                                append_text(&mut word, value, true);
                            }
                        } else {
                            append_text(&mut word, value, true);
                        }
                        i = end;
                    } else if let Some(d) = input[next_i..].chars().next() {
                        if is_name_start(d) {
                            let mut end = next_i + d.len_utf8();
                            while end < input.len() {
                                let x = input[end..].chars().next().unwrap();
                                if is_name_char(x) {
                                    end += x.len_utf8();
                                } else {
                                    break;
                                }
                            }
                            let name = &input[next_i..end];
                            active = true;
                            append_text(&mut word, first_matching_env(env, name).unwrap_or(""), true);
                            i = end;
                        } else {
                            active = true;
                            append_text(&mut word, "$", false);
                            i = next_i;
                        }
                    } else {
                        active = true;
                        append_text(&mut word, "$", false);
                        i = next_i;
                    }
                } else {
                    active = true;
                    append_text(&mut word, &input[i..next_i], true);
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
            return Err(JoinError::Nul { word: word_index, pos });
        }
    }

    let mut encoded = Vec::with_capacity(words.len());
    for word in words {
        let mut candidates = Vec::new();
        if let Some(s) = bare_candidate(word) {
            candidates.push((s.len(), 0u8, s));
        }
        if let Some(s) = single_candidate(word) {
            candidates.push((s.len(), 1u8, s));
        }
        if let Some(s) = double_candidate(word) {
            candidates.push((s.len(), 2u8, s));
        }

        let choice = if candidates.is_empty() {
            let mut s = String::from("'");
            for c in word.chars() {
                if c == '\'' {
                    s.push('\'');
                    s.push('\\');
                    s.push('\'');
                    s.push('\'');
                } else {
                    s.push(c);
                }
            }
            s.push('\'');
            s
        } else {
            candidates.sort_by_key(|(len, priority, _)| (*len, *priority));
            candidates.remove(0).2
        };
        encoded.push(choice);
    }
    Ok(encoded.join(" "))
}
