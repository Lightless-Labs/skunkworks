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
    Single { opening: usize },
    Double { opening: usize },
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

fn parse_expansion(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
) -> Result<(String, usize), SplitError> {
    let after_open = dollar + 2;
    let mut close = None;
    let mut cursor = after_open;

    while cursor < input.len() {
        let c = input[cursor..].chars().next().expect("valid boundary");
        if c == '\\' {
            cursor += c.len_utf8();
            if cursor < input.len() {
                let next = input[cursor..].chars().next().expect("valid boundary");
                cursor += next.len_utf8();
            }
        } else if c == '}' {
            close = Some(cursor);
            break;
        } else {
            cursor += c.len_utf8();
        }
    }

    let close = close.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
    let contents = &input[after_open..close];

    let mut name_end = 0;
    let mut chars = contents.char_indices();
    match chars.next() {
        Some((_, first)) if is_name_start(first) => {
            name_end = first.len_utf8();
            for (idx, c) in chars {
                if !is_name_continue(c) {
                    break;
                }
                name_end = idx + c.len_utf8();
            }
        }
        _ => return Err(SplitError::InvalidExpansion { pos: dollar }),
    }

    let name = &contents[..name_end];
    let rest = &contents[name_end..];
    let value = if rest.is_empty() {
        lookup(name, env).unwrap_or("").to_owned()
    } else if let Some(default_text) = rest.strip_prefix(":-") {
        match lookup(name, env) {
            Some(value) if !value.is_empty() => value.to_owned(),
            _ => decode_default(default_text),
        }
    } else {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    };

    Ok((value, close + 1))
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<String>, SplitError> {
    if let Some(pos) = input.find('\0') {
        return Err(SplitError::Nul { pos });
    }

    let mut words = Vec::new();
    let mut word = String::new();
    let mut active = false;
    let mut quote = Quote::None;
    let mut cursor = 0;

    while cursor < input.len() {
        let c = input[cursor..].chars().next().expect("valid boundary");
        let next_cursor = cursor + c.len_utf8();

        match quote {
            Quote::Single { .. } => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    word.push(c);
                }
                cursor = next_cursor;
            }
            Quote::Double { .. } => {
                if c == '"' {
                    quote = Quote::None;
                    cursor = next_cursor;
                } else if c == '\\' {
                    if next_cursor == input.len() {
                        return Err(SplitError::TrailingEscape { pos: cursor });
                    }
                    let next = input[next_cursor..].chars().next().expect("valid boundary");
                    let after_next = next_cursor + next.len_utf8();
                    if matches!(next, '"' | '\\' | '$') {
                        word.push(next);
                        cursor = after_next;
                    } else if next == '\n' {
                        cursor = after_next;
                    } else {
                        word.push('\\');
                        cursor = next_cursor;
                    }
                } else if c == '$' {
                    if input[next_cursor..].starts_with('{') {
                        let (value, after) = parse_expansion(input, cursor, env)?;
                        word.push_str(&value);
                        active = true;
                        cursor = after;
                    } else if let Some((name, after)) = {
                        let mut iter = input[next_cursor..].char_indices();
                        match iter.next() {
                            Some((_, first)) if is_name_start(first) => {
                                let mut end = first.len_utf8();
                                for (idx, ch) in iter {
                                    if !is_name_continue(ch) {
                                        break;
                                    }
                                    end = idx + ch.len_utf8();
                                }
                                Some((&input[next_cursor..next_cursor + end], next_cursor + end))
                            }
                            _ => None,
                        }
                    } {
                        word.push_str(lookup(name, env).unwrap_or(""));
                        active = true;
                        cursor = after;
                    } else {
                        word.push('$');
                        cursor = next_cursor;
                    }
                } else {
                    word.push(c);
                    cursor = next_cursor;
                }
            }
            Quote::None => {
                if is_separator(c) {
                    if active {
                        words.push(std::mem::take(&mut word));
                        active = false;
                    }
                    cursor = next_cursor;
                } else if c == '\'' {
                    quote = Quote::Single { opening: cursor };
                    active = true;
                    cursor = next_cursor;
                } else if c == '"' {
                    quote = Quote::Double { opening: cursor };
                    active = true;
                    cursor = next_cursor;
                } else if c == '\\' {
                    if next_cursor == input.len() {
                        return Err(SplitError::TrailingEscape { pos: cursor });
                    }
                    let next = input[next_cursor..].chars().next().expect("valid boundary");
                    if next == '\n' {
                        cursor = next_cursor + next.len_utf8();
                    } else {
                        word.push(next);
                        active = true;
                        cursor = next_cursor + next.len_utf8();
                    }
                } else if c == '$' {
                    if input[next_cursor..].starts_with('{') {
                        let (value, after) = parse_expansion(input, cursor, env)?;
                        word.push_str(&value);
                        active = true;
                        cursor = after;
                    } else if let Some((name, after)) = {
                        let mut iter = input[next_cursor..].char_indices();
                        match iter.next() {
                            Some((_, first)) if is_name_start(first) => {
                                let mut end = first.len_utf8();
                                for (idx, ch) in iter {
                                    if !is_name_continue(ch) {
                                        break;
                                    }
                                    end = idx + ch.len_utf8();
                                }
                                Some((&input[next_cursor..next_cursor + end], next_cursor + end))
                            }
                            _ => None,
                        }
                    } {
                        word.push_str(lookup(name, env).unwrap_or(""));
                        active = true;
                        cursor = after;
                    } else {
                        word.push('$');
                        active = true;
                        cursor = next_cursor;
                    }
                } else {
                    word.push(c);
                    active = true;
                    cursor = next_cursor;
                }
            }
        }
    }

    match quote {
        Quote::Single { opening } => {
            return Err(SplitError::UnterminatedSingle { pos: opening });
        }
        Quote::Double { opening } => {
            return Err(SplitError::UnterminatedDouble { pos: opening });
        }
        Quote::None => {}
    }

    if active {
        words.push(word);
    }

    Ok(words)
}

fn main() {}