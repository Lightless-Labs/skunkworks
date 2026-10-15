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

fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

fn is_name_start(c: char) -> bool {
    c == '_' || c.is_ascii_alphabetic()
}

fn is_name_continue(c: char) -> bool {
    is_name_start(c) || c.is_ascii_digit()
}

fn env_value<'a>(env: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| (*key == name).then_some(*value))
}

fn append(word: &mut Word, value: &str, active_glob: bool) {
    for c in value.chars() {
        if active_glob && matches!(c, '*' | '?' | '[') {
            word.glob.push(word.text.len());
        }
        word.text.push(c);
    }
}

fn char_at(input: &str, pos: usize) -> Option<(char, usize)> {
    input[pos..].chars().next().map(|c| (c, c.len_utf8()))
}

fn decode_default(input: &str) -> String {
    let mut out = String::new();
    let mut pos = 0;
    while pos < input.len() {
        let (c, n) = char_at(input, pos).unwrap();
        if c == '\\' {
            let next_pos = pos + n;
            if next_pos < input.len() {
                let (next, nn) = char_at(input, next_pos).unwrap();
                out.push(next);
                pos = next_pos + nn;
            } else {
                out.push('\\');
                pos += n;
            }
        } else {
            out.push(c);
            pos += n;
        }
    }
    out
}

fn parse_braced(input: &str, dollar: usize) -> Result<(usize, String, bool), SplitError> {
    let content_start = dollar + 2;
    let mut pos = content_start;

    while pos < input.len() {
        let (c, n) = char_at(input, pos).unwrap();
        if c == '\\' {
            pos += n;
            if pos < input.len() {
                let (_, nn) = char_at(input, pos).unwrap();
                pos += nn;
            }
        } else if c == '}' {
            break;
        } else {
            pos += n;
        }
    }

    if pos >= input.len() {
        return Err(SplitError::UnterminatedExpansion { pos: dollar });
    }

    let contents = &input[content_start..pos];
    let mut name_end = 0;
    let mut chars = contents.char_indices();
    match chars.next() {
        Some((_, c)) if is_name_start(c) => name_end = c.len_utf8(),
        _ => return Err(SplitError::InvalidExpansion { pos: dollar }),
    }
    for (i, c) in chars {
        if is_name_continue(c) {
            name_end = i + c.len_utf8();
        } else {
            break;
        }
    }

    let name = &contents[..name_end];
    let rest = &contents[name_end..];

    if rest.is_empty() {
        Ok((pos + 1, name.to_string(), false))
    } else if let Some(default) = rest.strip_prefix(":-") {
        let value = env_value(&[], ""); // Keep parsing independent of the environment.
        let _ = value;
        Ok((pos + 1, format!("{name}\0{default}"), true))
    } else {
        Err(SplitError::InvalidExpansion { pos: dollar })
    }
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
        let (c, n) = char_at(input, pos).unwrap();

        if quote == Quote::Single {
            if c == '\'' {
                quote = Quote::None;
            } else {
                word.text.push(c);
            }
            pos += n;
            continue;
        }

        if quote == Quote::None && is_separator(c) {
            if active {
                words.push(word);
                word = Word {
                    text: String::new(),
                    glob: Vec::new(),
                };
                active = false;
            }
            pos += n;
            continue;
        }

        if c == '\\' {
            let slash_pos = pos;
            pos += n;
            if pos >= input.len() {
                return Err(SplitError::TrailingEscape { pos: slash_pos });
            }
            let (next, nn) = char_at(input, pos).unwrap();
            if quote == Quote::None {
                if next != '\n' {
                    active = true;
                    word.text.push(next);
                }
                pos += nn;
            } else {
                match next {
                    '"' | '\\' | '$' => {
                        active = true;
                        word.text.push(next);
                        pos += nn;
                    }
                    '\n' => {
                        pos += nn;
                    }
                    _ => {
                        active = true;
                        word.text.push('\\');
                    }
                }
            }
            continue;
        }

        if quote == Quote::None && c == '\'' {
            quote = Quote::Single;
            quote_start = pos;
            active = true;
            pos += n;
            continue;
        }

        if c == '"' {
            if quote == Quote::None {
                quote = Quote::Double;
                quote_start = pos;
                active = true;
            } else {
                quote = Quote::None;
            }
            pos += n;
            continue;
        }

        if c == '`' {
            return Err(SplitError::CommandSubstitution { pos });
        }

        if c == '$' {
            let next_pos = pos + n;
            if next_pos < input.len() {
                let (next, nn) = char_at(input, next_pos).unwrap();
                if next == '(' {
                    return Err(SplitError::CommandSubstitution { pos });
                }
                if next == '{' {
                    let (end, packed, has_default) = parse_braced(input, pos)?;
                    active = true;
                    if has_default {
                        let (name, default) = packed.split_once('\0').unwrap();
                        let value = env_value(env, name).filter(|v| !v.is_empty());
                        if let Some(value) = value {
                            append(&mut word, value, quote == Quote::None);
                        } else {
                            append(&mut word, &decode_default(default), quote == Quote::None);
                        }
                    } else if let Some(value) = env_value(env, &packed) {
                        append(&mut word, value, quote == Quote::None);
                    }
                    pos = end;
                    continue;
                }
                if is_name_start(next) {
                    let start = next_pos;
                    let mut end = start + nn;
                    while end < input.len() {
                        let (ch, cn) = char_at(input, end).unwrap();
                        if is_name_continue(ch) {
                            end += cn;
                        } else {
                            break;
                        }
                    }
                    let name = &input[start..end];
                    active = true;
                    if let Some(value) = env_value(env, name) {
                        append(&mut word, value, quote == Quote::None);
                    }
                    pos = end;
                    continue;
                }
            }
            active = true;
            word.text.push('$');
            pos += n;
            continue;
        }

        if quote == Quote::None {
            active = true;
            append(&mut word, &c.to_string(), true);
        } else {
            active = true;
            word.text.push(c);
        }
        pos += n;
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

fn main() {}