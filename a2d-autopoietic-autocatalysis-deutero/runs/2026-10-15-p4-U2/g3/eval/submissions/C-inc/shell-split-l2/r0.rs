#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitError {
    Nul { pos: usize },
    TrailingEscape { pos: usize },
    UnterminatedSingle { pos: usize },
    UnterminatedDouble { pos: usize },
    UnterminatedExpansion { pos: usize },
    InvalidExpansion { pos: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Expansion {
    Simple { end: usize, value: String },
    Braced { end: usize, value: String },
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn lookup<'a>(name: &str, env: &'a [(&str, &str)]) -> &'a str {
    env.iter()
        .find_map(|(key, value)| (*key == name).then_some(*value))
        .unwrap_or("")
}

fn expand(input: &str, dollar: usize, env: &[(&str, &str)]) -> Result<Option<Expansion>, SplitError> {
    let tail = &input[dollar + 1..];
    let Some(first) = tail.chars().next() else {
        return Ok(None);
    };

    if first == '{' {
        let content_start = dollar + 2;
        let mut pos = content_start;
        let mut close = None;

        while pos < input.len() {
            let c = input[pos..].chars().next().unwrap();
            if c == '\\' {
                pos += 1;
                if pos < input.len() {
                    pos += input[pos..].chars().next().unwrap().len_utf8();
                }
            } else if c == '}' {
                close = Some(pos);
                break;
            } else {
                pos += c.len_utf8();
            }
        }

        let Some(close_pos) = close else {
            return Err(SplitError::UnterminatedExpansion { pos: dollar });
        };

        let contents = &input[content_start..close_pos];
        let mut name_end = 0;
        for c in contents.chars() {
            if is_name_char(c) {
                name_end += c.len_utf8();
            } else {
                break;
            }
        }

        let name = &contents[..name_end];
        if name.is_empty() || !is_name_start(name.chars().next().unwrap()) {
            return Err(SplitError::InvalidExpansion { pos: dollar });
        }

        let rest = &contents[name_end..];
        let value = if rest.is_empty() {
            lookup(name, env).to_string()
        } else if let Some(default) = rest.strip_prefix(":-") {
            let found = lookup(name, env);
            if !found.is_empty() {
                found.to_string()
            } else {
                decode_default(default)
            }
        } else {
            return Err(SplitError::InvalidExpansion { pos: dollar });
        };

        Ok(Some(Expansion::Braced {
            end: close_pos + 1,
            value,
        }))
    } else if is_name_start(first) {
        let mut end = dollar + 1;
        while end < input.len() {
            let c = input[end..].chars().next().unwrap();
            if !is_name_char(c) {
                break;
            }
            end += c.len_utf8();
        }
        let name = &input[dollar + 1..end];
        Ok(Some(Expansion::Simple {
            end,
            value: lookup(name, env).to_string(),
        }))
    } else {
        Ok(None)
    }
}

fn decode_default(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
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

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<String>, SplitError> {
    if let Some(pos) = input.find('\0') {
        return Err(SplitError::Nul { pos });
    }

    let mut words = Vec::new();
    let mut word = String::new();
    let mut active = false;
    let mut pos = 0;
    let mut quote: Option<(char, usize)> = None;

    while pos < input.len() {
        let c = input[pos..].chars().next().unwrap();
        let next_pos = pos + c.len_utf8();

        if let Some((q, _)) = quote {
            if q == '\'' {
                if c == '\'' {
                    quote = None;
                } else {
                    word.push(c);
                }
                pos = next_pos;
                continue;
            }

            if c == '"' {
                quote = None;
                pos = next_pos;
                continue;
            }
            if c == '\\' {
                if next_pos == input.len() {
                    return Err(SplitError::TrailingEscape { pos });
                }
                let next = input[next_pos..].chars().next().unwrap();
                let after_next = next_pos + next.len_utf8();
                if matches!(next, '"' | '\\' | '$') {
                    word.push(next);
                    pos = after_next;
                } else if next == '\n' {
                    pos = after_next;
                } else {
                    word.push('\\');
                    pos = next_pos;
                }
                continue;
            }
            if c == '$' {
                if let Some(expansion) = expand(input, pos, env)? {
                    match expansion {
                        Expansion::Simple { end, value } | Expansion::Braced { end, value } => {
                            word.push_str(&value);
                            active = true;
                            pos = end;
                            continue;
                        }
                    }
                }
            }
            word.push(c);
            pos = next_pos;
            continue;
        }

        if c == ' ' || c == '\t' || c == '\r' || c == '\n' {
            if active {
                words.push(std::mem::take(&mut word));
                active = false;
            }
            pos = next_pos;
            continue;
        }

        if c == '\'' || c == '"' {
            quote = Some((c, pos));
            active = true;
            pos = next_pos;
            continue;
        }

        if c == '\\' {
            if next_pos == input.len() {
                return Err(SplitError::TrailingEscape { pos });
            }
            let next = input[next_pos..].chars().next().unwrap();
            let after_next = next_pos + next.len_utf8();
            if next != '\n' {
                word.push(next);
                active = true;
            }
            pos = after_next;
            continue;
        }

        if c == '$' {
            if let Some(expansion) = expand(input, pos, env)? {
                match expansion {
                    Expansion::Simple { end, value } | Expansion::Braced { end, value } => {
                        word.push_str(&value);
                        active = true;
                        pos = end;
                        continue;
                    }
                }
            }
        }

        word.push(c);
        active = true;
        pos = next_pos;
    }

    if let Some((q, opening)) = quote {
        if q == '\'' {
            return Err(SplitError::UnterminatedSingle { pos: opening });
        }
        return Err(SplitError::UnterminatedDouble { pos: opening });
    }

    if active {
        words.push(word);
    }
    Ok(words)
}

fn main() {}