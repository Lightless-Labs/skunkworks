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
    is_name_start(c) || c.is_ascii_digit()
}

fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

fn append_literal(word: &mut Word, c: char, active_wildcard: bool) {
    let offset = word.text.len();
    word.text.push(c);
    if active_wildcard && matches!(c, '*' | '?' | '[') {
        word.glob.push(offset);
    }
}

fn append_expansion(word: &mut Word, value: &str, active_wildcard: bool) {
    for c in value.chars() {
        append_literal(word, c, active_wildcard);
    }
}

fn lookup<'a>(env: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| if *key == name { Some(*value) } else { None })
}

fn char_at(input: &str, pos: usize) -> Option<char> {
    input.get(pos..)?.chars().next()
}

fn next_pos(input: &str, pos: usize) -> usize {
    pos + char_at(input, pos).map(char::len_utf8).unwrap_or(0)
}

fn expansion(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
) -> Result<(usize, String), SplitError> {
    let after_dollar = dollar + 1;
    if char_at(input, after_dollar) != Some('{') {
        let mut end = after_dollar;
        while let Some(c) = char_at(input, end) {
            if !is_name_char(c) {
                break;
            }
            end += c.len_utf8();
        }
        let name = &input[after_dollar..end];
        return Ok((end, lookup(env, name).unwrap_or("").to_string()));
    }

    let content_start = after_dollar + 1;
    let mut cursor = content_start;
    let mut close = None;

    while let Some(c) = char_at(input, cursor) {
        if c == '\\' {
            cursor += 1;
            if let Some(next) = char_at(input, cursor) {
                cursor += next.len_utf8();
            }
        } else if c == '}' {
            close = Some(cursor);
            break;
        } else {
            cursor += c.len_utf8();
        }
    }

    let close = close.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
    let contents = &input[content_start..close];

    let mut name_end = 0;
    let mut chars = contents.char_indices();
    match chars.next() {
        Some((_, first)) if is_name_start(first) => name_end = first.len_utf8(),
        _ => return Err(SplitError::InvalidExpansion { pos: dollar }),
    }
    while let Some(c) = char_at(contents, name_end) {
        if !is_name_char(c) {
            break;
        }
        name_end += c.len_utf8();
    }

    let name = &contents[..name_end];
    let rest = &contents[name_end..];
    let selected = if rest.is_empty() {
        lookup(env, name).unwrap_or("").to_string()
    } else if let Some(default) = rest.strip_prefix(":-") {
        match lookup(env, name) {
            Some(value) if !value.is_empty() => value.to_string(),
            _ => decode_default(default),
        }
    } else {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    };

    Ok((close + 1, selected))
}

fn decode_default(text: &str) -> String {
    let mut out = String::new();
    let mut pos = 0;
    while let Some(c) = char_at(text, pos) {
        if c == '\\' {
            pos += 1;
            if let Some(next) = char_at(text, pos) {
                out.push(next);
                pos += next.len_utf8();
            }
        } else {
            out.push(c);
            pos += c.len_utf8();
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
    let mut quote_start = 0;
    let mut pos = 0;

    while pos < input.len() {
        let c = char_at(input, pos).unwrap();

        match quote {
            Quote::Single => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    append_literal(&mut word, c, false);
                }
                pos += c.len_utf8();
            }
            Quote::Double => {
                if c == '"' {
                    quote = Quote::None;
                    pos += 1;
                } else if c == '\\' {
                    let slash = pos;
                    pos += 1;
                    match char_at(input, pos) {
                        None => return Err(SplitError::TrailingEscape { pos: slash }),
                        Some(next) if matches!(next, '"' | '\\' | '$' | '\n') => {
                            pos += next.len_utf8();
                            if next != '\n' {
                                append_literal(&mut word, next, false);
                            }
                        }
                        Some(_) => append_literal(&mut word, '\\', false),
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos });
                } else if c == '$' {
                    if char_at(input, pos + 1) == Some('(') {
                        return Err(SplitError::CommandSubstitution { pos });
                    }
                    if char_at(input, pos + 1).is_some_and(is_name_start)
                        || char_at(input, pos + 1) == Some('{')
                    {
                        active = true;
                        let (end, value) = expansion(input, pos, env)?;
                        append_expansion(&mut word, &value, false);
                        pos = end;
                    } else {
                        append_literal(&mut word, '$', false);
                        pos += 1;
                    }
                } else {
                    append_literal(&mut word, c, false);
                    pos += c.len_utf8();
                }
            }
            Quote::None => {
                if is_separator(c) {
                    if active {
                        words.push(std::mem::replace(
                            &mut word,
                            Word {
                                text: String::new(),
                                glob: Vec::new(),
                            },
                        ));
                        active = false;
                    }
                    pos += c.len_utf8();
                } else if c == '\'' {
                    active = true;
                    quote = Quote::Single;
                    quote_start = pos;
                    pos += 1;
                } else if c == '"' {
                    active = true;
                    quote = Quote::Double;
                    quote_start = pos;
                    pos += 1;
                } else if c == '\\' {
                    let slash = pos;
                    pos += 1;
                    match char_at(input, pos) {
                        None => return Err(SplitError::TrailingEscape { pos: slash }),
                        Some('\n') => pos += 1,
                        Some(next) => {
                            active = true;
                            append_literal(&mut word, next, false);
                            pos += next.len_utf8();
                        }
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos });
                } else if c == '$' {
                    if char_at(input, pos + 1) == Some('(') {
                        return Err(SplitError::CommandSubstitution { pos });
                    }
                    if char_at(input, pos + 1).is_some_and(is_name_start)
                        || char_at(input, pos + 1) == Some('{')
                    {
                        active = true;
                        let (end, value) = expansion(input, pos, env)?;
                        append_expansion(&mut word, &value, true);
                        pos = end;
                    } else {
                        active = true;
                        append_literal(&mut word, '$', false);
                        pos += 1;
                    }
                } else {
                    active = true;
                    append_literal(&mut word, c, true);
                    pos += c.len_utf8();
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
        words.push(word);
    }
    Ok(words)
}

fn bare_candidate(word: &str) -> Option<String> {
    if word.is_empty() || word.contains('\n') {
        return None;
    }

    let mut out = String::new();
    for c in word.chars() {
        if c.is_ascii_alphanumeric() || "_-./:=,+@%".contains(c) {
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
        Some(format!("'{word}'"))
    }
}

fn double_candidate(word: &str) -> Option<String> {
    if word.contains('`') {
        return None;
    }

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

fn encode_word(word: &str) -> String {
    let mut candidates: Vec<(usize, String)> = Vec::new();

    if let Some(candidate) = bare_candidate(word) {
        candidates.push((0, candidate));
    }
    if let Some(candidate) = single_candidate(word) {
        candidates.push((1, candidate));
    }
    if let Some(candidate) = double_candidate(word) {
        candidates.push((2, candidate));
    }

    if candidates.is_empty() {
        return fallback_candidate(word);
    }

    candidates.sort_by(|a, b| a.1.len().cmp(&b.1.len()).then(a.0.cmp(&b.0)));
    candidates.remove(0).1
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

    Ok(words
        .iter()
        .map(|word| encode_word(word))
        .collect::<Vec<_>>()
        .join(" "))
}

fn main() {}