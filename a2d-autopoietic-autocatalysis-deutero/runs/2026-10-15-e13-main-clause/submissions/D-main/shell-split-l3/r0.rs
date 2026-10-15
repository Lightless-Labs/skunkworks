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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

fn append_text(word: &mut Word, text: &str, active_glob: bool) {
    for c in text.chars() {
        if active_glob && matches!(c, '*' | '?' | '[') {
            word.glob.push(word.text.len());
        }
        word.text.push(c);
    }
}

fn append_char(word: &mut Word, c: char, active_glob: bool) {
    if active_glob && matches!(c, '*' | '?' | '[') {
        word.glob.push(word.text.len());
    }
    word.text.push(c);
}

fn lookup<'a>(env: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| (*key == name).then_some(*value))
}

fn decode_default(text: &str) -> String {
    let mut result = String::new();
    let mut chars = text.char_indices().peekable();

    while let Some((_, c)) = chars.next() {
        if c == '\\' {
            if let Some((_, next)) = chars.next() {
                result.push(next);
            } else {
                result.push('\\');
            }
        } else {
            result.push(c);
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
    let mut quote = Quote::None;
    let mut quote_start = 0usize;
    let mut i = 0usize;

    while i < input.len() {
        let c = input[i..].chars().next().expect("valid character boundary");
        let next_i = i + c.len_utf8();

        match quote {
            Quote::Single => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    append_char(&mut word, c, false);
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
                    let next = input[next_i..]
                        .chars()
                        .next()
                        .expect("nonempty remainder");
                    let after_next = next_i + next.len_utf8();
                    if matches!(next, '"' | '\\' | '$' | '\n') {
                        if next != '\n' {
                            append_char(&mut word, next, false);
                        }
                        i = after_next;
                    } else {
                        append_char(&mut word, '\\', false);
                        i = next_i;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if input[next_i..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }
                    active = true;
                    if input[next_i..].starts_with('{') {
                        let (value, end) = parse_braced(input, i, env)?;
                        append_text(&mut word, &value, false);
                        i = end;
                    } else if let Some((name, end)) = parse_dollar_name(input, next_i) {
                        if let Some(value) = lookup(env, name) {
                            append_text(&mut word, value, false);
                        }
                        i = end;
                    } else {
                        append_char(&mut word, '$', false);
                        i = next_i;
                    }
                } else {
                    append_char(&mut word, c, false);
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
                    active = true;
                    quote = Quote::Single;
                    quote_start = i;
                    i = next_i;
                } else if c == '"' {
                    active = true;
                    quote = Quote::Double;
                    quote_start = i;
                    i = next_i;
                } else if c == '\\' {
                    if next_i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let next = input[next_i..]
                        .chars()
                        .next()
                        .expect("nonempty remainder");
                    let after_next = next_i + next.len_utf8();
                    if next == '\n' {
                        i = after_next;
                    } else {
                        active = true;
                        append_char(&mut word, next, false);
                        i = after_next;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if input[next_i..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }
                    active = true;
                    if input[next_i..].starts_with('{') {
                        let (value, end) = parse_braced(input, i, env)?;
                        append_text(&mut word, &value, true);
                        i = end;
                    } else if let Some((name, end)) = parse_dollar_name(input, next_i) {
                        if let Some(value) = lookup(env, name) {
                            append_text(&mut word, value, true);
                        }
                        i = end;
                    } else {
                        append_char(&mut word, '$', true);
                        i = next_i;
                    }
                } else {
                    active = true;
                    append_char(&mut word, c, true);
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
        let c = input[end..].chars().next().expect("valid character boundary");
        if !is_name_continue(c) {
            break;
        }
        end += c.len_utf8();
    }

    Some((&input[start..end], end))
}

fn parse_braced(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
) -> Result<(String, usize), SplitError> {
    let content_start = dollar + 2;
    let mut cursor = content_start;
    let mut close = None;

    while cursor < input.len() {
        let c = input[cursor..].chars().next().expect("valid character boundary");
        let next = cursor + c.len_utf8();
        if c == '\\' {
            if next < input.len() {
                let escaped = input[next..].chars().next().expect("nonempty remainder");
                cursor = next + escaped.len_utf8();
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
    let contents = &input[content_start..close];

    let (name, default) = if let Some((name, end)) = parse_dollar_name(contents, 0) {
        if end == contents.len() {
            (name, None)
        } else if contents[end..].starts_with(":-") {
            (name, Some(&contents[end + 2..]))
        } else {
            return Err(SplitError::InvalidExpansion { pos: dollar });
        }
    } else {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    };

    let selected = lookup(env, name).filter(|value| !value.is_empty());
    let value = match default {
        None => lookup(env, name).unwrap_or("").to_string(),
        Some(text) => match selected {
            Some(value) => value.to_string(),
            None => decode_default(text),
        },
    };

    Ok((value, close + 1))
}

fn main() {}