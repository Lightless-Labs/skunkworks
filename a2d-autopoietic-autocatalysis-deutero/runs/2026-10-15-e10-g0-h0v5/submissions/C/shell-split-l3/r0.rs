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
enum Mode {
    Outside,
    Single(usize),
    Double(usize),
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_name_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn append_scalar(word: &mut Word, c: char, active_glob: bool) {
    let offset = word.text.len();
    if active_glob && matches!(c, '*' | '?' | '[') {
        word.glob.push(offset);
    }
    word.text.push(c);
}

fn append_str(word: &mut Word, value: &str, active_glob: bool) {
    for c in value.chars() {
        append_scalar(word, c, active_glob);
    }
}

fn lookup<'a>(name: &str, env: &'a [(&str, &str)]) -> Option<&'a str> {
    env.iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| *value)
}

fn decode_default(input: &str) -> String {
    let mut out = String::new();
    let mut iter = input.chars();
    while let Some(c) = iter.next() {
        if c == '\\' {
            if let Some(next) = iter.next() {
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

fn parse_braced(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
) -> Result<(usize, String), SplitError> {
    let bytes = input.as_bytes();
    let mut cursor = dollar + 2;

    let close = loop {
        if cursor >= bytes.len() {
            return Err(SplitError::UnterminatedExpansion { pos: dollar });
        }
        let c = input[cursor..].chars().next().unwrap();
        if c == '\\' {
            cursor += 1;
            if cursor < bytes.len() {
                let next = input[cursor..].chars().next().unwrap();
                cursor += next.len_utf8();
            }
        } else if c == '}' {
            break cursor;
        } else {
            cursor += c.len_utf8();
        }
    };

    let contents = &input[dollar + 2..close];
    let mut chars = contents.char_indices();
    let Some((_, first)) = chars.next() else {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    };
    if !is_name_start(first) {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }

    let mut name_end = first.len_utf8();
    for (idx, c) in chars {
        if is_name_continue(c) {
            name_end = idx + c.len_utf8();
        } else {
            break;
        }
    }

    let name = &contents[..name_end];
    let suffix = &contents[name_end..];
    let selected = if suffix.is_empty() {
        lookup(name, env).unwrap_or("").to_string()
    } else if let Some(default) = suffix.strip_prefix(":-") {
        match lookup(name, env) {
            Some(value) if !value.is_empty() => value.to_string(),
            _ => decode_default(default),
        }
    } else {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    };

    Ok((close + 1, selected))
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<Word>, SplitError> {
    if let Some(pos) = input.find('\0') {
        return Err(SplitError::Nul { pos });
    }

    let mut words = Vec::new();
    let mut current: Option<Word> = None;
    let mut mode = Mode::Outside;
    let mut cursor = 0;

    while cursor < input.len() {
        let c = input[cursor..].chars().next().unwrap();
        let next_cursor = cursor + c.len_utf8();

        match mode {
            Mode::Single(_) => {
                if c == '\'' {
                    mode = Mode::Outside;
                } else {
                    append_scalar(current.as_mut().unwrap(), c, false);
                }
                cursor = next_cursor;
            }
            Mode::Double(_) => match c {
                '"' => {
                    mode = Mode::Outside;
                    cursor = next_cursor;
                }
                '\\' => {
                    if next_cursor == input.len() {
                        return Err(SplitError::TrailingEscape { pos: cursor });
                    }
                    let escaped = input[next_cursor..].chars().next().unwrap();
                    let after = next_cursor + escaped.len_utf8();
                    if matches!(escaped, '"' | '\\' | '$') {
                        append_scalar(current.as_mut().unwrap(), escaped, false);
                        cursor = after;
                    } else if escaped == '\n' {
                        cursor = after;
                    } else {
                        append_scalar(current.as_mut().unwrap(), '\\', false);
                        cursor = next_cursor;
                    }
                }
                '`' => return Err(SplitError::CommandSubstitution { pos: cursor }),
                '$' => {
                    if input[next_cursor..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: cursor });
                    }
                    if input[next_cursor..].starts_with('{') {
                        let (after, value) = parse_braced(input, cursor, env)?;
                        append_str(current.as_mut().unwrap(), &value, false);
                        cursor = after;
                    } else if next_cursor < input.len()
                        && is_name_start(input[next_cursor..].chars().next().unwrap())
                    {
                        let mut end = next_cursor;
                        while end < input.len() {
                            let ch = input[end..].chars().next().unwrap();
                            if !is_name_continue(ch) {
                                break;
                            }
                            end += ch.len_utf8();
                        }
                        let name = &input[next_cursor..end];
                        append_str(
                            current.as_mut().unwrap(),
                            lookup(name, env).unwrap_or(""),
                            false,
                        );
                        cursor = end;
                    } else {
                        append_scalar(current.as_mut().unwrap(), '$', false);
                        cursor = next_cursor;
                    }
                }
                _ => {
                    append_scalar(current.as_mut().unwrap(), c, false);
                    cursor = next_cursor;
                }
            },
            Mode::Outside => match c {
                ' ' | '\t' | '\r' | '\n' => {
                    if let Some(word) = current.take() {
                        words.push(word);
                    }
                    cursor = next_cursor;
                }
                '\'' => {
                    current.get_or_insert_with(|| Word {
                        text: String::new(),
                        glob: Vec::new(),
                    });
                    mode = Mode::Single(cursor);
                    cursor = next_cursor;
                }
                '"' => {
                    current.get_or_insert_with(|| Word {
                        text: String::new(),
                        glob: Vec::new(),
                    });
                    mode = Mode::Double(cursor);
                    cursor = next_cursor;
                }
                '\\' => {
                    if next_cursor == input.len() {
                        return Err(SplitError::TrailingEscape { pos: cursor });
                    }
                    let escaped = input[next_cursor..].chars().next().unwrap();
                    let after = next_cursor + escaped.len_utf8();
                    if escaped == '\n' {
                        cursor = after;
                    } else {
                        let word = current.get_or_insert_with(|| Word {
                            text: String::new(),
                            glob: Vec::new(),
                        });
                        append_scalar(word, escaped, false);
                        cursor = after;
                    }
                }
                '`' => return Err(SplitError::CommandSubstitution { pos: cursor }),
                '$' => {
                    if input[next_cursor..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: cursor });
                    }
                    let word = current.get_or_insert_with(|| Word {
                        text: String::new(),
                        glob: Vec::new(),
                    });
                    if input[next_cursor..].starts_with('{') {
                        let (after, value) = parse_braced(input, cursor, env)?;
                        append_str(word, &value, true);
                        cursor = after;
                    } else if next_cursor < input.len()
                        && is_name_start(input[next_cursor..].chars().next().unwrap())
                    {
                        let mut end = next_cursor;
                        while end < input.len() {
                            let ch = input[end..].chars().next().unwrap();
                            if !is_name_continue(ch) {
                                break;
                            }
                            end += ch.len_utf8();
                        }
                        let name = &input[next_cursor..end];
                        append_str(word, lookup(name, env).unwrap_or(""), true);
                        cursor = end;
                    } else {
                        append_scalar(word, '$', false);
                        cursor = next_cursor;
                    }
                }
                _ => {
                    let word = current.get_or_insert_with(|| Word {
                        text: String::new(),
                        glob: Vec::new(),
                    });
                    append_scalar(word, c, true);
                    cursor = next_cursor;
                }
            },
        }
    }

    match mode {
        Mode::Single(pos) => return Err(SplitError::UnterminatedSingle { pos }),
        Mode::Double(pos) => return Err(SplitError::UnterminatedDouble { pos }),
        Mode::Outside => {}
    }

    if let Some(word) = current {
        words.push(word);
    }
    Ok(words)
}

fn main() {}