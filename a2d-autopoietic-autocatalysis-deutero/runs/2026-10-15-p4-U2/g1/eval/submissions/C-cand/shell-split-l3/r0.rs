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

fn name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn name_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn append_literal(word: &mut Word, c: char, wildcard_active: bool) {
    if wildcard_active && matches!(c, '*' | '?' | '[') {
        word.glob.push(word.text.len());
    }
    word.text.push(c);
}

fn append_atomic(word: &mut Word, value: &str, wildcard_active: bool) {
    for c in value.chars() {
        append_literal(word, c, wildcard_active);
    }
}

fn decode_default(text: &str) -> String {
    let mut result = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                result.push(next);
            }
        } else {
            result.push(c);
        }
    }
    result
}

fn first_env_value<'a>(env: &'a [(&'a str, &'a str)], name: &str) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| (*key == name).then_some(*value))
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<Word>, SplitError> {
    if let Some(pos) = input.as_bytes().iter().position(|&b| b == 0) {
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
                    let Some(next) = input[next_i..].chars().next() else {
                        return Err(SplitError::TrailingEscape { pos: i });
                    };
                    let after_next = next_i + next.len_utf8();
                    if matches!(next, '"' | '\\' | '$') {
                        append_literal(&mut word, next, false);
                        i = after_next;
                    } else if next == '\n' {
                        i = after_next;
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
                        let dollar = i;
                        let content_start = next_i + 1;
                        let mut j = content_start;
                        let mut closing = None;
                        while j < input.len() {
                            let sc = input[j..].chars().next().unwrap();
                            let sj = j + sc.len_utf8();
                            if sc == '\\' {
                                if sj < input.len() {
                                    let escaped = input[sj..].chars().next().unwrap();
                                    j = sj + escaped.len_utf8();
                                } else {
                                    j = sj;
                                }
                            } else if sc == '}' {
                                closing = Some(j);
                                break;
                            } else {
                                j = sj;
                            }
                        }
                        let Some(close) = closing else {
                            return Err(SplitError::UnterminatedExpansion { pos: dollar });
                        };
                        let contents = &input[content_start..close];
                        let chars: Vec<char> = contents.chars().collect();
                        let mut n = 0usize;
                        if chars.first().copied().is_some_and(name_start) {
                            n = 1;
                            while n < chars.len() && name_continue(chars[n]) {
                                n += 1;
                            }
                        } else {
                            return Err(SplitError::InvalidExpansion { pos: dollar });
                        }
                        let name: String = chars[..n].iter().collect();
                        let selected = if n == chars.len() {
                            first_env_value(env, &name).unwrap_or("")
                        } else if n + 1 < chars.len() && chars[n] == ':' && chars[n + 1] == '-' {
                            let existing = first_env_value(env, &name).unwrap_or("");
                            if existing.is_empty() {
                                let mut text_start = 0;
                                for ch in &chars[..n + 2] {
                                    text_start += ch.len_utf8();
                                }
                                let default_text = &contents[text_start..];
                                // The closing brace was found by the escape-aware search.
                                // Remove the final unescaped brace from the default slice.
                                let default_end = default_text
                                    .char_indices()
                                    .fold(None, |found, (offset, ch)| {
                                        if ch == '}' {
                                            Some(offset)
                                        } else {
                                            found
                                        }
                                    });
                                if let Some(end) = default_end {
                                    let raw = &default_text[..end];
                                    let decoded = decode_default(raw);
                                    append_atomic(&mut word, &decoded, false);
                                }
                                i = close + 1;
                                active = true;
                                continue;
                            } else {
                                existing
                            }
                        } else {
                            return Err(SplitError::InvalidExpansion { pos: dollar });
                        };
                        append_atomic(&mut word, selected, false);
                        active = true;
                        i = close + 1;
                    } else if input[next_i..].chars().next().is_some_and(name_start) {
                        let mut end = next_i;
                        while end < input.len() {
                            let ch = input[end..].chars().next().unwrap();
                            if !name_continue(ch) {
                                break;
                            }
                            end += ch.len_utf8();
                        }
                        let name = &input[next_i..end];
                        append_atomic(
                            &mut word,
                            first_env_value(env, name).unwrap_or(""),
                            false,
                        );
                        active = true;
                        i = end;
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
                if matches!(c, ' ' | '\t' | '\r' | '\n') {
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
                    let Some(next) = input[next_i..].chars().next() else {
                        return Err(SplitError::TrailingEscape { pos: i });
                    };
                    if next == '\n' {
                        i = next_i + next.len_utf8();
                    } else {
                        append_literal(&mut word, next, false);
                        active = true;
                        i = next_i + next.len_utf8();
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
                        let mut closing = None;
                        while j < input.len() {
                            let sc = input[j..].chars().next().unwrap();
                            let sj = j + sc.len_utf8();
                            if sc == '\\' {
                                if sj < input.len() {
                                    let escaped = input[sj..].chars().next().unwrap();
                                    j = sj + escaped.len_utf8();
                                } else {
                                    j = sj;
                                }
                            } else if sc == '}' {
                                closing = Some(j);
                                break;
                            } else {
                                j = sj;
                            }
                        }
                        let Some(close) = closing else {
                            return Err(SplitError::UnterminatedExpansion { pos: dollar });
                        };
                        let contents = &input[content_start..close];
                        let chars: Vec<char> = contents.chars().collect();
                        let mut n = 0usize;
                        if chars.first().copied().is_some_and(name_start) {
                            n = 1;
                            while n < chars.len() && name_continue(chars[n]) {
                                n += 1;
                            }
                        } else {
                            return Err(SplitError::InvalidExpansion { pos: dollar });
                        }
                        let name: String = chars[..n].iter().collect();
                        if n == chars.len() {
                            append_atomic(
                                &mut word,
                                first_env_value(env, &name).unwrap_or(""),
                                true,
                            );
                        } else if n + 1 < chars.len() && chars[n] == ':' && chars[n + 1] == '-' {
                            let existing = first_env_value(env, &name).unwrap_or("");
                            if existing.is_empty() {
                                let default_start: usize =
                                    chars[..n + 2].iter().map(|ch| ch.len_utf8()).sum();
                                let raw = &contents[default_start..];
                                let decoded = decode_default(raw);
                                append_atomic(&mut word, &decoded, true);
                            } else {
                                append_atomic(&mut word, existing, true);
                            }
                        } else {
                            return Err(SplitError::InvalidExpansion { pos: dollar });
                        }
                        active = true;
                        i = close + 1;
                    } else if input[next_i..].chars().next().is_some_and(name_start) {
                        let mut end = next_i;
                        while end < input.len() {
                            let ch = input[end..].chars().next().unwrap();
                            if !name_continue(ch) {
                                break;
                            }
                            end += ch.len_utf8();
                        }
                        let name = &input[next_i..end];
                        append_atomic(
                            &mut word,
                            first_env_value(env, name).unwrap_or(""),
                            true,
                        );
                        active = true;
                        i = end;
                    } else {
                        word.text.push('$');
                        active = true;
                        i = next_i;
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
        Quote::Single => return Err(SplitError::UnterminatedSingle { pos: quote_pos }),
        Quote::Double => return Err(SplitError::UnterminatedDouble { pos: quote_pos }),
        Quote::None => {}
    }

    if active {
        words.push(word);
    }
    Ok(words)
}

fn main() {}