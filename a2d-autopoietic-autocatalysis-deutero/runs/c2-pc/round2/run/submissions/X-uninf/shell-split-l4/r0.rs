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

fn is_name_start(c: char) -> bool {
    c == '_' || c.is_ascii_alphabetic()
}

fn is_name_continue(c: char) -> bool {
    is_name_start(c) || c.is_ascii_digit()
}

fn append_literal(word: &mut Word, c: char, wildcard_active: bool) {
    let offset = word.text.len();
    word.text.push(c);
    if wildcard_active && matches!(c, '*' | '?' | '[') {
        word.glob.push(offset);
    }
}

fn lookup<'a>(name: &str, env: &'a [(&str, &str)]) -> Option<&'a str> {
    env.iter().find(|(key, _)| *key == name).map(|(_, value)| *value)
}

fn decode_default(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.char_indices().peekable();
    while let Some((_, c)) = chars.next() {
        if c == '\\' {
            if let Some((_, next)) = chars.next() {
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
    let mut i = dollar + 2;
    let mut close = None;

    while i < input.len() {
        let c = input[i..].chars().next().unwrap();
        if c == '\\' {
            i += 1;
            if i < input.len() {
                i += input[i..].chars().next().unwrap().len_utf8();
            }
        } else if c == '}' {
            close = Some(i);
            break;
        } else {
            i += c.len_utf8();
        }
    }

    let close = close.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
    let content = &input[dollar + 2..close];
    let chars: Vec<(usize, char)> = content.char_indices().collect();

    let Some((_, first)) = chars.first().copied() else {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    };
    if !is_name_start(first) {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    }

    let mut name_end = first.len_utf8();
    while name_end < content.len() {
        let c = content[name_end..].chars().next().unwrap();
        if is_name_continue(c) {
            name_end += c.len_utf8();
        } else {
            break;
        }
    }

    let name = &content[..name_end];
    let suffix = &content[name_end..];
    let value = if suffix.is_empty() {
        lookup(name, env).unwrap_or("").to_string()
    } else if let Some(default) = suffix.strip_prefix(":-") {
        match lookup(name, env) {
            Some(value) if !value.is_empty() => value.to_string(),
            _ => decode_default(default),
        }
    } else {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    };

    Ok((close + 1, value))
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<Word>, SplitError> {
    if let Some(pos) = input.as_bytes().iter().position(|&b| b == 0) {
        return Err(SplitError::Nul { pos });
    }

    let mut words = Vec::new();
    let mut current: Option<Word> = None;
    let mut i = 0;
    let mut quote: Option<(char, usize)> = None;

    while i < input.len() {
        let c = input[i..].chars().next().unwrap();
        let next_i = i + c.len_utf8();

        if let Some((quote_char, _)) = quote {
            match quote_char {
                '\'' => {
                    if c == '\'' {
                        quote = None;
                    } else {
                        append_literal(current.as_mut().unwrap(), c, false);
                    }
                    i = next_i;
                }
                '"' => {
                    if c == '"' {
                        quote = None;
                        i = next_i;
                    } else if c == '\\' {
                        let slash = i;
                        if next_i == input.len() {
                            return Err(SplitError::TrailingEscape { pos: slash });
                        }
                        let escaped = input[next_i..].chars().next().unwrap();
                        let after = next_i + escaped.len_utf8();
                        if matches!(escaped, '"' | '\\' | '$') {
                            append_literal(current.as_mut().unwrap(), escaped, false);
                            i = after;
                        } else if escaped == '\n' {
                            i = after;
                        } else {
                            append_literal(current.as_mut().unwrap(), '\\', false);
                            i = next_i;
                        }
                    } else if c == '$' {
                        if input[next_i..].starts_with('(') {
                            return Err(SplitError::CommandSubstitution { pos: i });
                        }
                        if input[next_i..].starts_with('{') {
                            let (after, value) = parse_braced(input, i, env)?;
                            for ch in value.chars() {
                                append_literal(current.as_mut().unwrap(), ch, false);
                            }
                            i = after;
                        } else if next_i < input.len()
                            && is_name_start(input[next_i..].chars().next().unwrap())
                        {
                            let mut end = next_i;
                            while end < input.len() {
                                let ch = input[end..].chars().next().unwrap();
                                if is_name_continue(ch) {
                                    end += ch.len_utf8();
                                } else {
                                    break;
                                }
                            }
                            let value = lookup(&input[next_i..end], env).unwrap_or("");
                            for ch in value.chars() {
                                append_literal(current.as_mut().unwrap(), ch, false);
                            }
                            i = end;
                        } else {
                            append_literal(current.as_mut().unwrap(), '$', false);
                            i = next_i;
                        }
                    } else if c == '`' {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    } else {
                        append_literal(current.as_mut().unwrap(), c, false);
                        i = next_i;
                    }
                }
                _ => unreachable!(),
            }
            continue;
        }

        match c {
            ' ' | '\t' | '\r' | '\n' => {
                if let Some(word) = current.take() {
                    words.push(word);
                }
                i = next_i;
            }
            '\'' | '"' => {
                current.get_or_insert_with(|| Word {
                    text: String::new(),
                    glob: Vec::new(),
                });
                quote = Some((c, i));
                i = next_i;
            }
            '\\' => {
                if next_i == input.len() {
                    return Err(SplitError::TrailingEscape { pos: i });
                }
                let escaped = input[next_i..].chars().next().unwrap();
                let after = next_i + escaped.len_utf8();
                if escaped == '\n' {
                    i = after;
                } else {
                    let word = current.get_or_insert_with(|| Word {
                        text: String::new(),
                        glob: Vec::new(),
                    });
                    append_literal(word, escaped, false);
                    i = after;
                }
            }
            '$' => {
                if input[next_i..].starts_with('(') {
                    return Err(SplitError::CommandSubstitution { pos: i });
                }
                let word = current.get_or_insert_with(|| Word {
                    text: String::new(),
                    glob: Vec::new(),
                });
                if input[next_i..].starts_with('{') {
                    let (after, value) = parse_braced(input, i, env)?;
                    for ch in value.chars() {
                        append_literal(word, ch, true);
                    }
                    i = after;
                } else if next_i < input.len()
                    && is_name_start(input[next_i..].chars().next().unwrap())
                {
                    let mut end = next_i;
                    while end < input.len() {
                        let ch = input[end..].chars().next().unwrap();
                        if is_name_continue(ch) {
                            end += ch.len_utf8();
                        } else {
                            break;
                        }
                    }
                    let value = lookup(&input[next_i..end], env).unwrap_or("");
                    for ch in value.chars() {
                        append_literal(word, ch, true);
                    }
                    i = end;
                } else {
                    append_literal(word, '$', true);
                    i = next_i;
                }
            }
            '`' => return Err(SplitError::CommandSubstitution { pos: i }),
            _ => {
                let word = current.get_or_insert_with(|| Word {
                    text: String::new(),
                    glob: Vec::new(),
                });
                append_literal(word, c, true);
                i = next_i;
            }
        }
    }

    if let Some((quote_char, pos)) = quote {
        return Err(if quote_char == '\'' {
            SplitError::UnterminatedSingle { pos }
        } else {
            SplitError::UnterminatedDouble { pos }
        });
    }

    if let Some(word) = current {
        words.push(word);
    }
    Ok(words)
}

fn encode_bare(word: &str) -> Option<String> {
    if word.is_empty() || word.contains('\n') {
        return None;
    }

    let mut out = String::new();
    for c in word.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | ':' | '=' | ',' | '+' | '@' | '%') {
            out.push(c);
        } else {
            out.push('\\');
            out.push(c);
        }
    }
    Some(out)
}

fn encode_single(word: &str) -> Option<String> {
    if word.contains('\'') {
        None
    } else {
        Some(format!("'{}'", word))
    }
}

fn encode_double(word: &str) -> Option<String> {
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

fn encode_fallback(word: &str) -> String {
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
        if let Some(pos) = word.as_bytes().iter().position(|&b| b == 0) {
            return Err(JoinError::Nul {
                word: word_index,
                pos,
            });
        }
    }

    let mut encoded = Vec::with_capacity(words.len());
    for word in words {
        let mut candidates = Vec::new();
        if let Some(candidate) = encode_bare(word) {
            candidates.push((candidate.len(), 0, candidate));
        }
        if let Some(candidate) = encode_single(word) {
            candidates.push((candidate.len(), 1, candidate));
        }
        if let Some(candidate) = encode_double(word) {
            candidates.push((candidate.len(), 2, candidate));
        }

        let chosen = if let Some((_, _, candidate)) =
            candidates.into_iter().min_by_key(|(len, priority, _)| (*len, *priority))
        {
            candidate
        } else {
            encode_fallback(word)
        };
        encoded.push(chosen);
    }

    Ok(encoded.join(" "))
}