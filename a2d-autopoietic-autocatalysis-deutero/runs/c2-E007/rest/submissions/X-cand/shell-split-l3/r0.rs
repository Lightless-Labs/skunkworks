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

fn append_literal(word: &mut Word, c: char, active_glob: bool) {
    if active_glob && matches!(c, '*' | '?' | '[') {
        word.glob.push(word.text.len());
    }
    word.text.push(c);
}

fn append_expansion(word: &mut Word, value: &str, active_glob: bool) {
    for c in value.chars() {
        append_literal(word, c, active_glob);
    }
}

fn lookup<'a>(env: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| if *key == name { Some(*value) } else { None })
}

fn decode_default(text: &str) -> String {
    let mut out = String::new();
    let mut iter = text.chars().peekable();
    while let Some(c) = iter.next() {
        if c == '\\' {
            if let Some(next) = iter.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
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
    let mut i = 0;

    while i < input.len() {
        let c = input[i..].chars().next().unwrap();
        let next_i = i + c.len_utf8();

        match quote {
            Quote::Single(_) => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    word.text.push(c);
                }
                i = next_i;
            }
            Quote::Double(_) => {
                if c == '"' {
                    quote = Quote::None;
                    i = next_i;
                } else if c == '\\' {
                    if next_i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let n = input[next_i..].chars().next().unwrap();
                    let after_n = next_i + n.len_utf8();
                    if matches!(n, '"' | '\\' | '$' | '\n') {
                        word.text.push_if_not_lf(n);
                        i = after_n;
                    } else {
                        word.text.push('\\');
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
                        append_expansion(&mut word, &value, false);
                        active = true;
                        i = end;
                    } else {
                        let (end, name) = parse_simple_name(input, next_i);
                        if end == next_i {
                            word.text.push('$');
                            i = next_i;
                        } else {
                            let value = lookup(env, name).unwrap_or("");
                            append_expansion(&mut word, value, false);
                            active = true;
                            i = end;
                        }
                    }
                } else {
                    word.text.push(c);
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
                    quote = Quote::Single(i);
                    active = true;
                    i = next_i;
                } else if c == '"' {
                    quote = Quote::Double(i);
                    active = true;
                    i = next_i;
                } else if c == '\\' {
                    if next_i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let n = input[next_i..].chars().next().unwrap();
                    let after_n = next_i + n.len_utf8();
                    if n == '\n' {
                        i = after_n;
                    } else {
                        append_literal(&mut word, n, false);
                        active = true;
                        i = after_n;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if input[next_i..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }
                    if input[next_i..].starts_with('{') {
                        let (end, value) = parse_braced(input, i, env)?;
                        append_expansion(&mut word, &value, true);
                        active = true;
                        i = end;
                    } else {
                        let (end, name) = parse_simple_name(input, next_i);
                        if end == next_i {
                            word.text.push('$');
                            active = true;
                            i = next_i;
                        } else {
                            let value = lookup(env, name).unwrap_or("");
                            append_expansion(&mut word, value, true);
                            active = true;
                            i = end;
                        }
                    }
                } else {
                    append_literal(&mut word, c, true);
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

fn parse_simple_name(input: &str, start: usize) -> (usize, &str) {
    let Some(first) = input[start..].chars().next() else {
        return (start, "");
    };
    if !is_name_start(first) {
        return (start, "");
    }

    let mut end = start + first.len_utf8();
    while end < input.len() {
        let c = input[end..].chars().next().unwrap();
        if !is_name_continue(c) {
            break;
        }
        end += c.len_utf8();
    }
    (end, &input[start..end])
}

fn parse_braced(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
) -> Result<(usize, String), SplitError> {
    let content_start = dollar + 2;
    let mut cursor = content_start;
    let mut close = None;

    while cursor < input.len() {
        let c = input[cursor..].chars().next().unwrap();
        if c == '\\' {
            cursor += c.len_utf8();
            if cursor < input.len() {
                let escaped = input[cursor..].chars().next().unwrap();
                cursor += escaped.len_utf8();
            }
        } else if c == '}' {
            close = Some(cursor);
            break;
        } else {
            cursor += c.len_utf8();
        }
    }

    let Some(close_pos) = close else {
        return Err(SplitError::UnterminatedExpansion { pos: dollar });
    };

    let contents = &input[content_start..close_pos];
    let (name_end, name) = parse_simple_name(contents, 0);
    if name_end == 0 {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }

    let value = if name_end == contents.len() {
        lookup(env, name).unwrap_or("").to_string()
    } else if contents[name_end..].starts_with(":-") {
        let default = &contents[name_end + 2..];
        match lookup(env, name) {
            Some(value) if !value.is_empty() => value.to_string(),
            _ => decode_default(default),
        }
    } else {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    };

    Ok((close_pos + 1, value))
}

trait PushIfNotLf {
    fn push_if_not_lf(&mut self, c: char);
}

impl PushIfNotLf for String {
    fn push_if_not_lf(&mut self, c: char) {
        if c != '\n' {
            self.push(c);
        }
    }
}