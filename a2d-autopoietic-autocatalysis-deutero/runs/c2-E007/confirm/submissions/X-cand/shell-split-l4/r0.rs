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

fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn push_literal(word: &mut Word, c: char, active_glob: bool) {
    if active_glob && matches!(c, '*' | '?' | '[') {
        word.glob.push(word.text.len());
    }
    word.text.push(c);
}

fn push_expansion(word: &mut Word, value: &str, active_glob: bool) {
    for c in value.chars() {
        push_literal(word, c, active_glob);
    }
}

fn lookup<'a>(env: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| (*key == name).then_some(*value))
}

fn decode_default(text: &str) -> String {
    let mut result = String::new();
    let mut chars = text.char_indices().peekable();

    while let Some((_, c)) = chars.next() {
        if c == '\\' {
            if let Some((_, next)) = chars.next() {
                result.push(next);
            } else {
                result.push('\\');
            }
        } else {
            result.push(c);
        }
    }

    result
}

fn parse_braced(
    input: &str,
    dollar_pos: usize,
    after_dollar: usize,
    env: &[(&str, &str)],
) -> Result<(String, usize), SplitError> {
    let mut close = None;
    let mut search = after_dollar + 1; // Skip '{'.

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

    let close = close.ok_or(SplitError::UnterminatedExpansion { pos: dollar_pos })?;
    let contents = &input[after_dollar + 1..close];

    let mut name_end = 0;
    let mut chars = contents.char_indices();
    let valid_name = match chars.next() {
        Some((_, c)) if is_name_start(c) => {
            name_end = c.len_utf8();
            for (idx, c) in chars {
                if is_name_char(c) {
                    name_end = idx + c.len_utf8();
                } else {
                    break;
                }
            }
            true
        }
        _ => false,
    };

    if !valid_name {
        return Err(SplitError::InvalidExpansion { pos: dollar_pos });
    }

    let name = &contents[..name_end];
    let tail = &contents[name_end..];

    if tail.is_empty() {
        Ok((lookup(env, name).unwrap_or("").to_string(), close + 1))
    } else if let Some(default) = tail.strip_prefix(":-") {
        let value = lookup(env, name).unwrap_or("");
        if value.is_empty() {
            Ok((decode_default(default), close + 1))
        } else {
            Ok((value.to_string(), close + 1))
        }
    } else {
        Err(SplitError::InvalidExpansion { pos: dollar_pos })
    }
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
    let mut quote_pos = 0;
    let mut i = 0;

    while i < input.len() {
        let c = input[i..].chars().next().unwrap();
        let next_i = i + c.len_utf8();

        match quote {
            Quote::Single => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    push_literal(&mut current, c, false);
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
                    let next = input[next_i..].chars().next().unwrap();
                    if matches!(next, '"' | '\\' | '$' | '\n') {
                        if next != '\n' {
                            push_literal(&mut current, next, false);
                        }
                        i = next_i + next.len_utf8();
                    } else {
                        current.text.push('\\');
                        i = next_i;
                    }
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if next_i < input.len() {
                        let next = input[next_i..].chars().next().unwrap();
                        if next == '(' {
                            return Err(SplitError::CommandSubstitution { pos: i });
                        }
                        if next == '{' {
                            let (value, end) = parse_braced(input, i, next_i, env)?;
                            active = true;
                            push_expansion(&mut current, &value, false);
                            i = end;
                            continue;
                        }
                        if is_name_start(next) {
                            let mut end = next_i;
                            while end < input.len() {
                                let ch = input[end..].chars().next().unwrap();
                                if !is_name_char(ch) {
                                    break;
                                }
                                end += ch.len_utf8();
                            }
                            active = true;
                            push_expansion(
                                &mut current,
                                lookup(env, &input[next_i..end]).unwrap_or(""),
                                false,
                            );
                            i = end;
                            continue;
                        }
                    }
                    current.text.push('$');
                    i = next_i;
                } else {
                    push_literal(&mut current, c, false);
                    i = next_i;
                }
            }
            Quote::None => {
                if is_space(c) {
                    if active {
                        words.push(current);
                        current = Word {
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
                    let next = input[next_i..].chars().next().unwrap();
                    active |= next != '\n';
                    if next != '\n' {
                        push_literal(&mut current, next, false);
                    }
                    i = next_i + next.len_utf8();
                } else if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos: i });
                } else if c == '$' {
                    if next_i < input.len() {
                        let next = input[next_i..].chars().next().unwrap();
                        if next == '(' {
                            return Err(SplitError::CommandSubstitution { pos: i });
                        }
                        if next == '{' {
                            let (value, end) = parse_braced(input, i, next_i, env)?;
                            active = true;
                            push_expansion(&mut current, &value, true);
                            i = end;
                            continue;
                        }
                        if is_name_start(next) {
                            let mut end = next_i;
                            while end < input.len() {
                                let ch = input[end..].chars().next().unwrap();
                                if !is_name_char(ch) {
                                    break;
                                }
                                end += ch.len_utf8();
                            }
                            active = true;
                            push_expansion(
                                &mut current,
                                lookup(env, &input[next_i..end]).unwrap_or(""),
                                true,
                            );
                            i = end;
                            continue;
                        }
                    }
                    active = true;
                    current.text.push('$');
                    i = next_i;
                } else {
                    active = true;
                    push_literal(&mut current, c, true);
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
        words.push(current);
    }
    Ok(words)
}

