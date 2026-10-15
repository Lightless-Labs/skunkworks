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
    Single(usize),
    Double(usize),
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_name_char(c: char) -> bool {
    is_name_start(c) || c.is_ascii_digit()
}

fn lookup<'a>(name: &str, env: &'a [(&str, &str)]) -> Option<&'a str> {
    if name.is_empty() {
        return None;
    }
    env.iter()
        .find_map(|(key, value)| (*key == name).then_some(*value))
}

fn append(word: &mut Word, s: &str, active_glob: bool) {
    let base = word.text.len();
    if active_glob {
        for (i, c) in s.char_indices() {
            if matches!(c, '*' | '?' | '[') {
                word.glob.push(base + i);
            }
        }
    }
    word.text.push_str(s);
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
                    append(&mut word, &input[i..next_i], false);
                }
                i = next_i;
            }
            Quote::Double(_) => {
                if c == '"' {
                    quote = Quote::None;
                    i = next_i;
                } else if c == '\\' {
                    let slash = i;
                    if next_i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: slash });
                    }
                    let escaped = input[next_i..].chars().next().unwrap();
                    let after = next_i + escaped.len_utf8();
                    if matches!(escaped, '"' | '\\' | '$' | '\n') {
                        if escaped != '\n' {
                            append(&mut word, &input[next_i..after], false);
                        }
                        i = after;
                    } else {
                        append(&mut word, "\\", false);
                        i = next_i;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if input[next_i..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }
                    if input[next_i..].starts_with('{') {
                        let dollar = i;
                        let content_start = next_i + 1;
                        let mut j = content_start;
                        let mut close = None;
                        while j < input.len() {
                            let ch = input[j..].chars().next().unwrap();
                            let after = j + ch.len_utf8();
                            if ch == '\\' {
                                if after < input.len() {
                                    let skipped = input[after..].chars().next().unwrap();
                                    j = after + skipped.len_utf8();
                                } else {
                                    j = after;
                                }
                            } else if ch == '}' {
                                close = Some(j);
                                break;
                            } else {
                                j = after;
                            }
                        }
                        let Some(close_pos) = close else {
                            return Err(SplitError::UnterminatedExpansion { pos: dollar });
                        };
                        let contents = &input[content_start..close_pos];
                        let (name, default) = parse_braced(contents)
                            .ok_or(SplitError::InvalidExpansion { pos: dollar })?;
                        let value = match default {
                            Some(text) => match lookup(name, env) {
                                Some(v) if !v.is_empty() => v.to_string(),
                                _ => decode_default(text),
                            },
                            None => lookup(name, env).unwrap_or("").to_string(),
                        };
                        append(&mut word, &value, false);
                        active = true;
                        i = close_pos + 1;
                    } else if let Some((name, end)) = parse_dollar_name(input, next_i) {
                        let value = lookup(name, env).unwrap_or("");
                        append(&mut word, value, false);
                        active = true;
                        i = end;
                    } else {
                        append(&mut word, "$", false);
                        active = true;
                        i = next_i;
                    }
                } else {
                    append(&mut word, &input[i..next_i], false);
                    i = next_i;
                }
            }
            Quote::None => {
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
                    quote = Quote::Single(i);
                    active = true;
                    i = next_i;
                } else if c == '"' {
                    quote = Quote::Double(i);
                    active = true;
                    i = next_i;
                } else if c == '\\' {
                    let slash = i;
                    if next_i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: slash });
                    }
                    let escaped = input[next_i..].chars().next().unwrap();
                    let after = next_i + escaped.len_utf8();
                    if escaped != '\n' {
                        append(&mut word, &input[next_i..after], false);
                        active = true;
                    }
                    i = after;
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if input[next_i..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }
                    if input[next_i..].starts_with('{') {
                        let dollar = i;
                        let content_start = next_i + 1;
                        let mut j = content_start;
                        let mut close = None;
                        while j < input.len() {
                            let ch = input[j..].chars().next().unwrap();
                            let after = j + ch.len_utf8();
                            if ch == '\\' {
                                if after < input.len() {
                                    let skipped = input[after..].chars().next().unwrap();
                                    j = after + skipped.len_utf8();
                                } else {
                                    j = after;
                                }
                            } else if ch == '}' {
                                close = Some(j);
                                break;
                            } else {
                                j = after;
                            }
                        }
                        let Some(close_pos) = close else {
                            return Err(SplitError::UnterminatedExpansion { pos: dollar });
                        };
                        let contents = &input[content_start..close_pos];
                        let (name, default) = parse_braced(contents)
                            .ok_or(SplitError::InvalidExpansion { pos: dollar })?;
                        let value = match default {
                            Some(text) => match lookup(name, env) {
                                Some(v) if !v.is_empty() => v.to_string(),
                                _ => decode_default(text),
                            },
                            None => lookup(name, env).unwrap_or("").to_string(),
                        };
                        append(&mut word, &value, true);
                        active = true;
                        i = close_pos + 1;
                    } else if let Some((name, end)) = parse_dollar_name(input, next_i) {
                        let value = lookup(name, env).unwrap_or("");
                        append(&mut word, value, true);
                        active = true;
                        i = end;
                    } else {
                        append(&mut word, "$", false);
                        active = true;
                        i = next_i;
                    }
                } else {
                    append(&mut word, &input[i..next_i], matches!(c, '*' | '?' | '['));
                    active = true;
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

fn parse_dollar_name(input: &str, start: usize) -> Option<(&str, usize)> {
    let first = input[start..].chars().next()?;
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
    Some((&input[start..end], end))
}

fn parse_braced(contents: &str) -> Option<(&str, Option<&str>)> {
    let mut chars = contents.char_indices();
    let (first_idx, first) = chars.next()?;
    if first_idx != 0 || !is_name_start(first) {
        return None;
    }
    let mut name_end = first.len_utf8();
    while name_end < contents.len() {
        let c = contents[name_end..].chars().next().unwrap();
        if !is_name_char(c) {
            break;
        }
        name_end += c.len_utf8();
    }
    let name = &contents[..name_end];
    let rest = &contents[name_end..];
    if rest.is_empty() {
        Some((name, None))
    } else if let Some(default) = rest.strip_prefix(":-") {
        Some((name, Some(default)))
    } else {
        None
    }
}

fn decode_default(text: &str) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < text.len() {
        let c = text[i..].chars().next().unwrap();
        let next = i + c.len_utf8();
        if c == '\\' && next < text.len() {
            let escaped = text[next..].chars().next().unwrap();
            let end = next + escaped.len_utf8();
            out.push(escaped);
            i = end;
        } else {
            out.push(c);
            i = next;
        }
    }
    out
}

fn main() {}