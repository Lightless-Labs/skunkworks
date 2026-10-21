#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitError {
    Nul { pos: usize },
    TrailingEscape { pos: usize },
    UnterminatedSingle { pos: usize },
    UnterminatedDouble { pos: usize },
    UnterminatedExpansion { pos: usize },
    InvalidExpansion { pos: usize },
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<String>, SplitError> {
    let bytes = input.as_bytes();
    if let Some(pos) = bytes.iter().position(|&byte| byte == 0) {
        return Err(SplitError::Nul { pos });
    }

    let mut result = Vec::new();
    let mut current = String::new();
    let mut active = false;
    let mut mode = 0u8; // 0 bare, 1 single-quoted, 2 double-quoted
    let mut opening = 0usize;
    let mut index = 0usize;

    while index < input.len() {
        let ch = input[index..].chars().next().unwrap();
        let width = ch.len_utf8();

        if mode == 1 {
            if ch == '\'' {
                mode = 0;
            } else {
                current.push(ch);
            }
            index += width;
            continue;
        }

        if mode == 2 {
            match ch {
                '"' => {
                    mode = 0;
                    index += width;
                }
                '\\' => {
                    let esc_pos = index;
                    index += width;
                    if index == input.len() {
                        return Err(SplitError::TrailingEscape { pos: esc_pos });
                    }
                    let next = input[index..].chars().next().unwrap();
                    if matches!(next, '"' | '\\' | '$' | '\n') {
                        index += next.len_utf8();
                        if next != '\n' {
                            current.push(next);
                        }
                    } else {
                        current.push('\\');
                    }
                }
                '$' => {
                    active = true;
                    index = expand_at(input, index, env, &mut current)?;
                }
                _ => {
                    current.push(ch);
                    index += width;
                }
            }
            continue;
        }

        match ch {
            ' ' | '\t' | '\r' | '\n' => {
                if active {
                    result.push(std::mem::take(&mut current));
                    active = false;
                }
                index += width;
            }
            '\'' | '"' => {
                active = true;
                mode = if ch == '\'' { 1 } else { 2 };
                opening = index;
                index += width;
            }
            '\\' => {
                let esc_pos = index;
                index += width;
                if index == input.len() {
                    return Err(SplitError::TrailingEscape { pos: esc_pos });
                }
                let next = input[index..].chars().next().unwrap();
                index += next.len_utf8();
                if next != '\n' {
                    active = true;
                    current.push(next);
                }
            }
            '$' => {
                active = true;
                index = expand_at(input, index, env, &mut current)?;
            }
            _ => {
                active = true;
                current.push(ch);
                index += width;
            }
        }
    }

    if mode == 1 {
        return Err(SplitError::UnterminatedSingle { pos: opening });
    }
    if mode == 2 {
        return Err(SplitError::UnterminatedDouble { pos: opening });
    }
    if active {
        result.push(current);
    }
    Ok(result)
}

fn expand_at(
    input: &str,
    dollar: usize,
    env: &[(&str, &str)],
    output: &mut String,
) -> Result<usize, SplitError> {
    let mut index = dollar + 1;
    if index == input.len() {
        output.push('$');
        return Ok(index);
    }
    let first = input[index..].chars().next().unwrap();
    if first == '{' {
        index += 1;
        let contents_start = index;
        let mut close = None;
        while index < input.len() {
            let ch = input[index..].chars().next().unwrap();
            if ch == '\\' {
                index += 1;
                if index < input.len() {
                    index += input[index..].chars().next().unwrap().len_utf8();
                }
            } else if ch == '}' {
                close = Some(index);
                break;
            } else {
                index += ch.len_utf8();
            }
        }
        let Some(close_at) = close else {
            return Err(SplitError::UnterminatedExpansion { pos: dollar });
        };
        let body = &input[contents_start..close_at];
        let operator = body.find(":-");
        let (name, default) = match operator {
            Some(at) => (&body[..at], Some(&body[at + 2..])),
            None => (body, None),
        };
        if !valid_name(name) {
            return Err(SplitError::InvalidExpansion { pos: dollar });
        }
        if default.is_none() && body.contains(':') {
            return Err(SplitError::InvalidExpansion { pos: dollar });
        }
        let found = env.iter().find(|(key, _)| *key == name).map(|(_, val)| *val);
        if let Some(default_text) = default {
            if found.is_some_and(|value| !value.is_empty()) {
                output.push_str(found.unwrap());
            } else {
                output.push_str(&decode(default_text));
            }
        } else if let Some(value) = found {
            output.push_str(value);
        }
        Ok(close_at + 1)
    } else if name_start(first) {
        let start = index;
        while index < input.len() {
            let ch = input[index..].chars().next().unwrap();
            if name_continue(ch) {
                index += ch.len_utf8();
            } else {
                break;
            }
        }
        let name = &input[start..index];
        if let Some((_, value)) = env.iter().find(|(key, _)| *key == name) {
            output.push_str(value);
        }
        Ok(index)
    } else {
        output.push('$');
        Ok(dollar + 1)
    }
}

fn name_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn name_continue(ch: char) -> bool {
    name_start(ch) || ch.is_ascii_digit()
}

fn valid_name(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(ch) if name_start(ch)) && chars.all(name_continue)
}

fn decode(text: &str) -> String {
    let mut chars = text.chars();
    let mut output = String::new();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next) = chars.next() {
                output.push(next);
            } else {
                output.push('\\');
            }
        } else {
            output.push(ch);
        }
    }
    output
}