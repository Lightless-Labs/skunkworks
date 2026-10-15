#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitError {
    Nul { pos: usize },
    TrailingEscape { pos: usize },
    UnterminatedSingle { pos: usize },
    UnterminatedDouble { pos: usize },
    UnterminatedExpansion { pos: usize },
    InvalidExpansion { pos: usize },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Quote {
    None,
    Single(usize),
    Double(usize),
}

fn is_separator(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\r' | '\n')
}

fn is_name_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_name_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn lookup<'a>(name: &str, env: &'a [(&str, &str)]) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| (*key == name).then_some(*value))
}

fn decode_default(text: &str) -> String {
    let mut result = String::new();
    let mut chars = text.char_indices().peekable();

    while let Some((_, ch)) = chars.next() {
        if ch == '\\' {
            if let Some((_, next)) = chars.next() {
                result.push(next);
            } else {
                result.push('\\');
            }
        } else {
            result.push(ch);
        }
    }

    result
}

/// Splits input into words according to the specified shell-like level-2 rules.
pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<String>, SplitError> {
    if let Some(pos) = input.find('\0') {
        return Err(SplitError::Nul { pos });
    }

    let mut words = Vec::new();
    let mut word = String::new();
    let mut active = false;
    let mut quote = Quote::None;
    let mut i = 0;

    while i < input.len() {
        let ch = input[i..].chars().next().expect("valid character boundary");
        let next_i = i + ch.len_utf8();

        match quote {
            Quote::Single(open_pos) => {
                if ch == '\'' {
                    quote = Quote::None;
                } else {
                    word.push(ch);
                }
                let _ = open_pos;
                i = next_i;
            }
            Quote::Double(open_pos) => {
                match ch {
                    '"' => {
                        quote = Quote::None;
                        i = next_i;
                    }
                    '\\' => {
                        if next_i == input.len() {
                            return Err(SplitError::TrailingEscape { pos: i });
                        }
                        let escaped = input[next_i..]
                            .chars()
                            .next()
                            .expect("character follows backslash");
                        let after_escaped = next_i + escaped.len_utf8();
                        if matches!(escaped, '"' | '\\' | '$') {
                            word.push(escaped);
                            i = after_escaped;
                        } else if escaped == '\n' {
                            i = after_escaped;
                        } else {
                            word.push('\\');
                            i = next_i;
                        }
                    }
                    '$' => {
                        let (end, value) = parse_expansion(input, i, env)?;
                        active = true;
                        word.push_str(&value);
                        i = end;
                    }
                    _ => {
                        word.push(ch);
                        i = next_i;
                    }
                }
                let _ = open_pos;
            }
            Quote::None => {
                match ch {
                    ' ' | '\t' | '\r' | '\n' => {
                        if active {
                            words.push(std::mem::take(&mut word));
                            active = false;
                        }
                        i = next_i;
                    }
                    '\'' => {
                        active = true;
                        quote = Quote::Single(i);
                        i = next_i;
                    }
                    '"' => {
                        active = true;
                        quote = Quote::Double(i);
                        i = next_i;
                    }
                    '\\' => {
                        if next_i == input.len() {
                            return Err(SplitError::TrailingEscape { pos: i });
                        }
                        let escaped = input[next_i..]
                            .chars()
                            .next()
                            .expect("character follows backslash");
                        let after_escaped = next_i + escaped.len_utf8();
                        if escaped != '\n' {
                            active = true;
                            word.push(escaped);
                        }
                        i = after_escaped;
                    }
                    '$' => {
                        let (end, value) = parse_expansion(input, i, env)?;
                        active = true;
                        word.push_str(&value);
                        i = end;
                    }
                    _ => {
                        active = true;
                        word.push(ch);
                        i = next_i;
                    }
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

fn parse_expansion(
    input: &str,
    dollar_pos: usize,
    env: &[(&str, &str)],
) -> Result<(usize, String), SplitError> {
    let after_dollar = dollar_pos + 1;
    if after_dollar >= input.len() {
        return Ok((after_dollar, "$".to_string()));
    }

    let first = input[after_dollar..]
        .chars()
        .next()
        .expect("character follows dollar");

    if first == '{' {
        let content_start = after_dollar + 1;
        let mut cursor = content_start;
        let close_pos;

        loop {
            if cursor >= input.len() {
                return Err(SplitError::UnterminatedExpansion { pos: dollar_pos });
            }
            let ch = input[cursor..]
                .chars()
                .next()
                .expect("valid character boundary");
            let ch_end = cursor + ch.len_utf8();

            if ch == '\\' {
                if ch_end >= input.len() {
                    return Err(SplitError::UnterminatedExpansion { pos: dollar_pos });
                }
                let escaped = input[ch_end..]
                    .chars()
                    .next()
                    .expect("character follows backslash");
                cursor = ch_end + escaped.len_utf8();
            } else if ch == '}' {
                close_pos = cursor;
                break;
            } else {
                cursor = ch_end;
            }
        }

        let content = &input[content_start..close_pos];
        let name_end = content
            .char_indices()
            .find_map(|(offset, ch)| (!is_name_continue(ch)).then_some(offset))
            .unwrap_or(content.len());

        if name_end == 0 {
            return Err(SplitError::InvalidExpansion { pos: dollar_pos });
        }
        let name = &content[..name_end];
        let mut name_chars = name.chars();
        if !name_chars.next().is_some_and(is_name_start)
            || !name_chars.all(is_name_continue)
        {
            return Err(SplitError::InvalidExpansion { pos: dollar_pos });
        }

        let remainder = &content[name_end..];
        let selected = if remainder.is_empty() {
            lookup(name, env).unwrap_or("").to_string()
        } else if let Some(default_text) = remainder.strip_prefix(":-") {
            match lookup(name, env) {
                Some(value) if !value.is_empty() => value.to_string(),
                _ => decode_default(default_text),
            }
        } else {
            return Err(SplitError::InvalidExpansion { pos: dollar_pos });
        };

        return Ok((close_pos + 1, selected));
    }

    if !is_name_start(first) {
        return Ok((after_dollar, "$".to_string()));
    }

    let mut end = after_dollar + first.len_utf8();
    while end < input.len() {
        let ch = input[end..]
            .chars()
            .next()
            .expect("valid character boundary");
        if !is_name_continue(ch) {
            break;
        }
        end += ch.len_utf8();
    }

    let name = &input[after_dollar..end];
    Ok((end, lookup(name, env).unwrap_or("").to_string()))
}

fn main() {}