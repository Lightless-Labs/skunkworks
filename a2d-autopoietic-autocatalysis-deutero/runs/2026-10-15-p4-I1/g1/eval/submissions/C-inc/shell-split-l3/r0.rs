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

fn append_char(word: &mut Word, c: char, wildcard_active: bool) {
    let offset = word.text.len();
    word.text.push(c);
    if wildcard_active && matches!(c, '*' | '?' | '[') {
        word.glob.push(offset);
    }
}

fn append_str(word: &mut Word, value: &str, wildcard_active: bool) {
    for c in value.chars() {
        append_char(word, c, wildcard_active);
    }
}

fn first_env<'a>(env: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| if *key == name { Some(*value) } else { None })
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
    let mut i = 0;

    while i < input.len() {
        let c = input[i..].chars().next().unwrap();
        let next_i = i + c.len_utf8();

        match quote {
            Quote::Single(_) => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    append_char(&mut current, c, false);
                }
                i = next_i;
            }
            Quote::Double(open_pos) => {
                match c {
                    '"' => {
                        quote = Quote::None;
                        i = next_i;
                    }
                    '\\' => {
                        if next_i >= input.len() {
                            return Err(SplitError::TrailingEscape { pos: i });
                        }
                        let escaped = input[next_i..].chars().next().unwrap();
                        let escaped_end = next_i + escaped.len_utf8();
                        if matches!(escaped, '"' | '\\' | '$') {
                            append_char(&mut current, escaped, false);
                            i = escaped_end;
                        } else if escaped == '\n' {
                            i = escaped_end;
                        } else {
                            append_char(&mut current, '\\', false);
                            i = next_i;
                        }
                    }
                    '`' => return Err(SplitError::CommandSubstitution { pos: i }),
                    '$' => {
                        if input[next_i..].starts_with('(') {
                            return Err(SplitError::CommandSubstitution { pos: i });
                        }
                        if input[next_i..].starts_with('{') {
                            let dollar = i;
                            let content_start = next_i + 1;
                            let mut j = content_start;
                            let mut closing = None;
                            while j < input.len() {
                                let ch = input[j..].chars().next().unwrap();
                                let end = j + ch.len_utf8();
                                if ch == '\\' {
                                    if end < input.len() {
                                        let skipped = input[end..].chars().next().unwrap();
                                        j = end + skipped.len_utf8();
                                    } else {
                                        j = end;
                                    }
                                } else if ch == '}' {
                                    closing = Some(j);
                                    break;
                                } else {
                                    j = end;
                                }
                            }
                            let close = closing.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
                            let contents = &input[content_start..close];
                            let (name, default) = if let Some(marker) = contents.find(":-") {
                                (&contents[..marker], Some(&contents[marker + 2..]))
                            } else {
                                (contents, None)
                            };
                            if name.is_empty() || !name.chars().next().is_some_and(is_name_start)
                                || !name.chars().all(is_name_continue)
                            {
                                return Err(SplitError::InvalidExpansion { pos: dollar });
                            }

                            let value = first_env(env, name);
                            if let Some(default_text) = default {
                                if let Some(value) = value.filter(|v| !v.is_empty()) {
                                    append_str(&mut current, value, false);
                                } else {
                                    let mut k = 0;
                                    while k < default_text.len() {
                                        let ch = default_text[k..].chars().next().unwrap();
                                        let end = k + ch.len_utf8();
                                        if ch == '\\' && end < default_text.len() {
                                            let escaped = default_text[end..].chars().next().unwrap();
                                            append_char(&mut current, escaped, false);
                                            k = end + escaped.len_utf8();
                                        } else {
                                            append_char(&mut current, ch, false);
                                            k = end;
                                        }
                                    }
                                }
                            } else if let Some(value) = value {
                                append_str(&mut current, value, false);
                            }
                            active = true;
                            i = close + 1;
                        } else if next_i < input.len() {
                            let following = input[next_i..].chars().next().unwrap();
                            if is_name_start(following) {
                                let mut end = next_i + following.len_utf8();
                                while end < input.len() {
                                    let ch = input[end..].chars().next().unwrap();
                                    if is_name_continue(ch) {
                                        end += ch.len_utf8();
                                    } else {
                                        break;
                                    }
                                }
                                let name = &input[next_i..end];
                                if let Some(value) = first_env(env, name) {
                                    append_str(&mut current, value, false);
                                }
                                active = true;
                                i = end;
                            } else {
                                append_char(&mut current, '$', false);
                                i = next_i;
                            }
                        } else {
                            append_char(&mut current, '$', false);
                            i = next_i;
                        }
                    }
                    _ => {
                        append_char(&mut current, c, false);
                        i = next_i;
                    }
                }
                let _ = open_pos;
            }
            Quote::None => {
                match c {
                    ' ' | '\t' | '\r' | '\n' => {
                        if active {
                            words.push(current);
                            current = Word {
                                text: String::new(),
                                glob: Vec::new(),
                            };
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
                        if next_i >= input.len() {
                            return Err(SplitError::TrailingEscape { pos: i });
                        }
                        let escaped = input[next_i..].chars().next().unwrap();
                        let end = next_i + escaped.len_utf8();
                        if escaped != '\n' {
                            append_char(&mut current, escaped, false);
                            active = true;
                        }
                        i = end;
                    }
                    '`' => return Err(SplitError::CommandSubstitution { pos: i }),
                    '$' => {
                        if input[next_i..].starts_with('(') {
                            return Err(SplitError::CommandSubstitution { pos: i });
                        }
                        if input[next_i..].starts_with('{') {
                            let dollar = i;
                            let content_start = next_i + 1;
                            let mut j = content_start;
                            let mut closing = None;
                            while j < input.len() {
                                let ch = input[j..].chars().next().unwrap();
                                let end = j + ch.len_utf8();
                                if ch == '\\' {
                                    if end < input.len() {
                                        let skipped = input[end..].chars().next().unwrap();
                                        j = end + skipped.len_utf8();
                                    } else {
                                        j = end;
                                    }
                                } else if ch == '}' {
                                    closing = Some(j);
                                    break;
                                } else {
                                    j = end;
                                }
                            }
                            let close = closing.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
                            let contents = &input[content_start..close];
                            let (name, default) = if let Some(marker) = contents.find(":-") {
                                (&contents[..marker], Some(&contents[marker + 2..]))
                            } else {
                                (contents, None)
                            };
                            if name.is_empty() || !name.chars().next().is_some_and(is_name_start)
                                || !name.chars().all(is_name_continue)
                            {
                                return Err(SplitError::InvalidExpansion { pos: dollar });
                            }

                            let value = first_env(env, name);
                            if let Some(default_text) = default {
                                if let Some(value) = value.filter(|v| !v.is_empty()) {
                                    append_str(&mut current, value, true);
                                } else {
                                    let mut k = 0;
                                    while k < default_text.len() {
                                        let ch = default_text[k..].chars().next().unwrap();
                                        let end = k + ch.len_utf8();
                                        if ch == '\\' && end < default_text.len() {
                                            let escaped = default_text[end..].chars().next().unwrap();
                                            append_char(&mut current, escaped, true);
                                            k = end + escaped.len_utf8();
                                        } else {
                                            append_char(&mut current, ch, true);
                                            k = end;
                                        }
                                    }
                                }
                            } else if let Some(value) = value {
                                append_str(&mut current, value, true);
                            }
                            active = true;
                            i = close + 1;
                        } else if next_i < input.len() {
                            let following = input[next_i..].chars().next().unwrap();
                            if is_name_start(following) {
                                let mut end = next_i + following.len_utf8();
                                while end < input.len() {
                                    let ch = input[end..].chars().next().unwrap();
                                    if is_name_continue(ch) {
                                        end += ch.len_utf8();
                                    } else {
                                        break;
                                    }
                                }
                                let name = &input[next_i..end];
                                if let Some(value) = first_env(env, name) {
                                    append_str(&mut current, value, true);
                                }
                                active = true;
                                i = end;
                            } else {
                                append_char(&mut current, '$', true);
                                active = true;
                                i = next_i;
                            }
                        } else {
                            append_char(&mut current, '$', true);
                            active = true;
                            i = next_i;
                        }
                    }
                    _ => {
                        append_char(&mut current, c, true);
                        active = true;
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
        words.push(current);
    }
    Ok(words)
}

fn main() {}