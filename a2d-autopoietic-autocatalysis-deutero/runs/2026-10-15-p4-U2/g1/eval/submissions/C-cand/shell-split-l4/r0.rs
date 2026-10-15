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
    Single(usize),
    Double(usize),
}

fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_name_char(c: char) -> bool {
    is_name_start(c) || c.is_ascii_digit()
}

fn push_literal(word: &mut Word, c: char, active_glob: bool) {
    if active_glob && matches!(c, '*' | '?' | '[') {
        word.glob.push(word.text.len());
    }
    word.text.push(c);
}

fn append_expansion(word: &mut Word, value: &str, active_glob: bool) {
    for c in value.chars() {
        push_literal(word, c, active_glob);
    }
}

fn lookup_env<'a>(env: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| (*key == name).then_some(*value))
}

fn parse_braced(
    input: &str,
    dollar: usize,
    after_open: usize,
    env: &[(&str, &str)],
    word: &mut Word,
    active_glob: bool,
) -> Result<usize, SplitError> {
    let mut close = None;
    let mut search = after_open;
    while search < input.len() {
        let c = input[search..].chars().next().unwrap();
        if c == '\\' {
            search += 1;
            if search < input.len() {
                search += input[search..].chars().next().unwrap().len_utf8();
            }
        } else if c == '}' {
            close = Some(search);
            break;
        } else {
            search += c.len_utf8();
        }
    }

    let close = close.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
    let contents = &input[after_open..close];

    let name_end = contents
        .char_indices()
        .find_map(|(i, c)| (!is_name_char(c)).then_some(i))
        .unwrap_or(contents.len());

    if name_end == 0 {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }

    let name = &contents[..name_end];
    if !name.chars().next().is_some_and(is_name_start) {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }

    let suffix = &contents[name_end..];
    let selected_default = if suffix.is_empty() {
        None
    } else if let Some(default) = suffix.strip_prefix(":-") {
        Some(default)
    } else {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    };

    if let Some(default) = selected_default {
        match lookup_env(env, name) {
            Some(value) if !value.is_empty() => append_expansion(word, value, active_glob),
            _ => {
                let mut chars = default.chars().peekable();
                while let Some(c) = chars.next() {
                    if c == '\\' {
                        if let Some(next) = chars.next() {
                            push_literal(word, next, active_glob);
                        } else {
                            push_literal(word, '\\', active_glob);
                        }
                    } else {
                        push_literal(word, c, active_glob);
                    }
                }
            }
        }
    } else if let Some(value) = lookup_env(env, name) {
        append_expansion(word, value, active_glob);
    }

    Ok(close + 1)
}

fn expand_dollar(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
    word: &mut Word,
    active_glob: bool,
) -> Result<Option<usize>, SplitError> {
    let next_pos = dollar + 1;
    if next_pos >= input.len() {
        return Ok(None);
    }

    let next = input[next_pos..].chars().next().unwrap();
    if next == '(' {
        return Err(SplitError::CommandSubstitution { pos: dollar });
    }

    if next == '{' {
        let end = parse_braced(input, dollar, next_pos + 1, env, word, active_glob)?;
        return Ok(Some(end));
    }

    if !is_name_start(next) {
        return Ok(None);
    }

    let mut end = next_pos;
    while end < input.len() {
        let c = input[end..].chars().next().unwrap();
        if !is_name_char(c) {
            break;
        }
        end += c.len_utf8();
    }
    let name = &input[next_pos..end];
    if let Some(value) = lookup_env(env, name) {
        append_expansion(word, value, active_glob);
    }
    Ok(Some(end))
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
        let len = c.len_utf8();

        match quote {
            Quote::Single(_) => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    push_literal(&mut word, c, false);
                }
                i += len;
            }
            Quote::Double(open) => {
                if c == '"' {
                    quote = Quote::None;
                    i += len;
                } else if c == '\\' {
                    let slash = i;
                    i += 1;
                    if i >= input.len() {
                        return Err(SplitError::TrailingEscape { pos: slash });
                    }
                    let next = input[i..].chars().next().unwrap();
                    if matches!(next, '"' | '\\' | '$' | '\n') {
                        if next != '\n' {
                            push_literal(&mut word, next, false);
                        }
                        i += next.len_utf8();
                    } else {
                        push_literal(&mut word, '\\', false);
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if let Some(end) = expand_dollar(input, i, env, &mut word, false)? {
                        active = true;
                        i = end;
                    } else {
                        push_literal(&mut word, '$', false);
                        i += len;
                    }
                } else {
                    push_literal(&mut word, c, false);
                    i += len;
                }
                if matches!(quote, Quote::Double(_)) && i == input.len() {
                    if input.ends_with('\\') {
                        return Err(SplitError::TrailingEscape { pos: input.len() - 1 });
                    }
                    let _ = open;
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
                    i += len;
                } else if c == '\'' {
                    active = true;
                    quote = Quote::Single(i);
                    i += len;
                } else if c == '"' {
                    active = true;
                    quote = Quote::Double(i);
                    i += len;
                } else if c == '\\' {
                    let slash = i;
                    i += 1;
                    if i >= input.len() {
                        return Err(SplitError::TrailingEscape { pos: slash });
                    }
                    let next = input[i..].chars().next().unwrap();
                    i += next.len_utf8();
                    if next != '\n' {
                        active = true;
                        push_literal(&mut word, next, false);
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if let Some(end) = expand_dollar(input, i, env, &mut word, true)? {
                        active = true;
                        i = end;
                    } else {
                        active = true;
                        push_literal(&mut word, '$', true);
                        i += len;
                    }
                } else {
                    active = true;
                    push_literal(&mut word, c, true);
                    i += len;
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
        Some(format!("'{}'", word))
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
            bare_candidate(word).map(|s| (0usize, s)),
            single_candidate(word).map(|s| (1usize, s)),
            double_candidate(word).map(|s| (2usize, s)),
        ];
        let best = candidates
            .into_iter()
            .flatten()
            .min_by_key(|(priority, s)| (s.len(), *priority));

        let result = if let Some((_, candidate)) = best {
            candidate
        } else {
            let mut fallback = String::from("'");
            for c in word.chars() {
                if c == '\'' {
                    fallback.push('\'');
                    fallback.push('\\');
                    fallback.push('\'');
                    fallback.push('\'');
                } else {
                    fallback.push(c);
                }
            }
            fallback.push('\'');
            fallback
        };
        encoded.push(result);
    }
    Ok(encoded.join(" "))
}

fn main() {}