fn bare_candidate(word: &str) -> Option<String> {
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

    let mut out = String::new();
    out.push('"');
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
    let mut out = String::new();
    out.push('\'');
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
    let candidates = [
        bare_candidate(word),
        single_candidate(word),
        double_candidate(word),
    ];

    let mut best: Option<(usize, String)> = None;
    for candidate in candidates.into_iter().flatten() {
        let len = candidate.len();
        if best.as_ref().is_none_or(|(best_len, _)| len < *best_len) {
            best = Some((len, candidate));
        }
    }

    best.map(|(_, candidate)| candidate)
        .unwrap_or_else(|| fallback_candidate(word))
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

    let mut result = String::new();
    for (index, word) in words.iter().enumerate() {
        if index > 0 {
            result.push(' ');
        }
        result.push_str(&encode_word(word));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_basic_words() {
        assert_eq!(
            split("alpha 'two words' x\"y\" ''", &[]),
            Ok(vec![
                Word {
                    text: "alpha".to_string(),
                    glob: vec![]
                },
                Word {
                    text: "two words".to_string(),
                    glob: vec![]
                },
                Word {
                    text: "xy".to_string(),
                    glob: vec![]
                },
                Word {
                    text: "".to_string(),
                    glob: vec![]
                }
            ])
        );
    }

    #[test]
    fn public_escapes() {
        assert_eq!(
            split("a\\ b \"c\\\"d\"", &[]),
            Ok(vec![
                Word {
                    text: "a b".to_string(),
                    glob: vec![]
                },
                Word {
                    text: "c\"d".to_string(),
                    glob: vec![]
                }
            ])
        );
    }

    #[test]
    fn public_glob_command_expansion() {
        assert_eq!(
            split(
                "a* $X",
                &[("X", "ok")]
            ),
            Ok(vec![
                Word {
                    text: "a*".to_string(),
                    glob: vec![1]
                },
                Word {
                    text: "ok".to_string(),
                    glob: vec![]
                }
            ])
        );
        assert_eq!(
            split("`x`", &[]),
            Err(SplitError::CommandSubstitution { pos: 0 })
        );
    }

    #[test]
    fn public_join() {
        assert_eq!(
            join(&vec!["alpha".to_string(), "two words".to_string()]),
            Ok("alpha two\\ words".to_string())
        );
        assert_eq!(
            split("alpha two\\ words", &[]),
            Ok(vec![
                Word {
                    text: "alpha".to_string(),
                    glob: vec![]
                },
                Word {
                    text: "two words".to_string(),
                    glob: vec![]
                }
            ])
        );
        assert_eq!(
            join(&vec!["\0".into()]),
            Err(JoinError::Nul { word: 0, pos: 0 })
        );
    }
}