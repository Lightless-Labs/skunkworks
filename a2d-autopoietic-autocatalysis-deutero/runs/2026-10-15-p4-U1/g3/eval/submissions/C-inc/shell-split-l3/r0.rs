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
    Single(usize),
    Double(usize),
}

fn is_name_start(c: char) -> bool {
    c == '_' || c.is_ascii_alphabetic()
}

fn is_name_char(c: char) -> bool {
    is_name_start(c) || c.is_ascii_digit()
}

fn append_char(word: &mut Word, c: char, wildcard_active: bool) {
    if wildcard_active && matches!(c, '*' | '?' | '[') {
        word.glob.push(word.text.len());
    }
    word.text.push(c);
}

fn append_string(word: &mut Word, value: &str, wildcard_active: bool) {
    for c in value.chars() {
        append_char(word, c, wildcard_active);
    }
}

fn lookup<'a>(env: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    env.iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| *value)
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<Word>, SplitError> {
    if let Some(pos) = input.find('\0') {
        return Err(SplitError::Nul { pos });
    }

    let chars: Vec<(char, usize)> = input.char_indices().map(|(pos, c)| (c, pos)).collect();
    let mut result = Vec::new();
    let mut word = Word {
        text: String::new(),
        glob: Vec::new(),
    };
    let mut active = false;
    let mut quote: Option<Quote> = None;
    let mut i = 0;

    while i < chars.len() {
        let (c, pos) = chars[i];

        match quote {
            Some(Quote::Single(open_pos)) => {
                if c == '\'' {
                    quote = None;
                } else {
                    append_char(&mut word, c, false);
                }
                i += 1;
                if i == chars.len() && quote == Some(Quote::Single(open_pos)) {
                    return Err(SplitError::UnterminatedSingle { pos: open_pos });
                }
                continue;
            }
            Some(Quote::Double(open_pos)) => {
                if c == '"' {
                    quote = None;
                    i += 1;
                    continue;
                }

                if c == '\\' {
                    if i + 1 >= chars.len() {
                        return Err(SplitError::TrailingEscape { pos });
                    }
                    let next = chars[i + 1].0;
                    if matches!(next, '"' | '\\' | '$' | '\n') {
                        if next != '\n' {
                            append_char(&mut word, next, false);
                        }
                        i += 2;
                    } else {
                        append_char(&mut word, '\\', false);
                        i += 1;
                    }
                    continue;
                }

                if c == '`' {
                    return Err(SplitError::CommandSubstitution { pos });
                }

                if c == '$' {
                    if i + 1 < chars.len() && chars[i + 1].0 == '(' {
                        return Err(SplitError::CommandSubstitution { pos });
                    }

                    if i + 1 < chars.len() && chars[i + 1].0 == '{' {
                        let (value, next_i) = parse_braced(&chars, i, env)?;
                        active = true;
                        append_string(&mut word, &value, false);
                        i = next_i;
                        continue;
                    }

                    if i + 1 < chars.len() && is_name_start(chars[i + 1].0) {
                        let mut end = i + 1;
                        while end < chars.len() && is_name_char(chars[end].0) {
                            end += 1;
                        }
                        let name: String = chars[i + 1..end].iter().map(|x| x.0).collect();
                        active = true;
                        if let Some(value) = lookup(env, &name) {
                            append_string(&mut word, value, false);
                        }
                        i = end;
                        continue;
                    }
                }

                append_char(&mut word, c, false);
                i += 1;
                if i == chars.len() && quote == Some(Quote::Double(open_pos)) {
                    return Err(SplitError::UnterminatedDouble { pos: open_pos });
                }
                continue;
            }
            None => {}
        }

        if matches!(c, ' ' | '\t' | '\r' | '\n') {
            if active {
                result.push(word);
                word = Word {
                    text: String::new(),
                    glob: Vec::new(),
                };
                active = false;
            }
            i += 1;
            continue;
        }

        if c == '\'' {
            active = true;
            quote = Some(Quote::Single(pos));
            i += 1;
            continue;
        }

        if c == '"' {
            active = true;
            quote = Some(Quote::Double(pos));
            i += 1;
            continue;
        }

        if c == '\\' {
            if i + 1 >= chars.len() {
                return Err(SplitError::TrailingEscape { pos });
            }
            let next = chars[i + 1].0;
            if next == '\n' {
                i += 2;
            } else {
                active = true;
                append_char(&mut word, next, false);
                i += 2;
            }
            continue;
        }

        if c == '`' {
            return Err(SplitError::CommandSubstitution { pos });
        }

        if c == '$' {
            if i + 1 < chars.len() && chars[i + 1].0 == '(' {
                return Err(SplitError::CommandSubstitution { pos });
            }

            if i + 1 < chars.len() && chars[i + 1].0 == '{' {
                let (value, next_i) = parse_braced(&chars, i, env)?;
                active = true;
                append_string(&mut word, &value, true);
                i = next_i;
                continue;
            }

            if i + 1 < chars.len() && is_name_start(chars[i + 1].0) {
                let mut end = i + 1;
                while end < chars.len() && is_name_char(chars[end].0) {
                    end += 1;
                }
                let name: String = chars[i + 1..end].iter().map(|x| x.0).collect();
                active = true;
                if let Some(value) = lookup(env, &name) {
                    append_string(&mut word, value, true);
                }
                i = end;
                continue;
            }
        }

        active = true;
        append_char(&mut word, c, true);
        i += 1;
    }

    match quote {
        Some(Quote::Single(pos)) => return Err(SplitError::UnterminatedSingle { pos }),
        Some(Quote::Double(pos)) => return Err(SplitError::UnterminatedDouble { pos }),
        None => {}
    }

    if active {
        result.push(word);
    }
    Ok(result)
}

fn parse_braced(
    chars: &[(char, usize)],
    dollar_i: usize,
    env: &[(&str, &str)],
) -> Result<(String, usize), SplitError> {
    let dollar_pos = chars[dollar_i].1;
    let mut close = dollar_i + 2;

    while close < chars.len() {
        if chars[close].0 == '\\' {
            close += 2;
        } else if chars[close].0 == '}' {
            break;
        } else {
            close += 1;
        }
    }

    if close >= chars.len() {
        return Err(SplitError::UnterminatedExpansion { pos: dollar_pos });
    }

    let content: Vec<char> = chars[dollar_i + 2..close].iter().map(|x| x.0).collect();
    let mut name_end = 0;

    if content.is_empty() || !is_name_start(content[0]) {
        return Err(SplitError::InvalidExpansion { pos: dollar_pos });
    }

    name_end += 1;
    while name_end < content.len() && is_name_char(content[name_end]) {
        name_end += 1;
    }
    let name: String = content[..name_end].iter().collect();

    if name_end == content.len() {
        return Ok((lookup(env, &name).unwrap_or("").to_string(), close + 1));
    }

    if name_end + 2 > content.len()
        || content[name_end] != ':'
        || content[name_end + 1] != '-'
    {
        return Err(SplitError::InvalidExpansion { pos: dollar_pos });
    }

    let selected = lookup(env, &name).filter(|value| !value.is_empty());
    if let Some(value) = selected {
        return Ok((value.to_string(), close + 1));
    }

    let mut decoded = String::new();
    let mut j = name_end + 2;
    while j < content.len() {
        if content[j] == '\\' && j + 1 < content.len() {
            decoded.push(content[j + 1]);
            j += 2;
        } else {
            decoded.push(content[j]);
            j += 1;
        }
    }

    Ok((decoded, close + 1))
}

fn main() {}