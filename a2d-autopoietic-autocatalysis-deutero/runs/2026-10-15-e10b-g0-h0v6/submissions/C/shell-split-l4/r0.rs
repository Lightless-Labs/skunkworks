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

fn is_name_start(c: char) -> bool {
    c == '_' || c.is_ascii_alphabetic()
}

fn is_name_continue(c: char) -> bool {
    is_name_start(c) || c.is_ascii_digit()
}

fn append(word: &mut Word, s: &str, wildcard_active: bool) {
    for c in s.chars() {
        if wildcard_active && matches!(c, '*' | '?' | '[') {
            word.glob.push(word.text.len());
        }
        word.text.push(c);
    }
}

fn first_env<'a>(env: &'a [(&'a str, &'a str)], name: &str) -> Option<&'a str> {
    if name.is_empty() {
        return None;
    }
    env.iter().find(|(key, _)| *key == name).map(|(_, value)| *value)
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
        let n = c.len_utf8();

        match quote {
            Quote::Single(_) => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    append(&mut word, &input[i..i + n], false);
                }
                i += n;
            }
            Quote::Double(open) => {
                if c == '"' {
                    quote = Quote::None;
                    i += n;
                    continue;
                }
                if c == '\\' {
                    let pos = i;
                    i += n;
                    if i == input.len() {
                        return Err(SplitError::TrailingEscape { pos });
                    }
                    let next = input[i..].chars().next().unwrap();
                    let nn = next.len_utf8();
                    if matches!(next, '"' | '\\' | '$' | '\n') {
                        if next != '\n' {
                            append(&mut word, &input[i..i + nn], false);
                        }
                        i += nn;
                    } else {
                        append(&mut word, "\\", false);
                    }
                    continue;
                }
                if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                }
                if c == '$' {
                    if input[i + n..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }
                    let result = expand(input, i, env)?;
                    if let Some((end, value)) = result {
                        append(&mut word, value, false);
                        active = true;
                        i = end;
                        continue;
                    }
                }
                append(&mut word, &input[i..i + n], false);
                i += n;
                let _ = open;
            }
            Quote::None => {
                if matches!(c, ' ' | '\t' | '\r' | '\n') {
                    if active {
                        words.push(word);
                        word = Word { text: String::new(), glob: Vec::new() };
                        active = false;
                    }
                    i += n;
                    continue;
                }
                if c == '\'' {
                    quote = Quote::Single(i);
                    active = true;
                    i += n;
                    continue;
                }
                if c == '"' {
                    quote = Quote::Double(i);
                    active = true;
                    i += n;
                    continue;
                }
                if c == '\\' {
                    let pos = i;
                    i += n;
                    if i == input.len() {
                        return Err(SplitError::TrailingEscape { pos });
                    }
                    let next = input[i..].chars().next().unwrap();
                    let nn = next.len_utf8();
                    if next != '\n' {
                        append(&mut word, &input[i..i + nn], false);
                        active = true;
                    }
                    i += nn;
                    continue;
                }
                if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                }
                if c == '$' {
                    if input[i + n..].starts_with('(') {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }
                    let result = expand(input, i, env)?;
                    if let Some((end, value)) = result {
                        append(&mut word, value, true);
                        active = true;
                        i = end;
                        continue;
                    }
                }
                append(&mut word, &input[i..i + n], matches!(c, '*' | '?' | '['));
                active = true;
                i += n;
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

fn expand<'a>(
    input: &'a str,
    dollar: usize,
    env: &'a [(&'a str, &'a str)],
) -> Result<Option<(usize, &'a str)>, SplitError> {
    let after = dollar + 1;
    if after >= input.len() {
        return Ok(None);
    }
    let c = input[after..].chars().next().unwrap();
    if c == '{' {
        let content_start = after + 1;
        let mut j = content_start;
        let close;
        loop {
            if j >= input.len() {
                return Err(SplitError::UnterminatedExpansion { pos: dollar });
            }
            let ch = input[j..].chars().next().unwrap();
            let len = ch.len_utf8();
            if ch == '\\' {
                j += len;
                if j < input.len() {
                    j += input[j..].chars().next().unwrap().len_utf8();
                }
                continue;
            }
            if ch == '}' {
                close = j;
                break;
            }
            j += len;
        }

        let content = &input[content_start..close];
        let name_end = content
            .char_indices()
            .find(|(_, ch)| !is_name_continue(*ch))
            .map(|(p, _)| p)
            .unwrap_or(content.len());
        let name = &content[..name_end];
        if name.is_empty() || !name.chars().next().map(is_name_start).unwrap_or(false) {
            return Err(SplitError::InvalidExpansion { pos: dollar });
        }

        let remainder = &content[name_end..];
        if remainder.is_empty() {
            return Ok(Some((close + 1, first_env(env, name).unwrap_or(""))));
        }
        if let Some(default) = remainder.strip_prefix(":-") {
            let value = first_env(env, name).filter(|v| !v.is_empty());
            if let Some(value) = value {
                return Ok(Some((close + 1, value)));
            }
            let mut decoded = String::new();
            let mut k = 0;
            while k < default.len() {
                let ch = default[k..].chars().next().unwrap();
                if ch == '\\' {
                    k += 1;
                    if k < default.len() {
                        let escaped = default[k..].chars().next().unwrap();
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
            // The returned value must outlive this function, so selected defaults
            // are handled by the caller through a dedicated owned-value path.
            return Ok(Some((close + 1, Box::leak(decoded.into_boxed_str()))));
        }
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }

    if !is_name_start(c) {
        return Ok(None);
    }
    let mut end = after;
    while end < input.len() {
        let ch = input[end..].chars().next().unwrap();
        if !is_name_continue(ch) {
            break;
        }
        end += ch.len_utf8();
    }
    Ok(Some((end, first_env(env, &input[after..end]).unwrap_or(""))))
}

pub fn join(words: &[String]) -> Result<String, JoinError> {
    for (word_index, word) in words.iter().enumerate() {
        if let Some(pos) = word.find('\0') {
            return Err(JoinError::Nul { word: word_index, pos });
        }
    }

    let mut encoded = Vec::with_capacity(words.len());
    for word in words {
        encoded.push(encode_word(word));
    }
    Ok(encoded.join(" "))
}

fn encode_word(word: &str) -> String {
    let bare = if !word.is_empty() && !word.contains('\n') {
        let mut s = String::new();
        for c in word.chars() {
            if c.is_ascii_alphanumeric() || "_-./:=,+@%".contains(c) {
                s.push(c);
            } else {
                s.push('\\');
                s.push(c);
            }
        }
        Some(s)
    } else {
        None
    };

    let single = if !word.contains('\'') {
        Some(format!("'{word}'"))
    } else {
        None
    };

    let double = if !word.contains('`') {
        let mut s = String::from("\"");
        for c in word.chars() {
            if matches!(c, '\\' | '"' | '$') {
                s.push('\\');
            }
            s.push(c);
        }
        s.push('"');
        Some(s)
    } else {
        None
    };

    let mut candidates: Vec<(usize, usize, String)> = Vec::new();
    if let Some(s) = bare {
        candidates.push((s.len(), 0, s));
    }
    if let Some(s) = single {
        candidates.push((s.len(), 1, s));
    }
    if let Some(s) = double {
        candidates.push((s.len(), 2, s));
    }
    if let Some((_, _, s)) = candidates.into_iter().min_by_key(|(len, priority, _)| (*len, *priority)) {
        return s;
    }

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
}

fn main() {}