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
    c == '_' || c.is_ascii_alphabetic()
}

fn is_name_continue(c: char) -> bool {
    is_name_start(c) || c.is_ascii_digit()
}

fn append_char(word: &mut Word, c: char, wildcard_active: bool) {
    if wildcard_active && matches!(c, '*' | '?' | '[') {
        word.glob.push(word.text.len());
    }
    word.text.push(c);
}

fn append_string(word: &mut Word, text: &str, wildcard_active: bool) {
    for c in text.chars() {
        append_char(word, c, wildcard_active);
    }
}

fn env_value<'a>(env: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| (*key == name).then_some(*value))
}

fn decode_default(text: &str) -> String {
    let mut result = String::new();
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                result.push(next);
            }
        } else {
            result.push(c);
        }
    }

    result
}

fn parse_braced(
    input: &str,
    dollar_pos: usize,
    env: &[(&str, &str)],
) -> Result<(String, usize), SplitError> {
    let after_open = dollar_pos + 2;
    let mut i = after_open;
    let mut close = None;

    while i < input.len() {
        let c = input[i..].chars().next().unwrap();
        if c == '\\' {
            i += 1;
            if i < input.len() {
                let next = input[i..].chars().next().unwrap();
                i += next.len_utf8();
            }
        } else if c == '}' {
            close = Some(i);
            break;
        } else {
            i += c.len_utf8();
        }
    }

    let close_pos = close.ok_or(SplitError::UnterminatedExpansion { pos: dollar_pos })?;
    let contents = &input[after_open..close_pos];

    let name_end = contents
        .char_indices()
        .find_map(|(idx, c)| (!is_name_continue(c)).then_some(idx))
        .unwrap_or(contents.len());

    if name_end == 0 {
        return Err(SplitError::InvalidExpansion { pos: dollar_pos });
    }

    let name = &contents[..name_end];
    if !name.chars().next().is_some_and(is_name_start) {
        return Err(SplitError::InvalidExpansion { pos: dollar_pos });
    }

    let selected = if name_end == contents.len() {
        env_value(env, name).unwrap_or("").to_string()
    } else {
        let rest = &contents[name_end..];
        if let Some(default) = rest.strip_prefix(":-") {
            match env_value(env, name) {
                Some(value) if !value.is_empty() => value.to_string(),
                _ => decode_default(default),
            }
        } else {
            return Err(SplitError::InvalidExpansion { pos: dollar_pos });
        }
    };

    Ok((selected, close_pos + 1))
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
                    append_char(&mut word, c, false);
                }
                i = next_i;
            }
            Quote::Double => {
                if c == '"' {
                    quote = Quote::None;
                    i = next_i;
                } else if c == '\\' {
                    let slash_pos = i;
                    if next_i >= input.len() {
                        return Err(SplitError::TrailingEscape { pos: slash_pos });
                    }
                    let escaped = input[next_i..].chars().next().unwrap();
                    if matches!(escaped, '"' | '\\' | '$' | '\n') {
                        if escaped != '\n' {
                            append_char(&mut word, escaped, false);
                        }
                        i = next_i + escaped.len_utf8();
                    } else {
                        append_char(&mut word, '\\', false);
                        i = next_i;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if next_i < input.len() {
                        let following = input[next_i..].chars().next().unwrap();
                        if following == '(' {
                            return Err(SplitError::CommandSubstitution { pos: i });
                        }
                        if following == '{' {
                            let (value, end) = parse_braced(input, i, env)?;
                            append_string(&mut word, &value, false);
                            active = true;
                            i = end;
                            continue;
                        }
                        if is_name_start(following) {
                            let mut end = next_i;
                            while end < input.len() {
                                let ch = input[end..].chars().next().unwrap();
                                if !is_name_continue(ch) {
                                    break;
                                }
                                end += ch.len_utf8();
                            }
                            let name = &input[next_i..end];
                            append_string(&mut word, env_value(env, name).unwrap_or(""), false);
                            active = true;
                            i = end;
                            continue;
                        }
                    }
                    append_char(&mut word, '$', false);
                    i = next_i;
                } else {
                    append_char(&mut word, c, false);
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
                    quote = Quote::Single;
                    quote_pos = i;
                    active = true;
                    i = next_i;
                } else if c == '"' {
                    quote = Quote::Double;
                    quote_pos = i;
                    active = true;
                    i = next_i;
                } else if c == '\\' {
                    let slash_pos = i;
                    if next_i >= input.len() {
                        return Err(SplitError::TrailingEscape { pos: slash_pos });
                    }
                    let escaped = input[next_i..].chars().next().unwrap();
                    if escaped != '\n' {
                        append_char(&mut word, escaped, false);
                        active = true;
                    }
                    i = next_i + escaped.len_utf8();
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if next_i < input.len() {
                        let following = input[next_i..].chars().next().unwrap();
                        if following == '(' {
                            return Err(SplitError::CommandSubstitution { pos: i });
                        }
                        if following == '{' {
                            let (value, end) = parse_braced(input, i, env)?;
                            append_string(&mut word, &value, true);
                            active = true;
                            i = end;
                            continue;
                        }
                        if is_name_start(following) {
                            let mut end = next_i;
                            while end < input.len() {
                                let ch = input[end..].chars().next().unwrap();
                                if !is_name_continue(ch) {
                                    break;
                                }
                                end += ch.len_utf8();
                            }
                            let name = &input[next_i..end];
                            append_string(&mut word, env_value(env, name).unwrap_or(""), true);
                            active = true;
                            i = end;
                            continue;
                        }
                    }
                    append_char(&mut word, '$', true);
                    active = true;
                    i = next_i;
                } else {
                    append_char(&mut word, c, true);
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