use std::fmt::Write;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JoinError {
    Nul { word: usize, pos: usize },
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

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn separators(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

fn push_text(word: &mut Word, value: &str, active_globs: bool) {
    for c in value.chars() {
        if active_globs && matches!(c, '*' | '?' | '[') {
            word.glob.push(word.text.len());
        }
        word.text.push(c);
    }
}

fn lookup<'a>(name: &str, env: &'a [(&str, &str)]) -> Option<&'a str> {
    env.iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| *value)
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
                    word.text.push(c);
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
                            word.text.push(n);
                        }
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
                        let (value, end) = parse_braced(input, i, env)?;
                        push_text(&mut word, &value, false);
                        active = true;
                        i = end;
                    } else if next_i < input.len() {
                        let n = input[next_i..].chars().next().unwrap();
                        if is_name_start(n) {
                            let (name, end) = parse_name(input, next_i);
                            let value = lookup(name, env).unwrap_or("");
                            push_text(&mut word, value, false);
                            active = true;
                            i = end;
                        } else {
                            word.text.push('$');
                            i = next_i;
                        }
                    } else {
                        word.text.push('$');
                        i = next_i;
                    }
                } else {
                    word.text.push(c);
                    i = next_i;
                }
            }
            Quote::None => {
                if separators(c) {
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
                    if next_i == input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let n = input[next_i..].chars().next().unwrap();
                    let after_n = next_i + n.len_utf8();
                    if n == '\n' {
                        i = after_n;
                    } else {
                        word.text.push(n);
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
                        let (value, end) = parse_braced(input, i, env)?;
                        push_text(&mut word, &value, true);
                        active = true;
                        i = end;
                    } else if next_i < input.len() {
                        let n = input[next_i..].chars().next().unwrap();
                        if is_name_start(n) {
                            let (name, end) = parse_name(input, next_i);
                            let value = lookup(name, env).unwrap_or("");
                            push_text(&mut word, value, true);
                            active = true;
                            i = end;
                        } else {
                            word.text.push('$');
                            active = true;
                            i = next_i;
                        }
                    } else {
                        word.text.push('$');
                        active = true;
                        i = next_i;
                    }
                } else {
                    if matches!(c, '*' | '?' | '[') {
                        word.glob.push(word.text.len());
                    }
                    word.text.push(c);
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

fn parse_name(input: &str, start: usize) -> (&str, usize) {
    let mut end = start;
    for (offset, c) in input[start..].char_indices() {
        if !is_name_char(c) {
            break;
        }
        end = start + offset + c.len_utf8();
    }
    (&input[start..end], end)
}

fn parse_braced(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
) -> Result<(String, usize), SplitError> {
    let content_start = dollar + 2;
    let mut p = content_start;
    let close;

    loop {
        if p >= input.len() {
            return Err(SplitError::UnterminatedExpansion { pos: dollar });
        }
        let c = input[p..].chars().next().unwrap();
        let after = p + c.len_utf8();
        if c == '\\' {
            if after >= input.len() {
                return Err(SplitError::UnterminatedExpansion { pos: dollar });
            }
            let escaped = input[after..].chars().next().unwrap();
            p = after + escaped.len_utf8();
        } else if c == '}' {
            close = p;
            break;
        } else {
            p = after;
        }
    }

    let contents = &input[content_start..close];
    let mut name_end = 0usize;
    let mut chars = contents.char_indices();
    let Some((_, first)) = chars.next() else {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    };
    if !is_name_start(first) {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }
    name_end = first.len_utf8();
    for (offset, c) in chars {
        if !is_name_char(c) {
            break;
        }
        name_end = offset + c.len_utf8();
    }

    let name = &contents[..name_end];
    let tail = &contents[name_end..];
    let (has_default, default_start) = if tail.is_empty() {
        (false, 0)
    } else if tail.starts_with(":-") {
        (true, name_end + 2)
    } else {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    };

    let found = lookup(name, env);
    if has_default && found.map_or(true, str::is_empty) {
        let raw = &contents[default_start..];
        let mut decoded = String::new();
        let mut cursor = 0usize;
        while cursor < raw.len() {
            let c = raw[cursor..].chars().next().unwrap();
            let after = cursor + c.len_utf8();
            if c == '\\' {
                if after < raw.len() {
                    let n = raw[after..].chars().next().unwrap();
                    decoded.push(n);
                    cursor = after + n.len_utf8();
                } else {
                    decoded.push('\\');
                    cursor = after;
                }
            } else {
                decoded.push(c);
                cursor = after;
            }
        }
        Ok((decoded, close + 1))
    } else {
        Ok((found.unwrap_or("").to_string(), close + 1))
    }
}

fn bare_candidate(word: &str) -> Option<String> {
    if word.is_empty() || word.contains('\n') {
        return None;
    }
    let mut out = String::new();
    for c in word.chars() {
        if c.is_ascii_alphanumeric()
            || matches!(c, '_' | '-' | '.' | '/' | ':' | '=' | ',' | '+' | '@' | '%')
        {
            out.push(c);
        } else {
            out.push('\\');
            out.push(c);
        }
    }
    Some(out)
}

fn single_candidate(word: &str) -> Option<String> {
    if word.contains('\'') {
        None
    } else {
        Some(format!("'{}'", word))
    }
}

fn double_candidate(word: &str) -> Option<String> {
    if word.contains('`') {
        None
    } else {
        let mut out = String::from("\"");
        for c in word.chars() {
            if matches!(c, '\\' | '"' | '$') {
                out.push('\\');
            }
            out.push(c);
        }
        out.push('"');
        Some(out)
    }
}

fn fallback_candidate(word: &str) -> String {
    let mut out = String::from("'");
    for c in word.chars() {
        if c == '\'' {
            out.push('\'');
            out.push('\\');
            out.push('\'');
            out.push('\'');
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

pub fn join(words: &[String]) -> Result<String, JoinError> {
    for (word_index, word) in words.iter().enumerate() {
        if let Some(pos) = word.find('\0') {
            return Err(JoinError::Nul {
                word: word_index,
                pos,
            });
        }
    }

    let mut encoded = Vec::with_capacity(words.len());
    for word in words {
        let candidates = [
            bare_candidate(word),
            single_candidate(word),
            double_candidate(word),
        ];
        let best = candidates
            .into_iter()
            .enumerate()
            .filter_map(|(priority, candidate)| candidate.map(|s| (priority, s)))
            .min_by_key(|(priority, s)| (s.len(), *priority))
            .map(|(_, s)| s)
            .unwrap_or_else(|| fallback_candidate(word));
        encoded.push(best);
    }

    let mut result = String::new();
    for (index, item) in encoded.iter().enumerate() {
        if index != 0 {
            result.push(' ');
        }
        result.push_str(item);
    }
    Ok(result)
}