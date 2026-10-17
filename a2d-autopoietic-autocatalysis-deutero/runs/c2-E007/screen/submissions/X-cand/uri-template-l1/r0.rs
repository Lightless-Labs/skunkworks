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

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut cursor = 0;

    while cursor < bytes.len() {
        match bytes[cursor] {
            b'}' => return Err(Error::UnexpectedClose { pos: cursor }),
            b'{' => {
                let open = cursor;
                let close = match bytes[open + 1..].iter().position(|&b| b == b'}') {
                    Some(offset) => open + 1 + offset,
                    None => return Err(Error::Unclosed { pos: open }),
                };

                if let Some(offset) = bytes[open + 1..close].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen {
                        pos: open + 1 + offset,
                    });
                }

                let content_start = open + 1;
                let content = &bytes[content_start..close];
                let mut spec_start = content_start;

                if let Some(&operator) = content.first() {
                    if matches!(operator, b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&') {
                        return Err(Error::UnsupportedOperator { pos: content_start });
                    }
                }

                if content.is_empty() {
                    return Err(Error::EmptyExpression { pos: close });
                }

                let mut names = Vec::new();
                let mut seen = Vec::<&str>::new();
                while spec_start <= close {
                    let spec_end = bytes[spec_start..close]
                        .iter()
                        .position(|&b| b == b',')
                        .map(|offset| spec_start + offset)
                        .unwrap_or(close);

                    if spec_start == spec_end {
                        return Err(Error::EmptyVariable { pos: spec_start });
                    }

                    let name_bytes = &bytes[spec_start..spec_end];
                    let first_ok = name_bytes[0].is_ascii_alphabetic() || name_bytes[0] == b'_';
                    if !first_ok {
                        return Err(Error::InvalidName { pos: spec_start });
                    }

                    for (offset, &byte) in name_bytes.iter().enumerate().skip(1) {
                        if !(byte.is_ascii_alphanumeric() || byte == b'_') {
                            return Err(Error::InvalidName {
                                pos: spec_start + offset,
                            });
                        }
                    }

                    let name = &template[spec_start..spec_end];
                    if seen.contains(&name) {
                        return Err(Error::DuplicateVariable { pos: spec_start });
                    }
                    seen.push(name);
                    names.push(name);

                    if spec_end == close {
                        break;
                    }
                    spec_start = spec_end + 1;
                }

                expressions.push((open, close + 1, names));
                cursor = close + 1;
            }
            _ => cursor += 1,
        }
    }

    let mut output = String::new();
    let mut literal_start = 0;
    for (open, after_close, names) in expressions {
        output.push_str(&template[literal_start..open]);
        let mut first = true;

        for name in names {
            if let Some(value) = vars
                .iter()
                .rev()
                .find_map(|&(binding, value)| (binding == name).then_some(value))
            {
                if !first {
                    output.push(',');
                }
                first = false;
                encode_value(value, &mut output);
            }
        }

        literal_start = after_close;
    }
    output.push_str(&template[literal_start..]);

    Ok(output)
}

fn encode_value(value: &str, output: &mut String) {
    let bytes = value.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex_value(bytes[index + 1]), hex_value(bytes[index + 2])) {
                output.push('%');
                output.push(hex_digit(high));
                output.push(hex_digit(low));
                index += 3;
                continue;
            }
        }

        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            output.push(byte as char);
        } else {
            output.push('%');
            output.push(hex_digit(byte >> 4));
            output.push(hex_digit(byte & 0x0f));
        }
        index += 1;
    }
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        _ => (b'A' + value - 10) as char,
    }
}