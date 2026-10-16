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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Quote {
    None,
    Single,
    Double,
}

fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_name_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn append_scalar(word: &mut Word, c: char, active_glob: bool) {
    if active_glob && matches!(c, '*' | '?' | '[') {
        word.glob.push(word.text.len());
    }
    word.text.push(c);
}

fn append_string(word: &mut Word, value: &str, active_glob: bool) {
    for c in value.chars() {
        append_scalar(word, c, active_glob);
    }
}

fn lookup<'a>(env: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| (*key == name).then_some(*value))
}

fn decode_default(text: &str) -> String {
    let mut output = String::new();
    let mut chars = text.char_indices().peekable();

    while let Some((_, c)) = chars.next() {
        if c == '\\' {
            if let Some((_, escaped)) = chars.next() {
                output.push(escaped);
            } else {
                output.push('\\');
            }
        } else {
            output.push(c);
        }
    }

    output
}

fn split_inner(input: &str, env: &[(&str, &str)]) -> Result<Vec<Word>, SplitError> {
    let mut words = Vec::new();
    let mut word = Word {
        text: String::new(),
        glob: Vec::new(),
    };
    let mut active = false;
    let mut quote = Quote::None;
    let mut quote_start = 0usize;
    let mut i = 0usize;

    while i < input.len() {
        let c = input[i..].chars().next().expect("valid UTF-8 boundary");
        let next_i = i + c.len_utf8();

        match quote {
            Quote::Single => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    append_scalar(&mut word, c, false);
                }
                i = next_i;
            }
            Quote::Double => {
                if c == '"' {
                    quote = Quote::None;
                    i = next_i;
                } else if c == '\\' {
                    if next_i >= input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let escaped = input[next_i..].chars().next().expect("nonempty suffix");
                    let after_escaped = next_i + escaped.len_utf8();
                    if matches!(escaped, '"' | '\\' | '$') {
                        append_scalar(&mut word, escaped, false);
                        i = after_escaped;
                    } else if escaped == '\n' {
                        i = after_escaped;
                    } else {
                        append_scalar(&mut word, '\\', false);
                        i = next_i;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if next_i < input.len()
                        && input[next_i..].starts_with('(')
                    {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }
                    if next_i < input.len()
                        && input[next_i..].starts_with('{')
                    {
                        let (value, end) = parse_braced(input, i, env)?;
                        append_string(&mut word, &value, false);
                        active = true;
                        i = end;
                    } else if next_i < input.len() {
                        let following = input[next_i..].chars().next().expect("nonempty suffix");
                        if is_name_start(following) {
                            let (name, end) = parse_name(input, next_i);
                            let value = lookup(env, name).unwrap_or("");
                            append_string(&mut word, value, false);
                            active = true;
                            i = end;
                        } else {
                            append_scalar(&mut word, '$', false);
                            i = next_i;
                        }
                    } else {
                        append_scalar(&mut word, '$', false);
                        i = next_i;
                    }
                } else {
                    append_scalar(&mut word, c, false);
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
                    active = true;
                    quote = Quote::Single;
                    quote_start = i;
                    i = next_i;
                } else if c == '"' {
                    active = true;
                    quote = Quote::Double;
                    quote_start = i;
                    i = next_i;
                } else if c == '\\' {
                    if next_i >= input.len() {
                        return Err(SplitError::TrailingEscape { pos: i });
                    }
                    let escaped = input[next_i..].chars().next().expect("nonempty suffix");
                    let after_escaped = next_i + escaped.len_utf8();
                    if escaped != '\n' {
                        append_scalar(&mut word, escaped, false);
                        active = true;
                    }
                    i = after_escaped;
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if next_i < input.len()
                        && input[next_i..].starts_with('(')
                    {
                        return Err(SplitError::CommandSubstitution { pos: i });
                    }
                    if next_i < input.len()
                        && input[next_i..].starts_with('{')
                    {
                        let (value, end) = parse_braced(input, i, env)?;
                        append_string(&mut word, &value, true);
                        active = true;
                        i = end;
                    } else if next_i < input.len() {
                        let following = input[next_i..].chars().next().expect("nonempty suffix");
                        if is_name_start(following) {
                            let (name, end) = parse_name(input, next_i);
                            let value = lookup(env, name).unwrap_or("");
                            append_string(&mut word, value, true);
                            active = true;
                            i = end;
                        } else {
                            append_scalar(&mut word, '$', false);
                            active = true;
                            i = next_i;
                        }
                    } else {
                        append_scalar(&mut word, '$', false);
                        active = true;
                        i = next_i;
                    }
                } else {
                    append_scalar(&mut word, c, matches!(c, '*' | '?' | '['));
                    active = true;
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

    if active {
        words.push(word);
    }

    Ok(words)
}

fn parse_name(input: &str, start: usize) -> (&str, usize) {
    let mut end = start;
    for (offset, c) in input[start..].char_indices() {
        if is_name_continue(c) {
            end = start + offset + c.len_utf8();
        } else {
            break;
        }
    }
    (&input[start..end], end)
}

fn parse_braced(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
) -> Result<(String, usize), SplitError> {
    let content_start = dollar + 2;
    let mut cursor = content_start;
    let mut closing = None;

    while cursor < input.len() {
        let c = input[cursor..].chars().next().expect("valid UTF-8 boundary");
        let next = cursor + c.len_utf8();
        if c == '\\' {
            if next < input.len() {
                let escaped = input[next..].chars().next().expect("nonempty suffix");
                cursor = next + escaped.len_utf8();
            } else {
                cursor = next;
            }
        } else if c == '}' {
            closing = Some(cursor);
            break;
        } else {
            cursor = next;
        }
    }

    let close = closing.ok_or(SplitError::UnterminatedExpansion { pos: dollar })?;
    let contents = &input[content_start..close];

    let mut name_end = 0usize;
    let mut chars = contents.char_indices();
    match chars.next() {
        Some((_, first)) if is_name_start(first) => {
            name_end = first.len_utf8();
        }
        _ => return Err(SplitError::InvalidExpansion { pos: dollar }),
    }

    for (offset, c) in contents[name_end..].char_indices() {
        if is_name_continue(c) {
            name_end += offset + c.len_utf8();
        } else {
            break;
        }
    }

    let name = &contents[..name_end];
    let rest = &contents[name_end..];

    if rest.is_empty() {
        Ok((lookup(env, name).unwrap_or("").to_string(), close + 1))
    } else if let Some(default_text) = rest.strip_prefix(":-") {
        let value = lookup(env, name).unwrap_or("");
        if value.is_empty() {
            Ok((decode_default(default_text), close + 1))
        } else {
            Ok((value.to_string(), close + 1))
        }
    } else {
        Err(SplitError::InvalidExpansion { pos: dollar })
    }
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<Word>, SplitError> {
    if let Some(pos) = input.find('\0') {
        return Err(SplitError::Nul { pos });
    }
    split_inner(input, env)
}

fn bare_candidate(word: &str) -> Option<String> {
    if word.is_empty() || word.contains('\n') {
        return None;
    }

    let mut output = String::new();
    for c in word.chars() {
        if c.is_ascii_alphanumeric() || "_-./:=,+@%".contains(c) {
            output.push(c);
        } else {
            output.push('\\');
            output.push(c);
        }
    }
    Some(output)
}

fn single_candidate(word: &str) -> Option<String> {
    if word.contains('\'') {
        return None;
    }
    let mut output = String::with_capacity(word.len() + 2);
    output.push('\'');
    output.push_str(word);
    output.push('\'');
    Some(output)
}

fn double_candidate(word: &str) -> Option<String> {
    if word.contains('`') {
        return None;
    }
    let mut output = String::with_capacity(word.len() + 2);
    output.push('"');
    for c in word.chars() {
        if matches!(c, '\\' | '"' | '$') {
            output.push('\\');
        }
        output.push(c);
    }
    output.push('"');
    Some(output)
}

fn fallback_candidate(word: &str) -> String {
    let mut output = String::with_capacity(word.len() + 2);
    output.push('\'');
    for c in word.chars() {
        if c == '\'' {
            output.push('\'');
            output.push('\\');
            output.push('\'');
            output.push('\'');
        } else {
            output.push(c);
        }
    }
    output.push('\'');
    output
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
        let mut candidates = Vec::new();
        if let Some(candidate) = bare_candidate(word) {
            candidates.push((candidate, 0usize));
        }
        if let Some(candidate) = single_candidate(word) {
            candidates.push((candidate, 1usize));
        }
        if let Some(candidate) = double_candidate(word) {
            candidates.push((candidate, 2usize));
        }

        let chosen = if candidates.is_empty() {
            fallback_candidate(word)
        } else {
            candidates
                .into_iter()
                .min_by_key(|(candidate, priority)| (candidate.len(), *priority))
                .expect("nonempty candidates")
                .0
        };

        encoded.push(chosen);
    }

    Ok(encoded.join(" "))
}