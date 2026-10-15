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

fn env_value<'a>(env: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| (*key == name).then_some(*value))
}

fn append(text: &mut String, glob: &mut Vec<usize>, value: &str, active: bool) {
    for c in value.chars() {
        if active && matches!(c, '*' | '?' | '[') {
            glob.push(text.len());
        }
        text.push(c);
    }
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<Word>, SplitError> {
    if let Some(pos) = input.as_bytes().iter().position(|&b| b == 0) {
        return Err(SplitError::Nul { pos });
    }

    let mut words = Vec::new();
    let mut text = String::new();
    let mut glob = Vec::new();
    let mut active_word = false;
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
                    text.push(c);
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
                    let n = input[next_i..].chars().next().unwrap();
                    let after_n = next_i + n.len_utf8();
                    if matches!(n, '"' | '\\' | '$' | '\n') {
                        if n != '\n' {
                            text.push(n);
                        }
                        i = after_n;
                    } else {
                        text.push('\\');
                        i = next_i;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if input[next_i..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }
                    if input[next_i..].starts_with('{') {
                        let dollar = i;
                        let content_start = next_i + 1;
                        let mut j = content_start;
                        let mut close = None;
                        while j < input.len() {
                            let x = input[j..].chars().next().unwrap();
                            let x_end = j + x.len_utf8();
                            if x == '\\' && x_end < input.len() {
                                let escaped = input[x_end..].chars().next().unwrap();
                                j = x_end + escaped.len_utf8();
                            } else if x == '}' {
                                close = Some(j);
                                break;
                            } else {
                                j = x_end;
                            }
                        }
                        let close = close.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
                        let contents = &input[content_start..close];
                        let (name, default) = if let Some(op) = contents.find(":-") {
                            (&contents[..op], Some(&contents[op + 2..]))
                        } else {
                            (contents, None)
                        };
                        if name.is_empty() || !name.chars().next().is_some_and(is_name_start)
                            || !name.chars().all(is_name_continue)
                        {
                            return Err(SplitError::InvalidExpansion { pos: dollar });
                        }
                        active_word = true;
                        let value = match (default, env_value(env, name)) {
                            (Some(default), Some(value)) if value.is_empty() => {
                                let mut decoded = String::new();
                                let mut k = 0;
                                while k < default.len() {
                                    let x = default[k..].chars().next().unwrap();
                                    let x_end = k + x.len_utf8();
                                    if x == '\\' && x_end < default.len() {
                                        let escaped = default[x_end..].chars().next().unwrap();
                                        decoded.push(escaped);
                                        k = x_end + escaped.len_utf8();
                                    } else {
                                        decoded.push(x);
                                        k = x_end;
                                    }
                                }
                                append(&mut text, &mut glob, &decoded, false);
                                i = close + 1;
                                continue;
                            }
                            (Some(_), Some(value)) if !value.is_empty() => value,
                            (Some(default), _) => {
                                let mut decoded = String::new();
                                let mut k = 0;
                                while k < default.len() {
                                    let x = default[k..].chars().next().unwrap();
                                    let x_end = k + x.len_utf8();
                                    if x == '\\' && x_end < default.len() {
                                        let escaped = default[x_end..].chars().next().unwrap();
                                        decoded.push(escaped);
                                        k = x_end + escaped.len_utf8();
                                    } else {
                                        decoded.push(x);
                                        k = x_end;
                                    }
                                }
                                append(&mut text, &mut glob, &decoded, false);
                                i = close + 1;
                                continue;
                            }
                            (None, Some(value)) => value,
                            (None, None) => "",
                        };
                        append(&mut text, &mut glob, value, false);
                        i = close + 1;
                    } else if next_i < input.len() {
                        let n = input[next_i..].chars().next().unwrap();
                        if is_name_start(n) {
                            let mut end = next_i + n.len_utf8();
                            while end < input.len() {
                                let x = input[end..].chars().next().unwrap();
                                if is_name_continue(x) {
                                    end += x.len_utf8();
                                } else {
                                    break;
                                }
                            }
                            active_word = true;
                            let name = &input[next_i..end];
                            append(&mut text, &mut glob, env_value(env, name).unwrap_or(""), false);
                            i = end;
                        } else {
                            text.push('$');
                            i = next_i;
                        }
                    } else {
                        text.push('$');
                        i = next_i;
                    }
                } else {
                    text.push(c);
                    i = next_i;
                }
            }
            Quote::None => {
                if is_separator(c) {
                    if active_word {
                        words.push(Word {
                            text: std::mem::take(&mut text),
                            glob: std::mem::take(&mut glob),
                        });
                        active_word = false;
                    }
                    i = next_i;
                } else if c == '\'' {
                    quote = Quote::Single;
                    quote_start = i;
                    active_word = true;
                    i = next_i;
                } else if c == '"' {
                    quote = Quote::Double;
                    quote_start = i;
                    active_word = true;
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
                        active_word = true;
                        text.push(n);
                        i = after_n;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if input[next_i..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }
                    if input[next_i..].starts_with('{') {
                        let dollar = i;
                        let content_start = next_i + 1;
                        let mut j = content_start;
                        let mut close = None;
                        while j < input.len() {
                            let x = input[j..].chars().next().unwrap();
                            let x_end = j + x.len_utf8();
                            if x == '\\' && x_end < input.len() {
                                let escaped = input[x_end..].chars().next().unwrap();
                                j = x_end + escaped.len_utf8();
                            } else if x == '}' {
                                close = Some(j);
                                break;
                            } else {
                                j = x_end;
                            }
                        }
                        let close = close.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
                        let contents = &input[content_start..close];
                        let (name, default) = if let Some(op) = contents.find(":-") {
                            (&contents[..op], Some(&contents[op + 2..]))
                        } else {
                            (contents, None)
                        };
                        if name.is_empty() || !name.chars().next().is_some_and(is_name_start)
                            || !name.chars().all(is_name_continue)
                        {
                            return Err(SplitError::InvalidExpansion { pos: dollar });
                        }
                        active_word = true;
                        let value = match (default, env_value(env, name)) {
                            (Some(default), Some(value)) if value.is_empty() => {
                                let mut decoded = String::new();
                                let mut k = 0;
                                while k < default.len() {
                                    let x = default[k..].chars().next().unwrap();
                                    let x_end = k + x.len_utf8();
                                    if x == '\\' && x_end < default.len() {
                                        let escaped = default[x_end..].chars().next().unwrap();
                                        decoded.push(escaped);
                                        k = x_end + escaped.len_utf8();
                                    } else {
                                        decoded.push(x);
                                        k = x_end;
                                    }
                                }
                                append(&mut text, &mut glob, &decoded, true);
                                i = close + 1;
                                continue;
                            }
                            (Some(_), Some(value)) if !value.is_empty() => value,
                            (Some(default), _) => {
                                let mut decoded = String::new();
                                let mut k = 0;
                                while k < default.len() {
                                    let x = default[k..].chars().next().unwrap();
                                    let x_end = k + x.len_utf8();
                                    if x == '\\' && x_end < default.len() {
                                        let escaped = default[x_end..].chars().next().unwrap();
                                        decoded.push(escaped);
                                        k = x_end + escaped.len_utf8();
                                    } else {
                                        decoded.push(x);
                                        k = x_end;
                                    }
                                }
                                append(&mut text, &mut glob, &decoded, true);
                                i = close + 1;
                                continue;
                            }
                            (None, Some(value)) => value,
                            (None, None) => "",
                        };
                        append(&mut text, &mut glob, value, true);
                        i = close + 1;
                    } else if next_i < input.len() {
                        let n = input[next_i..].chars().next().unwrap();
                        if is_name_start(n) {
                            let mut end = next_i + n.len_utf8();
                            while end < input.len() {
                                let x = input[end..].chars().next().unwrap();
                                if is_name_continue(x) {
                                    end += x.len_utf8();
                                } else {
                                    break;
                                }
                            }
                            active_word = true;
                            let name = &input[next_i..end];
                            append(&mut text, &mut glob, env_value(env, name).unwrap_or(""), true);
                            i = end;
                        } else {
                            active_word = true;
                            text.push('$');
                            i = next_i;
                        }
                    } else {
                        active_word = true;
                        text.push('$');
                        i = next_i;
                    }
                } else {
                    active_word = true;
                    if matches!(c, '*' | '?' | '[') {
                        glob.push(text.len());
                    }
                    text.push(c);
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

    if active_word {
        words.push(Word { text, glob });
    }
    Ok(words)
}

fn main() {}