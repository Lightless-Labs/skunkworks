#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitError {
    Nul { pos: usize },
    TrailingEscape { pos: usize },
    UnterminatedSingle { pos: usize },
    UnterminatedDouble { pos: usize },
    UnterminatedExpansion { pos: usize },
    InvalidExpansion { pos: usize },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Quote {
    None,
    Single,
    Double,
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

fn next_char(input: &str, pos: usize) -> (char, usize) {
    let ch = input[pos..].chars().next().expect("position must be in bounds");
    (ch, pos + ch.len_utf8())
}

fn append_expansion(
    input: &str,
    dollar: usize,
    end: usize,
    env: &[(&str, &str)],
    output: &mut String,
) -> Result<usize, SplitError> {
    let bytes = input.as_bytes();

    if dollar + 1 < end && bytes[dollar + 1] == b'{' {
        let mut close = dollar + 2;
        while close < bytes.len() {
            if bytes[close] == b'\\' {
                if close + 1 < bytes.len() {
                    let (_, next) = next_char(input, close + 1);
                    close = next;
                } else {
                    close += 1;
                }
            } else if bytes[close] == b'}' {
                break;
            } else {
                let (_, next) = next_char(input, close);
                close = next;
            }
        }

        if close == bytes.len() {
            return Err(SplitError::UnterminatedExpansion { pos: dollar });
        }

        let contents = &input[dollar + 2..close];
        let content_bytes = contents.as_bytes();
        let mut name_end = 0;

        if content_bytes.is_empty() || !is_name_start(content_bytes[0]) {
            return Err(SplitError::InvalidExpansion { pos: dollar });
        }
        name_end += 1;
        while name_end < content_bytes.len() && is_name_continue(content_bytes[name_end]) {
            name_end += 1;
        }

        let name = &contents[..name_end];
        let default = if name_end == content_bytes.len() {
            None
        } else if content_bytes.get(name_end..name_end + 2) == Some(b":-") {
            Some(&contents[name_end + 2..])
        } else {
            return Err(SplitError::InvalidExpansion { pos: dollar });
        };

        let value = env.iter().find(|(key, _)| *key == name).map(|(_, value)| *value);
        if let Some(default_text) = default {
            if let Some(value) = value.filter(|value| !value.is_empty()) {
                output.push_str(value);
            } else {
                let mut i = 0;
                while i < default_text.len() {
                    let (ch, next) = next_char(default_text, i);
                    if ch == '\\' {
                        if next < default_text.len() {
                            let (escaped, after) = next_char(default_text, next);
                            output.push(escaped);
                            i = after;
                        } else {
                            output.push('\\');
                            i = next;
                        }
                    } else {
                        output.push(ch);
                        i = next;
                    }
                }
            }
        } else if let Some(value) = value {
            output.push_str(value);
        }

        Ok(close + 1)
    } else {
        let mut name_end = dollar + 1;
        if name_end < end && is_name_start(bytes[name_end]) {
            name_end += 1;
            while name_end < end && is_name_continue(bytes[name_end]) {
                name_end += 1;
            }
            let name = &input[dollar + 1..name_end];
            if let Some((_, value)) = env.iter().find(|(key, _)| *key == name) {
                output.push_str(value);
            }
            Ok(name_end)
        } else {
            output.push('$');
            Ok(dollar + 1)
        }
    }
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<String>, SplitError> {
    if let Some(pos) = input.as_bytes().iter().position(|&byte| byte == 0) {
        return Err(SplitError::Nul { pos });
    }

    let bytes = input.as_bytes();
    let mut words = Vec::new();
    let mut word = String::new();
    let mut active = false;
    let mut quote = Quote::None;
    let mut quote_start = 0;
    let mut pos = 0;

    while pos < bytes.len() {
        let (ch, next) = next_char(input, pos);

        match quote {
            Quote::Single => {
                if ch == '\'' {
                    quote = Quote::None;
                } else {
                    word.push(ch);
                }
                pos = next;
            }
            Quote::Double => {
                if ch == '"' {
                    quote = Quote::None;
                    pos = next;
                } else if ch == '\\' {
                    if next == bytes.len() {
                        return Err(SplitError::TrailingEscape { pos });
                    }
                    let (escaped, after) = next_char(input, next);
                    if escaped == '"' || escaped == '\\' || escaped == '$' {
                        word.push(escaped);
                        pos = after;
                    } else if escaped == '\n' {
                        pos = after;
                    } else {
                        word.push('\\');
                        pos = next;
                    }
                } else if ch == '$' {
                    active = true;
                    pos = append_expansion(input, pos, bytes.len(), env, &mut word)?;
                } else {
                    word.push(ch);
                    pos = next;
                }
            }
            Quote::None => {
                if ch == ' ' || ch == '\t' || ch == '\r' || ch == '\n' {
                    if active {
                        words.push(std::mem::take(&mut word));
                        active = false;
                    }
                    pos = next;
                } else if ch == '\'' {
                    active = true;
                    quote = Quote::Single;
                    quote_start = pos;
                    pos = next;
                } else if ch == '"' {
                    active = true;
                    quote = Quote::Double;
                    quote_start = pos;
                    pos = next;
                } else if ch == '\\' {
                    if next == bytes.len() {
                        return Err(SplitError::TrailingEscape { pos });
                    }
                    let (escaped, after) = next_char(input, next);
                    if escaped == '\n' {
                        pos = after;
                    } else {
                        active = true;
                        word.push(escaped);
                        pos = after;
                    }
                } else if ch == '$' {
                    let old_pos = pos;
                    let was_expansion = next < bytes.len()
                        && (is_name_start(bytes[next]) || bytes[next] == b'{');
                    if was_expansion {
                        active = true;
                    }
                    pos = append_expansion(input, pos, bytes.len(), env, &mut word)?;
                    if pos == old_pos {
                        unreachable!("expansion scanner must advance");
                    }
                } else {
                    active = true;
                    word.push(ch);
                    pos = next;
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

fn main() {}