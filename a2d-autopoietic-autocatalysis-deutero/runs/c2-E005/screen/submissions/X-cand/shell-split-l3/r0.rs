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

struct Builder {
    text: String,
    glob: Vec<usize>,
    active: bool,
}

impl Builder {
    fn new() -> Self {
        Self {
            text: String::new(),
            glob: Vec::new(),
            active: false,
        }
    }

    fn append(&mut self, s: &str, wildcard_active: bool) {
        for ch in s.chars() {
            if wildcard_active && matches!(ch, '*' | '?' | '[') {
                self.glob.push(self.text.len());
            }
            self.text.push(ch);
        }
    }

    fn finish(&mut self, words: &mut Vec<Word>) {
        if self.active {
            words.push(Word {
                text: std::mem::take(&mut self.text),
                glob: std::mem::take(&mut self.glob),
            });
            self.active = false;
        }
    }
}

fn is_name_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn is_name_continue(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

fn lookup<'a>(env: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    env.iter()
        .find_map(|(key, value)| (*key == name).then_some(*value))
}

fn decode_default(text: &str) -> String {
    let mut result = String::new();
    let mut chars = text.char_indices().peekable();

    while let Some((_, ch)) = chars.next() {
        if ch == '\\' {
            if let Some((_, escaped)) = chars.next() {
                result.push(escaped);
            } else {
                result.push('\\');
            }
        } else {
            result.push(ch);
        }
    }

    result
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<Word>, SplitError> {
    if let Some(pos) = input.find('\0') {
        return Err(SplitError::Nul { pos });
    }

    let mut words = Vec::new();
    let mut word = Builder::new();
    let mut quote = Quote::None;
    let mut quote_pos = 0usize;
    let mut i = 0usize;

    while i < input.len() {
        let ch = input[i..].chars().next().expect("valid character boundary");
        let next_i = i + ch.len_utf8();

        match quote {
            Quote::Single => {
                if ch == '\'' {
                    quote = Quote::None;
                } else {
                    word.append(&input[i..next_i], false);
                }
                i = next_i;
            }
            Quote::Double => {
                match ch {
                    '"' => {
                        quote = Quote::None;
                        i = next_i;
                    }
                    '\\' => {
                        let Some(next) = input[next_i..].chars().next() else {
                            return Err(SplitError::TrailingEscape { pos: i });
                        };
                        let after_next = next_i + next.len_utf8();
                        if matches!(next, '"' | '\\' | '$' | '\n') {
                            if next != '\n' {
                                word.append(&input[next_i..after_next], false);
                            }
                            i = after_next;
                        } else {
                            word.append("\\", false);
                            i = next_i;
                        }
                    }
                    '`' => return Err(SplitError::CommandSubstitution { pos: i }),
                    '$' => {
                        let Some(next) = input[next_i..].chars().next() else {
                            word.append("$", false);
                            i = next_i;
                            continue;
                        };
                        if next == '(' {
                            return Err(SplitError::CommandSubstitution { pos: i });
                        }
                        if is_name_start(next) {
                            word.active = true;
                            let mut end = next_i;
                            while end < input.len() {
                                let c = input[end..].chars().next().unwrap();
                                if !is_name_continue(c) {
                                    break;
                                }
                                end += c.len_utf8();
                            }
                            let value = lookup(env, &input[next_i..end]).unwrap_or("");
                            word.append(value, false);
                            i = end;
                        } else if next == '{' {
                            let (end, value) = parse_braced(input, i, env)?;
                            word.active = true;
                            word.append(&value, false);
                            i = end;
                        } else {
                            word.append("$", false);
                            i = next_i;
                        }
                    }
                    _ => {
                        word.append(&input[i..next_i], false);
                        i = next_i;
                    }
                }
            }
            Quote::None => {
                match ch {
                    ' ' | '\t' | '\r' | '\n' => {
                        word.finish(&mut words);
                        i = next_i;
                    }
                    '\'' => {
                        word.active = true;
                        quote = Quote::Single;
                        quote_pos = i;
                        i = next_i;
                    }
                    '"' => {
                        word.active = true;
                        quote = Quote::Double;
                        quote_pos = i;
                        i = next_i;
                    }
                    '\\' => {
                        let Some(next) = input[next_i..].chars().next() else {
                            return Err(SplitError::TrailingEscape { pos: i });
                        };
                        let after_next = next_i + next.len_utf8();
                        if next != '\n' {
                            word.active = true;
                            word.append(&input[next_i..after_next], false);
                        }
                        i = after_next;
                    }
                    '`' => return Err(SplitError::CommandSubstitution { pos: i }),
                    '$' => {
                        let Some(next) = input[next_i..].chars().next() else {
                            word.active = true;
                            word.append("$", true);
                            i = next_i;
                            continue;
                        };
                        if next == '(' {
                            return Err(SplitError::CommandSubstitution { pos: i });
                        }
                        if is_name_start(next) {
                            word.active = true;
                            let mut end = next_i;
                            while end < input.len() {
                                let c = input[end..].chars().next().unwrap();
                                if !is_name_continue(c) {
                                    break;
                                }
                                end += c.len_utf8();
                            }
                            let value = lookup(env, &input[next_i..end]).unwrap_or("");
                            word.append(value, true);
                            i = end;
                        } else if next == '{' {
                            let (end, value) = parse_braced(input, i, env)?;
                            word.active = true;
                            word.append(&value, true);
                            i = end;
                        } else {
                            word.active = true;
                            word.append("$", true);
                            i = next_i;
                        }
                    }
                    _ => {
                        word.active = true;
                        word.append(&input[i..next_i], true);
                        i = next_i;
                    }
                }
            }
        }
    }

    match quote {
        Quote::Single => return Err(SplitError::UnterminatedSingle { pos: quote_pos }),
        Quote::Double => return Err(SplitError::UnterminatedDouble { pos: quote_pos }),
        Quote::None => {}
    }

    word.finish(&mut words);
    Ok(words)
}

fn parse_braced(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
) -> Result<(usize, String), SplitError> {
    let content_start = dollar + 2;
    let mut cursor = content_start;
    let mut close = None;

    while cursor < input.len() {
        let ch = input[cursor..].chars().next().expect("valid character boundary");
        let next = cursor + ch.len_utf8();

        if ch == '\\' {
            if next < input.len() {
                let escaped = input[next..].chars().next().unwrap();
                cursor = next + escaped.len_utf8();
            } else {
                cursor = next;
            }
        } else if ch == '}' {
            close = Some(cursor);
            break;
        } else {
            cursor = next;
        }
    }

    let Some(close_pos) = close else {
        return Err(SplitError::UnterminatedExpansion { pos: dollar });
    };

    let contents = &input[content_start..close_pos];
    let mut name_end = 0usize;
    let mut chars = contents.char_indices();

    match chars.next() {
        Some((_, ch)) if is_name_start(ch) => {
            name_end = ch.len_utf8();
        }
        _ => return Err(SplitError::InvalidExpansion { pos: dollar }),
    }

    for (offset, ch) in chars {
        if is_name_continue(ch) {
            name_end = offset + ch.len_utf8();
        } else {
            break;
        }
    }

    let name = &contents[..name_end];
    let rest = &contents[name_end..];

    let value = if rest.is_empty() {
        lookup(env, name).unwrap_or("").to_string()
    } else if let Some(default) = rest.strip_prefix(":-") {
        match lookup(env, name) {
            Some(value) if !value.is_empty() => value.to_string(),
            _ => decode_default(default),
        }
    } else {
        return Err(SplitError::InvalidExpansion { pos: dollar });
    };

    Ok((close_pos + 1, value))
}