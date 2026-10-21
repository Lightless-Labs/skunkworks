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

fn append_scalar(word: &mut Word, c: char, wildcard_active: bool) {
    if wildcard_active && matches!(c, '*' | '?' | '[') {
        word.glob.push(word.text.len());
    }
    word.text.push(c);
}

fn append_string(word: &mut Word, value: &str, wildcard_active: bool) {
    for c in value.chars() {
        append_scalar(word, c, wildcard_active);
    }
}

fn decode_default(text: &str) -> String {
    let mut result = String::new();
    let mut chars = text.char_indices().peekable();
    while let Some((_, c)) = chars.next() {
        if c == '\\' {
            if let Some((_, next)) = chars.next() {
                result.push(next);
            }
        } else {
            result.push(c);
        }
    }
    result
}

fn lookup<'a>(name: &str, env: &'a [(&str, &str)]) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| if *key == name { Some(*value) } else { None })
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<Word>, SplitError> {
    if let Some(pos) = input.as_bytes().iter().position(|&b| b == 0) {
        return Err(SplitError::Nul { pos });
    }

    let mut words = Vec::new();
    let mut word = Word {
        text: String::new(),
        glob: Vec::new(),
    };
    let mut active = false;
    let mut quote: Option<Quote> = None;
    let mut i = 0;

    while i < input.len() {
        let c = input[i..].chars().next().unwrap();
        let next_i = i + c.len_utf8();

        match quote {
            Some(Quote::Single(_)) => {
                if c == '\'' {
                    quote = None;
                } else {
                    append_scalar(&mut word, c, false);
                }
                i = next_i;
            }
            Some(Quote::Double(open_pos)) => {
                if c == '"' {
                    quote = None;
                    i = next_i;
                } else if c == '\\' {
                    if next_i >= input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let next = input[next_i..].chars().next().unwrap();
                    let after_next = next_i + next.len_utf8();
                    if matches!(next, '"' | '\\' | '$' | '\n') {
                        if next != '\n' {
                            append_scalar(&mut word, next, false);
                        }
                        i = after_next;
                    } else {
                        append_scalar(&mut word, '\\', false);
                        i = next_i;
                    }
                } else if c == '$' {
                    let dollar = i;
                    if next_i < input.len() && input[next_i..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: dollar });
                    }
                    if next_i < input.len() && input[next_i..].starts_with('{') {
                        let (end, value) = parse_braced(input, dollar, env)?;
                        append_string(&mut word, &value, false);
                        active = true;
                        i = end;
                    } else if next_i < input.len() {
                        let first = input[next_i..].chars().next().unwrap();
                        if is_name_start(first) {
                            let mut end = next_i + first.len_utf8();
                            while end < input.len() {
                                let ch = input[end..].chars().next().unwrap();
                                if is_name_continue(ch) {
                                    end += ch.len_utf8();
                                } else {
                                    break;
                                }
                            }
                            let value = lookup(&input[next_i..end], env).unwrap_or("");
                            append_string(&mut word, value, false);
                            active = true;
                            i = end;
                        } else {
                            append_scalar(&mut word, '$', false);
                            i = next_i;
                        }
                    } else {
                        append_scalar(&mut word, '$', false);
                        i = next_i;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else {
                    append_scalar(&mut word, c, false);
                    i = next_i;
                }
                let _ = open_pos;
            }
            None => {
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
                    quote = Some(Quote::Single(i));
                    active = true;
                    i = next_i;
                } else if c == '"' {
                    quote = Some(Quote::Double(i));
                    active = true;
                    i = next_i;
                } else if c == '\\' {
                    if next_i >= input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let next = input[next_i..].chars().next().unwrap();
                    let after_next = next_i + next.len_utf8();
                    if next == '\n' {
                        i = after_next;
                    } else {
                        append_scalar(&mut word, next, false);
                        active = true;
                        i = after_next;
                    }
                } else if c == '$' {
                    let dollar = i;
                    if next_i < input.len() && input[next_i..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: dollar });
                    }
                    if next_i < input.len() && input[next_i..].starts_with('{') {
                        let (end, value) = parse_braced(input, dollar, env)?;
                        append_string(&mut word, &value, true);
                        active = true;
                        i = end;
                    } else if next_i < input.len() {
                        let first = input[next_i..].chars().next().unwrap();
                        if is_name_start(first) {
                            let mut end = next_i + first.len_utf8();
                            while end < input.len() {
                                let ch = input[end..].chars().next().unwrap();
                                if is_name_continue(ch) {
                                    end += ch.len_utf8();
                                } else {
                                    break;
                                }
                            }
                            let value = lookup(&input[next_i..end], env).unwrap_or("");
                            append_string(&mut word, value, true);
                            active = true;
                            i = end;
                        } else {
                            append_scalar(&mut word, '$', false);
                            active = true;
                            i = next_i;
                        }
                    } else {
                        append_scalar(&mut word, '$', false);
                        active = true;
                        i = next_i;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else {
                    append_scalar(&mut word, c, matches!(c, '*' | '?' | '['));
                    active = true;
                    i = next_i;
                }
            }
        }
    }

    match quote {
        Some(Quote::Single(pos)) => return Err(SplitError::UnterminatedSingle { pos }),
        Some(Quote::Double(pos)) => return Err(SplitError::UnterminatedDouble { pos }),
        None => {}
    }

    if active {
        words.push(word);
    }
    Ok(words)
}

