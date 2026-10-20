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

fn append_char(word: &mut Word, c: char, wildcard_active: bool) {
    if wildcard_active && matches!(c, '*' | '?' | '[') {
        word.glob.push(word.text.len());
    }
    word.text.push(c);
}

fn append_str(word: &mut Word, value: &str, wildcard_active: bool) {
    for c in value.chars() {
        append_char(word, c, wildcard_active);
    }
}

fn lookup<'a>(env: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| (*key == name).then_some(*value))
}

fn parse_braced(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
) -> Result<(usize, String), SplitError> {
    let mut i = dollar + 2;
    let mut close = None;

    while i < input.len() {
        let c = input[i..].chars().next().unwrap();
        if c == '\\' {
            i += 1;
            if i < input.len() {
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
    let contents = &input[dollar + 2..close];

    let name_end = contents
        .char_indices()
        .take_while(|(_, c)| is_name_continue(*c))
        .map(|(idx, c)| idx + c.len_utf8())
        .last()
        .unwrap_or(0);

    if name_end == 0 || !is_name_start(contents.chars().next().unwrap()) {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }

    let name = &contents[..name_end];
    let tail = &contents[name_end..];

    let value = if tail.is_empty() {
        lookup(env, name).unwrap_or("").to_string()
    } else if let Some(default) = tail.strip_prefix(":-") {
        match lookup(env, name) {
            Some(value) if !value.is_empty() => value.to_string(),
            _ => {
                let mut decoded = String::new();
                let mut chars = default.chars();
                while let Some(c) = chars.next() {
                    if c == '\\' {
                        if let Some(next) = chars.next() {
                            decoded.push(next);
                        } else {
                            decoded.push('\\');
                        }
                    } else {
                        decoded.push(c);
                    }
                }
                decoded
            }
        }
    } else {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    };

    Ok((close + 1, value))
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<Word>, SplitError> {
    if let Some(pos) = input.find('\0') {
        return Err(SplitError::Nul { pos });
    }

    let mut words = Vec::new();
    let mut current = Word {
        text: String::new(),
        glob: Vec::new(),
    };
    let mut active = false;
    let mut quote = Quote::None;
    let mut quote_start = 0;
    let mut i = 0;

    while i < input.len() {
        let c = input[i..].chars().next().unwrap();
        let next_i = i + c.len_utf8();

        match quote {
            Quote::Single => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    current.text.push(c);
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
                    if matches!(next, '"' | '\\' | '$' | '\n') {
                        if next != '\n' {
                            append_char(&mut current, next, false);
                        }
                        i = next_i + next.len_utf8();
                    } else {
                        current.text.push('\\');
                        i = next_i;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if input[next_i..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }
                    if input[next_i..].starts_with('{') {
                        let (end, value) = parse_braced(input, i, env)?;
                        append_str(&mut current, &value, false);
                        active = true;
                        i = end;
                    } else if next_i < input.len() {
                        let next = input[next_i..].chars().next().unwrap();
                        if is_name_start(next) {
                            let mut end = next_i + next.len_utf8();
                            while end < input.len() {
                                let ch = input[end..].chars().next().unwrap();
                                if !is_name_continue(ch) {
                                    break;
                                }
                                end += ch.len_utf8();
                            }
                            let value = lookup(env, &input[next_i..end]).unwrap_or("");
                            append_str(&mut current, value, false);
                            active = true;
                            i = end;
                        } else {
                            current.text.push('$');
                            i = next_i;
                        }
                    } else {
                        current.text.push('$');
                        i = next_i;
                    }
                } else {
                    current.text.push(c);
                    i = next_i;
                }
            }
            Quote::None => {
                if is_separator(c) {
                    if active {
                        words.push(current);
                        current = Word {
                            text: String::new(),
                            glob: Vec::new(),
                        };
                        active = false;
                    }
                    i = next_i;
                } else if c == '\'' {
                    quote = Quote::Single;
                    quote_start = i;
                    active = true;
                    i = next_i;
                } else if c == '"' {
                    quote = Quote::Double;
                    quote_start = i;
                    active = true;
                    i = next_i;
                } else if c == '\\' {
                    if next_i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let next = input[next_i..].chars().next().unwrap();
                    if next != '\n' {
                        append_char(&mut current, next, false);
                        active = true;
                    }
                    i = next_i + next.len_utf8();
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if input[next_i..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }
                    if input[next_i..].starts_with('{') {
                        let (end, value) = parse_braced(input, i, env)?;
                        append_str(&mut current, &value, true);
                        active = true;
                        i = end;
                    } else if next_i < input.len() {
                        let next = input[next_i..].chars().next().unwrap();
                        if is_name_start(next) {
                            let mut end = next_i + next.len_utf8();
                            while end < input.len() {
                                let ch = input[end..].chars().next().unwrap();
                                if !is_name_continue(ch) {
                                    break;
                                }
                                end += ch.len_utf8();
                            }
                            let value = lookup(env, &input[next_i..end]).unwrap_or("");
                            append_str(&mut current, value, true);
                            active = true;
                            i = end;
                        } else {
                            current.text.push('$');
                            active = true;
                            i = next_i;
                        }
                    } else {
                        current.text.push('$');
                        active = true;
                        i = next_i;
                    }
                } else {
                    append_char(&mut current, c, true);
                    active = true;
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
        words.push(current);
    }
    Ok(words)
}