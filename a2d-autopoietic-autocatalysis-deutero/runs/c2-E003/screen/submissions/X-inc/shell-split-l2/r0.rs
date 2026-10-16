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
    is_name_start(c) || c.is_ascii_digit()
}

fn lookup<'a>(name: &str, env: &'a [(&str, &str)]) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| (*key == name).then_some(*value))
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<String>, SplitError> {
    if let Some(pos) = input.as_bytes().iter().position(|&b| b == 0) {
        return Err(SplitError::Nul { pos });
    }

    let mut words = Vec::new();
    let mut word = String::new();
    let mut active = false;
    let mut quote = Quote::None;
    let mut i = 0;

    while i < input.len() {
        let c = input[i..].chars().next().expect("valid UTF-8 boundary");
        let next_i = i + c.len_utf8();

        match quote {
            Quote::Single { .. } => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    word.push(c);
                }
                i = next_i;
            }
            Quote::Double { .. } => {
                match c {
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
                            .expect("nonempty suffix");
                        let after_escaped = next_i + escaped.len_utf8();
                        if matches!(escaped, '"' | '\\' | '$') {
                            word.push(escaped);
                            active = true;
                            i = after_escaped;
                        } else if escaped == '\n' {
                            i = after_escaped;
                        } else {
                            word.push('\\');
                            active = true;
                            i = next_i;
                        }
                    }
                    '$' => {
                        active = true;
                        let (end, value) = parse_expansion(input, i, env)?;
                        if let Some(value) = value {
                            word.push_str(value);
                        } else {
                            word.push('$');
                        }
                        i = end;
                    }
                    _ => {
                        word.push(c);
                        active = true;
                        i = next_i;
                    }
                }
            }
            Quote::None => {
                match c {
                    '\'' => {
                        quote = Quote::Single { opening: i };
                        active = true;
                        i = next_i;
                    }
                    '"' => {
                        quote = Quote::Double { opening: i };
                        active = true;
                        i = next_i;
                    }
                    '\\' => {
                        if next_i == input.len() {
                            return Err(SplitError::TrailingEscape { pos: i });
                        }
                        let escaped = input[next_i..]
                            .chars()
                            .next()
                            .expect("nonempty suffix");
                        let after_escaped = next_i + escaped.len_utf8();
                        if escaped == '\n' {
                            i = after_escaped;
                        } else {
                            word.push(escaped);
                            active = true;
                            i = after_escaped;
                        }
                    }
                    '$' => {
                        let (end, value) = parse_expansion(input, i, env)?;
                        if let Some(value) = value {
                            active = true;
                            word.push_str(value);
                        } else {
                            word.push('$');
                            active = true;
                        }
                        i = end;
                    }
                    c if is_separator(c) => {
                        if active {
                            words.push(std::mem::take(&mut word));
                            active = false;
                        }
                        i = next_i;
                    }
                    _ => {
                        word.push(c);
                        active = true;
                        i = next_i;
                    }
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

// Returns the end offset and Some(value) for an expansion, or None for a literal dollar.
fn parse_expansion<'a>(
    input: &str,
    dollar: usize,
    env: &'a [(&str, &str)],
) -> Result<(usize, Option<&'a str>), SplitError> {
    let after_dollar = dollar + 1;
    if after_dollar >= input.len() {
        return Ok((after_dollar, None));
    }

    let first = input[after_dollar..]
        .chars()
        .next()
        .expect("nonempty suffix");

    if first == '{' {
        let content_start = after_dollar + 1;
        let mut j = content_start;
        let mut close = None;

        while j < input.len() {
            let c = input[j..].chars().next().expect("valid UTF-8 boundary");
            let end = j + c.len_utf8();
            if c == '\\' {
                if end < input.len() {
                    let skipped = input[end..].chars().next().expect("nonempty suffix");
                    j = end + skipped.len_utf8();
                } else {
                    j = end;
                }
            } else if c == '}' {
                close = Some(j);
                break;
            } else {
                j = end;
            }
        }

        let Some(close_pos) = close else {
            return Err(SplitError::UnterminatedExpansion { pos: dollar });
        };

        let contents = &input[content_start..close_pos];
        let (name_end, name) = parse_braced_name(contents)
            .ok_or(SplitError::InvalidExpansion { pos: dollar })?;

        if name_end == contents.len() {
            let value = lookup(name, env).unwrap_or("");
            return Ok((close_pos + 1, Some(value)));
        }

        if contents[name_end..].starts_with(":-") {
            let default_start = name_end + 2;
            let value = lookup(name, env).filter(|v| !v.is_empty());
            if let Some(value) = value {
                return Ok((close_pos + 1, Some(value)));
            }
            let decoded = decode_default(&contents[default_start..]);
            // The decoded default is local storage, so it is appended by the caller below
            // through a special internal representation.
            return Ok((close_pos + 1, Some(Box::leak(decoded.into_boxed_str()))));
        }

        return Err(SplitError::InvalidExpansion { pos: dollar });
    }

    if is_name_start(first) {
        let mut end = after_dollar + first.len_utf8();
        while end < input.len() {
            let c = input[end..].chars().next().expect("valid UTF-8 boundary");
            if !is_name_continue(c) {
                break;
            }
            end += c.len_utf8();
        }
        let name = &input[after_dollar..end];
        return Ok((end, Some(lookup(name, env).unwrap_or(""))));
    }

    Ok((after_dollar, None))
}

fn parse_braced_name(contents: &str) -> Option<(usize, &str)> {
    let mut chars = contents.char_indices();
    let (_, first) = chars.next()?;
    if !is_name_start(first) {
        return None;
    }

    let mut end = first.len_utf8();
    for (offset, c) in chars {
        if !is_name_continue(c) {
            break;
        }
        end = offset + c.len_utf8();
    }

    Some((end, &contents[..end]))
}

fn decode_default(text: &str) -> String {
    let mut result = String::new();
    let mut i = 0;
    while i < text.len() {
        let c = text[i..].chars().next().expect("valid UTF-8 boundary");
        let next = i + c.len_utf8();
        if c == '\\' && next < text.len() {
            let escaped = text[next..].chars().next().expect("nonempty suffix");
            result.push(escaped);
            i = next + escaped.len_utf8();
        } else {
            result.push(c);
            i = next;
        }
    }
    result
}