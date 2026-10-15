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
    c.is_ascii_alphabetic() || c == '_'
}

fn is_name_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

fn lookup<'a>(name: &str, env: &'a [(&'a str, &'a str)]) -> Option<&'a str> {
    env.iter().find_map(|(key, value)| (*key == name).then_some(*value))
}

fn append_expansion(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
    word: &mut String,
) -> Result<usize, SplitError> {
    let after_dollar = dollar + 1;
    let rest = &input[after_dollar..];

    if rest.starts_with('{') {
        let contents_start = after_dollar + 1;
        let mut cursor = contents_start;
        let mut closing = None;

        while cursor < input.len() {
            let c = input[cursor..].chars().next().unwrap();
            if c == '\\' {
                cursor += c.len_utf8();
                if cursor < input.len() {
                    let next = input[cursor..].chars().next().unwrap();
                    cursor += next.len_utf8();
                }
            } else if c == '}' {
                closing = Some(cursor);
                break;
            } else {
                cursor += c.len_utf8();
            }
        }

        let close = closing.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
        let contents = &input[contents_start..close];

        let name_end = contents
            .char_indices()
            .find_map(|(i, c)| (!is_name_continue(c)).then_some(i))
            .unwrap_or(contents.len());

        if name_end == 0 {
            return Err(SplitError::InvalidExpansion { pos: dollar });
        }

        let name = &contents[..name_end];
        let mut name_chars = name.chars();
        if !name_chars.next().is_some_and(is_name_start)
            || !name_chars.all(is_name_continue)
        {
            return Err(SplitError::InvalidExpansion { pos: dollar });
        }

        let suffix = &contents[name_end..];
        if suffix.is_empty() {
            if let Some(value) = lookup(name, env) {
                word.push_str(value);
            }
        } else if let Some(default) = suffix.strip_prefix(":-") {
            if let Some(value) = lookup(name, env).filter(|value| !value.is_empty()) {
                word.push_str(value);
            } else {
                let mut chars = default.chars();
                while let Some(c) = chars.next() {
                    if c == '\\' {
                        if let Some(next) = chars.next() {
                            word.push(next);
                        } else {
                            word.push('\\');
                        }
                    } else {
                        word.push(c);
                    }
                }
            }
        } else {
            return Err(SplitError::InvalidExpansion { pos: dollar });
        }

        Ok(close + 1)
    } else {
        let mut chars = rest.char_indices();
        let Some((_, first)) = chars.next() else {
            word.push('$');
            return Ok(after_dollar);
        };

        if !is_name_start(first) {
            word.push('$');
            return Ok(after_dollar);
        }

        let mut end = after_dollar + first.len_utf8();
        for (relative, c) in chars {
            if !is_name_continue(c) {
                break;
            }
            end = after_dollar + relative + c.len_utf8();
        }
        let name = &input[after_dollar..end];
        if let Some(value) = lookup(name, env) {
            word.push_str(value);
        }
        Ok(end)
    }
}

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
        let c = input[i..].chars().next().unwrap();
        let next_i = i + c.len_utf8();

        match quote {
            Quote::Single(_) => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    word.push(c);
                }
                i = next_i;
            }
            Quote::Double(open) => {
                if c == '"' {
                    quote = Quote::None;
                    i = next_i;
                } else if c == '\\' {
                    if next_i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let next = input[next_i..].chars().next().unwrap();
                    let after_next = next_i + next.len_utf8();
                    if matches!(next, '"' | '\\' | '$') {
                        word.push(next);
                        active = true;
                        i = after_next;
                    } else if next == '\n' {
                        active = true;
                        i = after_next;
                    } else {
                        word.push('\\');
                        active = true;
                        i = next_i;
                    }
                } else if c == '$' {
                    active = true;
                    i = append_expansion(input, i, env, &mut word)?;
                } else {
                    word.push(c);
                    active = true;
                    i = next_i;
                }
                let _ = open;
            }
            Quote::None => {
                if is_separator(c) {
                    if active {
                        words.push(std::mem::take(&mut word));
                        active = false;
                    }
                    i = next_i;
                } else if c == '\'' {
                    active = true;
                    quote = Quote::Single(i);
                    i = next_i;
                } else if c == '"' {
                    active = true;
                    quote = Quote::Double(i);
                    i = next_i;
                } else if c == '\\' {
                    if next_i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let next = input[next_i..].chars().next().unwrap();
                    let after_next = next_i + next.len_utf8();
                    if next != '\n' {
                        word.push(next);
                        active = true;
                    }
                    i = after_next;
                } else if c == '$' {
                    active = true;
                    i = append_expansion(input, i, env, &mut word)?;
                } else {
                    word.push(c);
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