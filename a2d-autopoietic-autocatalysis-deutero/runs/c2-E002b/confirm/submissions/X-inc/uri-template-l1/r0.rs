#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    UnexpectedClose { pos: usize },
    Unclosed { pos: usize },
    NestedOpen { pos: usize },
    EmptyExpression { pos: usize },
    UnsupportedOperator { pos: usize },
    EmptyVariable { pos: usize },
    InvalidName { pos: usize },
    DuplicateVariable { pos: usize },
}

#[derive(Debug)]
struct Expression {
    names: Vec<String>,
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut cursor = 0;

    while cursor < bytes.len() {
        match bytes[cursor] {
            b'}' => return Err(Error::UnexpectedClose { pos: cursor }),
            b'{' => {
                let open = cursor;
                let Some(relative_close) = bytes[cursor + 1..].iter().position(|&b| b == b'}') else {
                    return Err(Error::Unclosed { pos: open });
                };
                let close = cursor + 1 + relative_close;

                if let Some(relative_open) = bytes[open + 1..close].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen {
                        pos: open + 1 + relative_open,
                    });
                }

                let mut content_start = open + 1;
                if content_start == close {
                    return Err(Error::EmptyExpression { pos: close });
                }

                if matches!(bytes[content_start], b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&') {
                    return Err(Error::UnsupportedOperator { pos: content_start });
                }

                let mut names = Vec::new();
                let mut seen: Vec<&str> = Vec::new();

                loop {
                    let entry_start = content_start;
                    let entry_end = bytes[entry_start..close]
                        .iter()
                        .position(|&b| b == b',')
                        .map_or(close, |offset| entry_start + offset);

                    if entry_start == entry_end {
                        return Err(Error::EmptyVariable { pos: entry_start });
                    }

                    let name_bytes = &bytes[entry_start..entry_end];
                    let valid_start = |b: u8| b.is_ascii_alphabetic() || b == b'_';
                    let valid_rest = |b: u8| valid_start(b) || b.is_ascii_digit();

                    if !valid_start(name_bytes[0]) {
                        return Err(Error::InvalidName { pos: entry_start });
                    }
                    if let Some(offset) = name_bytes.iter().position(|&b| !valid_rest(b)) {
                        return Err(Error::InvalidName {
                            pos: entry_start + offset,
                        });
                    }

                    let name = &template[entry_start..entry_end];
                    if seen.contains(&name) {
                        return Err(Error::DuplicateVariable { pos: entry_start });
                    }
                    seen.push(name);
                    names.push(name.to_owned());

                    if entry_end == close {
                        break;
                    }
                    content_start = entry_end + 1;
                    if content_start == close {
                        return Err(Error::EmptyVariable { pos: close });
                    }
                }

                expressions.push((open, close, Expression { names }));
                cursor = close + 1;
            }
            _ => cursor += 1,
        }
    }

    let mut output = String::with_capacity(template.len());
    let mut literal_start = 0;

    for (open, close, expression) in expressions {
        output.push_str(&template[literal_start..open]);

        let mut first = true;
        for name in expression.names {
            if let Some(value) = vars.iter().rev().find(|(binding, _)| *binding == name).map(|(_, value)| *value) {
                if !first {
                    output.push(',');
                }
                first = false;
                encode_value(value, &mut output);
            }
        }

        literal_start = close + 1;
    }
    output.push_str(&template[literal_start..]);

    Ok(output)
}

fn encode_value(value: &str, output: &mut String) {
    let bytes = value.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];

        if byte == b'%' && i + 2 < bytes.len() && is_hex(bytes[i + 1]) && is_hex(bytes[i + 2]) {
            output.push('%');
            output.push((bytes[i + 1] as char).to_ascii_uppercase());
            output.push((bytes[i + 2] as char).to_ascii_uppercase());
            i += 3;
        } else if is_unreserved(byte) {
            output.push(byte as char);
            i += 1;
        } else {
            output.push('%');
            output.push(hex_digit(byte >> 4));
            output.push(hex_digit(byte & 0x0f));
            i += 1;
        }
    }
}

fn is_hex(byte: u8) -> bool {
    byte.is_ascii_digit() || (b'A'..=b'F').contains(&byte) || (b'a'..=b'f').contains(&byte)
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        _ => (b'A' + value - 10) as char,
    }
}