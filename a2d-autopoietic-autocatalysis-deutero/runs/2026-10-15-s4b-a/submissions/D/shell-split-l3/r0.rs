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

fn append_literal(word: &mut Word, c: char, wildcard_active: bool) {
    if wildcard_active && matches!(c, '*' | '?' | '[') {
        word.glob.push(word.text.len());
    }
    word.text.push(c);
}

fn append_expansion(word: &mut Word, value: &str, wildcard_active: bool) {
    if wildcard_active {
        for (offset, c) in value.char_indices() {
            if matches!(c, '*' | '?' | '[') {
                word.glob.push(word.text.len() + offset);
            }
        }
    }
    word.text.push_str(value);
}

fn lookup<'a>(env: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| (*key == name).then_some(*value))
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
        let c = input[i..].chars().next().expect("valid character boundary");
        let len = c.len_utf8();

        match quote {
            Quote::Single(_) => {
                if c == '\'' {
                    quote = Quote::None;
                    i += len;
                } else {
                    word.text.push(c);
                    i += len;
                }
            }
            Quote::Double(_) => {
                match c {
                    '"' => {
                        quote = Quote::None;
                        i += len;
                    }
                    '\\' => {
                        let pos = i;
                        i += 1;
                        if i == input.len() {
                            return Err(SplitError::TrailingEscape { pos });
                        }
                        let next = input[i..].chars().next().expect("valid character boundary");
                        match next {
                            '"' | '\\' | '$' => {
                                if next == '$' && input[i + 1..].starts_with('(') {
                                    return Err(SplitError::CommandSubstitution { pos: i });
                                }
                                word.text.push(next);
                                i += next.len_utf8();
                            }
                            '\n' => {
                                i += 1;
                            }
                            _ => {
                                word.text.push('\\');
                            }
                        }
                    }
                    '$' => {
                        if input[i + 1..].starts_with('(') {
                            return Err(SplitError::CommandSubstitution { pos: i });
                        }
                        if input[i + 1..].starts_with('{') {
                            let dollar = i;
                            let content_start = i + 2;
                            let mut j = content_start;
                            let mut closing = None;
                            while j < input.len() {
                                let ch = input[j..].chars().next().expect("valid character boundary");
                                if ch == '\\' {
                                    j += 1;
                                    if j < input.len() {
                                        let skipped = input[j..].chars().next().expect("valid character boundary");
                                        j += skipped.len_utf8();
                                    }
                                } else if ch == '}' {
                                    closing = Some(j);
                                    break;
                                } else {
                                    j += ch.len_utf8();
                                }
                            }
                            let end = closing.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
                            let contents = &input[content_start..end];
                            let name_end = contents
                                .char_indices()
                                .find_map(|(idx, ch)| (!is_name_continue(ch)).then_some(idx))
                                .unwrap_or(contents.len());
                            let name = &contents[..name_end];
                            if name.is_empty()
                                || !name.chars().next().is_some_and(is_name_start)
                            {
                                return Err(SplitError::InvalidExpansion { pos: dollar });
                            }
                            let remainder = &contents[name_end..];
                            let selected = if remainder.is_empty() {
                                lookup(env, name).unwrap_or("")
                            } else if let Some(default) = remainder.strip_prefix(":-") {
                                match lookup(env, name) {
                                    Some(value) if !value.is_empty() => value,
                                    _ => {
                                        let mut decoded = String::new();
                                        let mut k = 0;
                                        while k < default.len() {
                                            let ch = default[k..].chars().next().expect("valid character boundary");
                                            if ch == '\\' {
                                                k += 1;
                                                if k < default.len() {
                                                    let escaped = default[k..].chars().next().expect("valid character boundary");
                                                    decoded.push(escaped);
                                                    k += escaped.len_utf8();
                                                } else {
                                                    decoded.push('\\');
                                                }
                                            } else {
                                                decoded.push(ch);
                                                k += ch.len_utf8();
                                            }
                                        }
                                        append_expansion(&mut word, &decoded, false);
                                        active = true;
                                        i = end + 1;
                                        continue;
                                    }
                                }
                            } else {
                                return Err(SplitError::InvalidExpansion { pos: dollar });
                            };
                            append_expansion(&mut word, selected, false);
                            active = true;
                            i = end + 1;
                        } else {
                            let next_start = i + 1;
                            if next_start < input.len() {
                                let next = input[next_start..].chars().next().expect("valid character boundary");
                                if is_name_start(next) {
                                    let mut end = next_start + next.len_utf8();
                                    while end < input.len() {
                                        let ch = input[end..].chars().next().expect("valid character boundary");
                                        if !is_name_continue(ch) {
                                            break;
                                        }
                                        end += ch.len_utf8();
                                    }
                                    let value = lookup(env, &input[next_start..end]).unwrap_or("");
                                    append_expansion(&mut word, value, false);
                                    active = true;
                                    i = end;
                                } else {
                                    word.text.push('$');
                                    i += 1;
                                }
                            } else {
                                word.text.push('$');
                                i += 1;
                            }
                        }
                    }
                    '`' => return Err(SplitError::CommandSubstitution { pos: i }),
                    _ => {
                        word.text.push(c);
                        i += len;
                    }
                }
            }
            Quote::None => {
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
                        i += len;
                    }
                    '\'' => {
                        active = true;
                        quote = Quote::Single(i);
                        i += len;
                    }
                    '"' => {
                        active = true;
                        quote = Quote::Double(i);
                        i += len;
                    }
                    '\\' => {
                        let pos = i;
                        i += 1;
                        if i == input.len() {
                            return Err(SplitError::TrailingEscape { pos });
                        }
                        let next = input[i..].chars().next().expect("valid character boundary");
                        if next != '\n' {
                            active = true;
                            word.text.push(next);
                        }
                        i += next.len_utf8();
                    }
                    '$' => {
                        if input[i + 1..].starts_with('(') {
                            return Err(SplitError::CommandSubstitution { pos: i });
                        }
                        if input[i + 1..].starts_with('{') {
                            let dollar = i;
                            let content_start = i + 2;
                            let mut j = content_start;
                            let mut closing = None;
                            while j < input.len() {
                                let ch = input[j..].chars().next().expect("valid character boundary");
                                if ch == '\\' {
                                    j += 1;
                                    if j < input.len() {
                                        let skipped = input[j..].chars().next().expect("valid character boundary");
                                        j += skipped.len_utf8();
                                    }
                                } else if ch == '}' {
                                    closing = Some(j);
                                    break;
                                } else {
                                    j += ch.len_utf8();
                                }
                            }
                            let end = closing.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
                            let contents = &input[content_start..end];
                            let name_end = contents
                                .char_indices()
                                .find_map(|(idx, ch)| (!is_name_continue(ch)).then_some(idx))
                                .unwrap_or(contents.len());
                            let name = &contents[..name_end];
                            if name.is_empty()
                                || !name.chars().next().is_some_and(is_name_start)
                            {
                                return Err(SplitError::InvalidExpansion { pos: dollar });
                            }
                            let remainder = &contents[name_end..];
                            let selected = if remainder.is_empty() {
                                lookup(env, name).unwrap_or("")
                            } else if let Some(default) = remainder.strip_prefix(":-") {
                                match lookup(env, name) {
                                    Some(value) if !value.is_empty() => value,
                                    _ => {
                                        let mut decoded = String::new();
                                        let mut k = 0;
                                        while k < default.len() {
                                            let ch = default[k..].chars().next().expect("valid character boundary");
                                            if ch == '\\' {
                                                k += 1;
                                                if k < default.len() {
                                                    let escaped = default[k..].chars().next().expect("valid character boundary");
                                                    decoded.push(escaped);
                                                    k += escaped.len_utf8();
                                                } else {
                                                    decoded.push('\\');
                                                }
                                            } else {
                                                decoded.push(ch);
                                                k += ch.len_utf8();
                                            }
                                        }
                                        append_expansion(&mut word, &decoded, true);
                                        active = true;
                                        i = end + 1;
                                        continue;
                                    }
                                }
                            } else {
                                return Err(SplitError::InvalidExpansion { pos: dollar });
                            };
                            append_expansion(&mut word, selected, true);
                            active = true;
                            i = end + 1;
                        } else {
                            let next_start = i + 1;
                            if next_start < input.len() {
                                let next = input[next_start..].chars().next().expect("valid character boundary");
                                if is_name_start(next) {
                                    let mut end = next_start + next.len_utf8();
                                    while end < input.len() {
                                        let ch = input[end..].chars().next().expect("valid character boundary");
                                        if !is_name_continue(ch) {
                                            break;
                                        }
                                        end += ch.len_utf8();
                                    }
                                    let value = lookup(env, &input[next_start..end]).unwrap_or("");
                                    append_expansion(&mut word, value, true);
                                    active = true;
                                    i = end;
                                } else {
                                    active = true;
                                    word.text.push('$');
                                    i += 1;
                                }
                            } else {
                                active = true;
                                word.text.push('$');
                                i += 1;
                            }
                        }
                    }
                    '`' => return Err(SplitError::CommandSubstitution { pos: i }),
                    _ => {
                        active = true;
                        append_literal(&mut word, c, true);
                        i += len;
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
        words.push(word);
    }
    Ok(words)
}