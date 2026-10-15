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

fn is_name_start(c: char) -> bool {
    c == '_' || c.is_ascii_alphabetic()
}

fn is_name_continue(c: char) -> bool {
    is_name_start(c) || c.is_ascii_digit()
}

fn env_value<'a>(name: &str, env: &'a [(&str, &str)]) -> Option<&'a str> {
    if name.is_empty() {
        return None;
    }
    env.iter()
        .find_map(|(key, value)| if *key == name { Some(*value) } else { None })
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

fn next_char(input: &str, pos: usize) -> Option<(char, usize)> {
    input.get(pos..)?.chars().next().map(|c| (c, pos + c.len_utf8()))
}

fn scan_braced(input: &str, dollar: usize, env: &[(&str, &str)]) -> Result<(String, usize), SplitError> {
    let mut cursor = dollar + 2;
    let mut close = None;
    while cursor < input.len() {
        let (c, next) = next_char(input, cursor).unwrap();
        if c == '\\' {
            if next < input.len() {
                let (_, after) = next_char(input, next).unwrap();
                cursor = after;
            } else {
                cursor = next;
            }
        } else if c == '}' {
            close = Some(cursor);
            break;
        } else {
            cursor = next;
        }
    }

    let close = close.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
    let contents = &input[dollar + 2..close];
    let bytes = contents.as_bytes();
    let mut i = 0;
    if i >= bytes.len() || !(bytes[i] == b'_' || bytes[i].is_ascii_alphabetic()) {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }
    i += 1;
    while i < bytes.len()
        && (bytes[i] == b'_' || bytes[i].is_ascii_alphanumeric())
    {
        i += 1;
    }
    let name = &contents[..i];
    let value = env_value(name, env).unwrap_or("");

    if i == bytes.len() {
        return Ok((value.to_string(), close + 1));
    }
    if bytes.get(i..i + 2) != Some(b":-") {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }
    let default_start = i + 2;
    let default = &contents[default_start..];
    if !value.is_empty() {
        return Ok((value.to_string(), close + 1));
    }

    let mut decoded = String::new();
    let mut p = 0;
    while p < default.len() {
        let (c, next) = next_char(default, p).unwrap();
        if c == '\\' && next < default.len() {
            let (escaped, after) = next_char(default, next).unwrap();
            decoded.push(escaped);
            p = after;
        } else {
            decoded.push(c);
            p = next;
        }
    }
    Ok((decoded, close + 1))
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<Word>, SplitError> {
    if let Some(pos) = input.find('\0') {
        return Err(SplitError::Nul { pos });
    }

    let mut words = Vec::new();
    let mut current: Option<Word> = None;
    let mut pos = 0;

    while pos < input.len() {
        let (c, next) = next_char(input, pos).unwrap();
        match c {
            ' ' | '\t' | '\r' | '\n' => {
                if let Some(word) = current.take() {
                    words.push(word);
                }
                pos = next;
            }
            '\'' => {
                let start = pos;
                let mut word = current.take().unwrap_or(Word {
                    text: String::new(),
                    glob: Vec::new(),
                });
                pos = next;
                let mut closed = false;
                while pos < input.len() {
                    let (inside, after) = next_char(input, pos).unwrap();
                    if inside == '\'' {
                        pos = after;
                        closed = true;
                        break;
                    }
                    word.text.push(inside);
                    pos = after;
                }
                if !closed {
                    return Err(SplitError::UnterminatedSingle { pos: start });
                }
                current = Some(word);
            }
            '"' => {
                let start = pos;
                let mut word = current.take().unwrap_or(Word {
                    text: String::new(),
                    glob: Vec::new(),
                });
                pos = next;
                let mut closed = false;
                while pos < input.len() {
                    let (inside, after) = next_char(input, pos).unwrap();
                    match inside {
                        '"' => {
                            pos = after;
                            closed = true;
                            break;
                        }
                        '\\' => {
                            if after >= input.len() {
                                return Err(SplitError::TrailingEscape { pos });
                            }
                            let (escaped, escaped_after) = next_char(input, after).unwrap();
                            if matches!(escaped, '"' | '\\' | '$' | '\n') {
                                if escaped != '\n' {
                                    word.text.push(escaped);
                                }
                                pos = escaped_after;
                            } else {
                                word.text.push('\\');
                                pos = after;
                            }
                        }
                        '`' => {
                            return Err(SplitError::CommandSubstitution { pos });
                        }
                        '$' => {
                            if let Some((following, _)) = next_char(input, after) {
                                if following == '(' {
                                    return Err(SplitError::CommandSubstitution { pos });
                                }
                            }
                            if after < input.len() {
                                let (following, _) = next_char(input, after).unwrap();
                                if following == '\\' && after + 1 < input.len() {
                                    let (escaped, _) = next_char(input, after + 1).unwrap();
                                    if escaped == '$' {
                                        word.text.push('$');
                                        pos = after + 1 + escaped.len_utf8();
                                        continue;
                                    }
                                }
                            }
                            if after < input.len() && input.as_bytes()[after] == b'{' {
                                let (value, end) = scan_braced(input, pos, env)?;
                                append_expansion(&mut word, &value, false);
                                pos = end;
                            } else if let Some((name_start, name_end)) = next_char(input, after) {
                                if is_name_start(name_start) {
                                    let mut end = name_end;
                                    while end < input.len() {
                                        let (ch, after_ch) = next_char(input, end).unwrap();
                                        if is_name_continue(ch) {
                                            end = after_ch;
                                        } else {
                                            break;
                                        }
                                    }
                                    let value = env_value(&input[after..end], env).unwrap_or("");
                                    append_expansion(&mut word, value, false);
                                    pos = end;
                                } else {
                                    word.text.push('$');
                                    pos = after;
                                }
                            } else {
                                word.text.push('$');
                                pos = after;
                            }
                        }
                        _ => {
                            word.text.push(inside);
                            pos = after;
                        }
                    }
                }
                if !closed {
                    return Err(SplitError::UnterminatedDouble { pos: start });
                }
                current = Some(word);
            }
            '\\' => {
                if next >= input.len() {
                    return Err(SplitError::TrailingEscape { pos });
                }
                let (escaped, after) = next_char(input, next).unwrap();
                if escaped == '\n' {
                    pos = after;
                } else {
                    let mut word = current.take().unwrap_or(Word {
                        text: String::new(),
                        glob: Vec::new(),
                    });
                    word.text.push(escaped);
                    current = Some(word);
                    pos = after;
                }
            }
            '`' => return Err(SplitError::CommandSubstitution { pos }),
            '$' => {
                if let Some((following, _)) = next_char(input, next) {
                    if following == '(' {
                        return Err(SplitError::CommandSubstitution { pos });
                    }
                }
                let mut word = current.take().unwrap_or(Word {
                    text: String::new(),
                    glob: Vec::new(),
                });
                if next < input.len() && input.as_bytes()[next] == b'{' {
                    let (value, end) = scan_braced(input, pos, env)?;
                    append_expansion(&mut word, &value, true);
                    pos = end;
                } else if let Some((name_start, name_end)) = next_char(input, next) {
                    if is_name_start(name_start) {
                        let mut end = name_end;
                        while end < input.len() {
                            let (ch, after_ch) = next_char(input, end).unwrap();
                            if is_name_continue(ch) {
                                end = after_ch;
                            } else {
                                break;
                            }
                        }
                        let value = env_value(&input[next..end], env).unwrap_or("");
                        append_expansion(&mut word, value, true);
                        pos = end;
                    } else {
                        word.text.push('$');
                        pos = next;
                    }
                } else {
                    word.text.push('$');
                    pos = next;
                }
                current = Some(word);
            }
            _ => {
                let mut word = current.take().unwrap_or(Word {
                    text: String::new(),
                    glob: Vec::new(),
                });
                append_literal(&mut word, c, true);
                current = Some(word);
                pos = next;
            }
        }
    }

    if let Some(word) = current {
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

        let chosen = if candidates.is_empty() {
            fallback_candidate(word)
        } else {
            candidates
                .into_iter()
                .min_by(|a, b| {
                    a.1.len()
                        .cmp(&b.1.len())
                        .then_with(|| a.0.cmp(&b.0))
                })
                .unwrap()
                .1
        };
        encoded.push(chosen);
    }
    Ok(encoded.join(" "))
}

fn main() {}