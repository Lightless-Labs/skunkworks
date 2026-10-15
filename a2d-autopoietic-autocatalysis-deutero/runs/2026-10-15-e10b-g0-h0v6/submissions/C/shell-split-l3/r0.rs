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

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

fn append_char(word: &mut Word, c: char, active_glob: bool) {
    let offset = word.text.len();
    word.text.push(c);
    if active_glob && matches!(c, '*' | '?' | '[') {
        word.glob.push(offset);
    }
}

fn append_str(word: &mut Word, value: &str, active_glob: bool) {
    for c in value.chars() {
        append_char(word, c, active_glob);
    }
}

fn decode_default(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn lookup<'a>(env: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    env.iter().find(|(key, _)| *key == name).map(|(_, value)| *value)
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<Word>, SplitError> {
    if let Some(pos) = input.as_bytes().iter().position(|&b| b == 0) {
        return Err(SplitError::Nul { pos });
    }

    let chars: Vec<(usize, char)> = input.char_indices().collect();
    let mut words = Vec::new();
    let mut word = Word {
        text: String::new(),
        glob: Vec::new(),
    };
    let mut active = false;
    let mut quote = Quote::None;
    let mut quote_open = 0usize;
    let mut i = 0usize;

    while i < chars.len() {
        let (pos, c) = chars[i];

        match quote {
            Quote::Single => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    append_char(&mut word, c, false);
                }
                i += 1;
                continue;
            }
            Quote::Double => {
                if c == '"' {
                    quote = Quote::None;
                    i += 1;
                    continue;
                }
                if c == '\\' {
                    if i + 1 >= chars.len() {
                        return Err(SplitError::TrailingEscape { pos });
                    }
                    let next = chars[i + 1].1;
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
                    if i + 1 < chars.len() && chars[i + 1].1 == '(' {
                        return Err(SplitError::CommandSubstitution { pos });
                    }
                    if i + 1 < chars.len() && chars[i + 1].1 == '{' {
                        let (value, next_i) = parse_braced(input, &chars, i, env)?;
                        active = true;
                        append_str(&mut word, &value, false);
                        i = next_i;
                        continue;
                    }
                    if i + 1 < chars.len() && is_name_start(chars[i + 1].1) {
                        let (name, end) = parse_name(&chars, i + 1);
                        active = true;
                        if let Some(value) = lookup(env, &name) {
                            append_str(&mut word, value, false);
                        }
                        i = end;
                        continue;
                    }
                }
                append_char(&mut word, c, false);
                i += 1;
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
                    i += 1;
                    continue;
                }
                if c == '\'' {
                    active = true;
                    quote = Quote::Single;
                    quote_open = pos;
                    i += 1;
                    continue;
                }
                if c == '"' {
                    active = true;
                    quote = Quote::Double;
                    quote_open = pos;
                    i += 1;
                    continue;
                }
                if c == '\\' {
                    if i + 1 >= chars.len() {
                        return Err(SplitError::TrailingEscape { pos });
                    }
                    let next = chars[i + 1].1;
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
                    if i + 1 < chars.len() && chars[i + 1].1 == '(' {
                        return Err(SplitError::CommandSubstitution { pos });
                    }
                    if i + 1 < chars.len() && chars[i + 1].1 == '{' {
                        let (value, next_i) = parse_braced(input, &chars, i, env)?;
                        active = true;
                        append_str(&mut word, &value, true);
                        i = next_i;
                        continue;
                    }
                    if i + 1 < chars.len() && is_name_start(chars[i + 1].1) {
                        let (name, end) = parse_name(&chars, i + 1);
                        active = true;
                        if let Some(value) = lookup(env, &name) {
                            append_str(&mut word, value, true);
                        }
                        i = end;
                        continue;
                    }
                    active = true;
                    append_char(&mut word, '$', true);
                    i += 1;
                    continue;
                }
                active = true;
                append_char(&mut word, c, true);
                i += 1;
            }
        }
    }

    match quote {
        Quote::Single => return Err(SplitError::UnterminatedSingle { pos: quote_open }),
        Quote::Double => return Err(SplitError::UnterminatedDouble { pos: quote_open }),
        Quote::None => {}
    }

    if active {
        words.push(word);
    }
    Ok(words)
}

fn parse_name(chars: &[(usize, char)], start: usize) -> (String, usize) {
    let mut name = String::new();
    let mut i = start;
    while i < chars.len() && is_name_char(chars[i].1) {
        name.push(chars[i].1);
        i += 1;
    }
    (name, i)
}

fn parse_braced(
    input: &str,
    chars: &[(usize, char)],
    dollar_i: usize,
    env: &[(&str, &str)],
) -> Result<(String, usize), SplitError> {
    let dollar_pos = chars[dollar_i].0;
    let mut close_i = None;
    let mut i = dollar_i + 2;

    while i < chars.len() {
        match chars[i].1 {
            '\\' => {
                i += 2;
            }
            '}' => {
                close_i = Some(i);
                break;
            }
            _ => i += 1,
        }
    }

    let close = close_i.ok_or(SplitError::UnterminatedExpansion { pos: dollar_pos })?;
    let mut content = String::new();
    for &(_, c) in &chars[dollar_i + 2..close] {
        content.push(c);
    }

    let (name, default_start) = {
        let content_chars: Vec<char> = content.chars().collect();
        if content_chars.is_empty() || !is_name_start(content_chars[0]) {
            return Err(SplitError::InvalidExpansion { pos: dollar_pos });
        }
        let mut n = String::new();
        let mut j = 0;
        while j < content_chars.len() && is_name_char(content_chars[j]) {
            n.push(content_chars[j]);
            j += 1;
        }
        (n, j)
    };

    let content_chars: Vec<char> = content.chars().collect();
    let selected = if default_start == content_chars.len() {
        lookup(env, &name).unwrap_or("")
    } else if default_start + 1 < content_chars.len()
        && content_chars[default_start] == ':'
        && content_chars[default_start + 1] == '-'
    {
        let raw_default: String = content_chars[default_start + 2..].iter().collect();
        let value = lookup(env, &name).unwrap_or("");
        if value.is_empty() {
            return Ok((decode_default(&raw_default), close + 1));
        }
        value
    } else {
        return Err(SplitError::InvalidExpansion { pos: dollar_pos });
    };

    Ok((selected.to_string(), close + 1))
}

fn main() {}