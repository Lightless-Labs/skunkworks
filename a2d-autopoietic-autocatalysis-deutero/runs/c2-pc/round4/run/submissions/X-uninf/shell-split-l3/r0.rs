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

fn append_scalar(word: &mut Word, c: char, active_glob: bool) {
    if active_glob && matches!(c, '*' | '?' | '[') {
        word.glob.push(word.text.len());
    }
    word.text.push(c);
}

fn append_string(word: &mut Word, value: &str, active_glob: bool) {
    for c in value.chars() {
        append_scalar(word, c, active_glob);
    }
}

fn lookup<'a>(env: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| (*key == name).then_some(*value))
}

fn next_char(input: &str, pos: usize) -> Option<(char, usize)> {
    input[pos..].chars().next().map(|c| (c, pos + c.len_utf8()))
}

fn decode_default(text: &str) -> String {
    let mut out = String::new();
    let mut pos = 0;
    while let Some((c, next)) = next_char(text, pos) {
        if c == '\\' {
            if let Some((escaped, after)) = next_char(text, next) {
                out.push(escaped);
                pos = after;
            } else {
                out.push('\\');
                pos = next;
            }
        } else {
            out.push(c);
            pos = next;
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
    let mut pos = after_open;
    let mut close = None;

    while let Some((c, next)) = next_char(input, pos) {
        if c == '\\' {
            if let Some((_, after_escaped)) = next_char(input, next) {
                pos = after_escaped;
            } else {
                pos = next;
            }
        } else if c == '}' {
            close = Some(pos);
            break;
        } else {
            pos = next;
        }
    }

    let close = close.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
    let contents = &input[after_open..close];

    let name_end = contents
        .char_indices()
        .find_map(|(i, c)| (!is_name_continue(c)).then_some(i))
        .unwrap_or(contents.len());

    if name_end == 0 {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }

    let name = &contents[..name_end];
    if !name.chars().next().is_some_and(is_name_start) {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }

    let rest = &contents[name_end..];
    let value = if rest.is_empty() {
        lookup(env, name).unwrap_or("").to_string()
    } else if let Some(default_text) = rest.strip_prefix(":-") {
        match lookup(env, name) {
            Some(value) if !value.is_empty() => value.to_string(),
            _ => decode_default(default_text),
        }
    } else {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    };

    Ok((value, close + 1))
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
    let mut quote = Quote::None;
    let mut quote_open = 0usize;
    let mut pos = 0usize;

    while let Some((c, next)) = next_char(input, pos) {
        match quote {
            Quote::Single => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    append_scalar(&mut word, c, false);
                }
                pos = next;
                continue;
            }
            Quote::Double => {
                if c == '"' {
                    quote = Quote::None;
                    pos = next;
                    continue;
                }
                if c == '\\' {
                    if let Some((escaped, after)) = next_char(input, next) {
                        match escaped {
                            '"' | '\\' | '$' => {
                                append_scalar(&mut word, escaped, false);
                                pos = after;
                            }
                            '\n' => pos = after,
                            _ => {
                                append_scalar(&mut word, '\\', false);
                                pos = next;
                            }
                        }
                    } else {
                        return Err(SplitError::TrailingEscape { pos });
                    }
                    continue;
                }
                if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos });
                }
                if c == '$' {
                    if let Some((following, _)) = next_char(input, next) {
                        if following == '(' {
                            return Err(SplitError::CommandSubstitution { pos });
                        }
                        if following == '{' {
                            let (value, after) = parse_expansion(input, pos, env)?;
                            active = true;
                            append_string(&mut word, &value, false);
                            pos = after;
                            continue;
                        }
                        if is_name_start(following) {
                            let mut end = next;
                            while let Some((name_char, after)) = next_char(input, end) {
                                if is_name_continue(name_char) {
                                    end = after;
                                } else {
                                    break;
                                }
                            }
                            let value = lookup(env, &input[next..end]).unwrap_or("");
                            active = true;
                            append_string(&mut word, value, false);
                            pos = end;
                            continue;
                        }
                    }
                    append_scalar(&mut word, '$', false);
                    pos = next;
                    continue;
                }
                append_scalar(&mut word, c, false);
                pos = next;
                continue;
            }
            Quote::None => {}
        }

        match c {
            ' ' | '\t' | '\r' | '\n' => {
                if active {
                    words.push(word);
                    word = Word {
                        text: String::new(),
                        glob: Vec::new(),
                    };
                    active = false;
                }
                pos = next;
            }
            '\'' => {
                active = true;
                quote = Quote::Single;
                quote_open = pos;
                pos = next;
            }
            '"' => {
                active = true;
                quote = Quote::Double;
                quote_open = pos;
                pos = next;
            }
            '\\' => {
                if let Some((escaped, after)) = next_char(input, next) {
                    if escaped == '\n' {
                        pos = after;
                    } else {
                        active = true;
                        append_scalar(&mut word, escaped, false);
                        pos = after;
                    }
                } else {
                    return Err(SplitError::TrailingEscape { pos });
                }
            }
            '`' => return Err(SplitError::CommandSubstitution { pos }),
            '$' => {
                if let Some((following, _)) = next_char(input, next) {
                    if following == '(' {
                        return Err(SplitError::CommandSubstitution { pos });
                    }
                    if following == '{' {
                        let (value, after) = parse_expansion(input, pos, env)?;
                        active = true;
                        append_string(&mut word, &value, true);
                        pos = after;
                        continue;
                    }
                    if is_name_start(following) {
                        let mut end = next;
                        while let Some((name_char, after)) = next_char(input, end) {
                            if is_name_continue(name_char) {
                                end = after;
                            } else {
                                break;
                            }
                        }
                        let value = lookup(env, &input[next..end]).unwrap_or("");
                        active = true;
                        append_string(&mut word, value, true);
                        pos = end;
                        continue;
                    }
                }
                active = true;
                append_scalar(&mut word, '$', true);
                pos = next;
            }
            _ => {
                active = true;
                append_scalar(&mut word, c, true);
                pos = next;
            }
        }
    }

    match quote {
        Quote::Single => return Err(SplitError::UnterminatedSingle { pos: quote_open }),
        Quote::Double => return Err(SplitError::UnterminatedDouble { pos: quote_open }),
        Quote::None => {}
    }

    if active {
        words.push(word);
    }
    Ok(words)
}