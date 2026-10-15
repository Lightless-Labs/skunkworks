#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitError {
    Nul { pos: usize },
    TrailingEscape { pos: usize },
    UnterminatedSingle { pos: usize },
    UnterminatedDouble { pos: usize },
    UnterminatedExpansion { pos: usize },
    InvalidExpansion { pos: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Quote {
    None,
    Single(usize),
    Double(usize),
}

fn is_name_start(c: char) -> bool {
    c == '_' || c.is_ascii_alphabetic()
}

fn is_name_char(c: char) -> bool {
    is_name_start(c) || c.is_ascii_digit()
}

fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

fn lookup<'a>(env: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| (*key == name).then_some(*value))
}

fn decode_default(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.char_indices().peekable();

    while let Some((_, c)) = chars.next() {
        if c == '\\' {
            if let Some((_, next)) = chars.next() {
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

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<String>, SplitError> {
    if let Some(pos) = input.find('\0') {
        return Err(SplitError::Nul { pos });
    }

    let mut words = Vec::new();
    let mut word = String::new();
    let mut active = false;
    let mut quote = Quote::None;
    let mut iter = input.char_indices().peekable();

    while let Some((pos, c)) = iter.next() {
        match quote {
            Quote::Single(_) => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    word.push(c);
                }
            }
            Quote::Double(open_pos) => {
                if c == '"' {
                    quote = Quote::None;
                } else if c == '\\' {
                    match iter.peek().copied() {
                        None => return Err(SplitError::TrailingEscape { pos }),
                        Some((_, next)) if matches!(next, '"' | '\\' | '$') => {
                            iter.next();
                            word.push(next);
                        }
                        Some((_, '\n')) => {
                            iter.next();
                        }
                        Some(_) => word.push('\\'),
                    }
                } else if c == '$' {
                    active = true;
                    expand(input, &mut iter, pos, env, &mut word)?;
                } else {
                    word.push(c);
                }
                let _ = open_pos;
            }
            Quote::None => {
                if is_separator(c) {
                    if active {
                        words.push(std::mem::take(&mut word));
                        active = false;
                    }
                } else if c == '\'' {
                    active = true;
                    quote = Quote::Single(pos);
                } else if c == '"' {
                    active = true;
                    quote = Quote::Double(pos);
                } else if c == '\\' {
                    match iter.peek().copied() {
                        None => return Err(SplitError::TrailingEscape { pos }),
                        Some((_, '\n')) => {
                            iter.next();
                        }
                        Some(_) => {
                            let (_, next) = iter.next().expect("peeked character");
                            active = true;
                            word.push(next);
                        }
                    }
                } else if c == '$' {
                    if is_expansion_start(iter.peek().map(|(_, ch)| *ch)) {
                        active = true;
                    }
                    expand(input, &mut iter, pos, env, &mut word)?;
                } else {
                    active = true;
                    word.push(c);
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

fn is_expansion_start(next: Option<char>) -> bool {
    matches!(next, Some('{')) || next.is_some_and(is_name_start)
}

fn expand(
    input: &str,
    iter: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
    dollar_pos: usize,
    env: &[(&str, &str)],
    word: &mut String,
) -> Result<(), SplitError> {
    match iter.peek().copied() {
        Some((_, '{')) => {
            iter.next();

            let mut close = None;
            let mut search = input[dollar_pos + 2..].char_indices().peekable();
            while let Some((offset, ch)) = search.next() {
                if ch == '\\' {
                    search.next();
                } else if ch == '}' {
                    close = Some(dollar_pos + 2 + offset);
                    break;
                }
            }

            let close_pos = close.ok_or(SplitError::UnterminatedExpansion { pos: dollar_pos })?;
            let content = &input[dollar_pos + 2..close_pos];

            let mut name_end = 0;
            let mut content_chars = content.char_indices();
            match content_chars.next() {
                Some((_, first)) if is_name_start(first) => {
                    name_end = first.len_utf8();
                    while let Some((offset, ch)) = content_chars.next() {
                        if is_name_char(ch) {
                            name_end = offset + ch.len_utf8();
                        } else {
                            break;
                        }
                    }
                }
                _ => return Err(SplitError::InvalidExpansion { pos: dollar_pos }),
            }

            let name = &content[..name_end];
            let suffix = &content[name_end..];
            let (value, default_text) = if suffix.is_empty() {
                (lookup(env, name).unwrap_or(""), None)
            } else if let Some(text) = suffix.strip_prefix(":-") {
                ("", Some(text))
            } else {
                return Err(SplitError::InvalidExpansion { pos: dollar_pos });
            };

            let selected = if let Some(text) = default_text {
                match lookup(env, name) {
                    Some(value) if !value.is_empty() => value.to_string(),
                    _ => decode_default(text),
                }
            } else {
                value.to_string()
            };
            word.push_str(&selected);

            while let Some((pos, _)) = iter.peek().copied() {
                if pos <= close_pos {
                    iter.next();
                } else {
                    break;
                }
            }
        }
        Some((_, first)) if is_name_start(first) => {
            let mut name = String::new();
            while let Some((_, ch)) = iter.peek().copied() {
                if is_name_char(ch) {
                    name.push(ch);
                    iter.next();
                } else {
                    break;
                }
            }
            if let Some(value) = lookup(env, &name) {
                word.push_str(value);
            }
        }
        _ => word.push('$'),
    }

    Ok(())
}