fn parse_braced(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
) -> Result<(usize, String), SplitError> {
    let contents_start = dollar + 2;
    let mut close = None;
    let mut i = contents_start;

    while i < input.len() {
        let c = input[i..].chars().next().unwrap();
        let next_i = i + c.len_utf8();
        if c == '\\' {
            if next_i < input.len() {
                let escaped = input[next_i..].chars().next().unwrap();
                i = next_i + escaped.len_utf8();
            } else {
                i = next_i;
            }
        } else if c == '}' {
            close = Some(i);
            break;
        } else {
            i = next_i;
        }
    }

    let close = close.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
    let contents = &input[contents_start..close];

    let name_end = contents
        .char_indices()
        .find_map(|(offset, c)| {
            if (offset == 0 && !is_name_start(c))
                || (offset > 0 && !is_name_continue(c))
            {
                Some(offset)
            } else {
                None
            }
        })
        .unwrap_or(contents.len());

    if name_end == 0 {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }
    let name = &contents[..name_end];
    if !name.chars().next().map(is_name_start).unwrap_or(false)
        || !name.chars().all(is_name_continue)
    {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }

    let value = if name_end == contents.len() {
        lookup(name, env).unwrap_or("").to_string()
    } else if contents[name_end..].starts_with(":-") {
        let default = &contents[name_end + 2..];
        match lookup(name, env) {
            Some(value) if !value.is_empty() => value.to_string(),
            _ => decode_default(default),
        }
    } else {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    };

    Ok((close + 1, value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_basic_words() {
        assert_eq!(
            split("alpha 'two words' x\"y\" ''", &[]),
            Ok(vec![
                Word { text: "alpha".to_string(), glob: vec![] },
                Word { text: "two words".to_string(), glob: vec![] },
                Word { text: "xy".to_string(), glob: vec![] },
                Word { text: "".to_string(), glob: vec![] }
            ])
        );
    }

    #[test]
    fn public_escapes() {
        assert_eq!(
            split("a\\ b \"c\\\"d\"", &[]),
            Ok(vec![
                Word { text: "a b".to_string(), glob: vec![] },
                Word { text: "c\"d".to_string(), glob: vec![] }
            ])
        );
    }

    #[test]
    fn public_globs_and_environment() {
        assert_eq!(
            split("a* \"?\" $X", &[("X", "hi")]),
            Ok(vec![
                Word { text: "a*".to_string(), glob: vec![1] },
                Word { text: "?".to_string(), glob: vec![] },
                Word { text: "hi".to_string(), glob: vec![] }
            ])
        );
    }

    #[test]
    fn public_command_rejected() {
        assert_eq!(
            split("`date`", &[]),
            Err(SplitError::CommandSubstitution { pos: 0 })
        );
    }
}