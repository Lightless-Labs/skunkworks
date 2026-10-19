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
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'}' => return Err(Error::UnexpectedClose { pos: i }),
            b'{' => {
                let open = i;
                let close = match bytes[i + 1..].iter().position(|&b| b == b'}') {
                    Some(offset) => i + 1 + offset,
                    None => return Err(Error::Unclosed { pos: open }),
                };

                if let Some(offset) = bytes[i + 1..close].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen { pos: i + 1 + offset });
                }

                let content_start = open + 1;
                let mut spec_start = content_start;
                if content_start < close && matches!(bytes[content_start], b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&') {
                    return Err(Error::UnsupportedOperator { pos: content_start });
                }

                if spec_start == close {
                    return Err(Error::EmptyExpression { pos: close });
                }

                let mut seen: Vec<&str> = Vec::new();
                loop {
                    let comma = bytes[spec_start..close]
                        .iter()
                        .position(|&b| b == b',')
                        .map(|offset| spec_start + offset);
                    let spec_end = comma.unwrap_or(close);

                    if spec_start == spec_end {
                        return Err(Error::EmptyVariable { pos: spec_start });
                    }

                    let name = &template[spec_start..spec_end];
                    let name_bytes = name.as_bytes();
                    if !is_name_start(name_bytes[0]) {
                        return Err(Error::InvalidName { pos: spec_start });
                    }
                    if let Some(offset) = name_bytes[1..]
                        .iter()
                        .position(|&b| !is_name_continue(b))
                    {
                        return Err(Error::InvalidName {
                            pos: spec_start + 1 + offset,
                        });
                    }
                    if seen.contains(&name) {
                        return Err(Error::DuplicateVariable { pos: spec_start });
                    }
                    seen.push(name);

                    if let Some(comma_pos) = comma {
                        spec_start = comma_pos + 1;
                    } else {
                        break;
                    }
                }

                expressions.push((open, close, content_start, close));
                i = close + 1;
            }
            _ => i += 1,
        }
    }

    let mut output = String::new();
    let mut literal_start = 0;
    for (open, close, content_start, content_end) in expressions {
        output.push_str(&template[literal_start..open]);

        let mut fields = Vec::new();
        let mut spec_start = content_start;
        loop {
            let comma = bytes[spec_start..content_end]
                .iter()
                .position(|&b| b == b',')
                .map(|offset| spec_start + offset);
            let spec_end = comma.unwrap_or(content_end);
            let name = &template[spec_start..spec_end];

            if let Some(value) = vars.iter().rev().find_map(|(binding, value)| {
                if *binding == name {
                    Some(*value)
                } else {
                    None
                }
            }) {
                fields.push(encode_value(value));
            }

            if let Some(comma_pos) = comma {
                spec_start = comma_pos + 1;
            } else {
                break;
            }
        }

        if !fields.is_empty() {
            output.push_str(&fields.join(","));
        }
        literal_start = close + 1;
    }
    output.push_str(&template[literal_start..]);
    Ok(output)
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

fn encode_value(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut encoded = String::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() && bytes[i + 1].is_ascii_hexdigit() && bytes[i + 2].is_ascii_hexdigit() {
            encoded.push('%');
            encoded.push((bytes[i + 1] as char).to_ascii_uppercase());
            encoded.push((bytes[i + 2] as char).to_ascii_uppercase());
            i += 3;
        } else if is_unreserved(bytes[i]) {
            encoded.push(bytes[i] as char);
            i += 1;
        } else {
            encoded.push('%');
            encoded.push(hex_digit(bytes[i] >> 4));
            encoded.push(hex_digit(bytes[i] & 0x0f));
            i += 1;
        }
    }

    encoded
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

fn hex_digit(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        _ => (b'A' + nibble - 10) as char,
    }
}