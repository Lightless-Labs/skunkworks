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
                let Some(relative_close) = bytes[i + 1..].iter().position(|&b| b == b'}') else {
                    return Err(Error::Unclosed { pos: open });
                };
                let close = i + 1 + relative_close;

                if let Some(relative_nested) = bytes[i + 1..close].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen {
                        pos: i + 1 + relative_nested,
                    });
                }

                let (operator, content_start) = match bytes.get(i + 1).copied() {
                    Some(b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&') => {
                        (Some(bytes[i + 1]), i + 2)
                    }
                    _ => (None, i + 1),
                };

                if operator.is_some() {
                    return Err(Error::UnsupportedOperator { pos: i + 1 });
                }

                if content_start == close {
                    return Err(Error::EmptyExpression { pos: close });
                }

                let mut names = Vec::new();
                let mut spec_start = content_start;

                for spec_end in (content_start..=close).filter(|&p| p == close || bytes[p] == b',') {
                    if spec_start == spec_end {
                        return Err(Error::EmptyVariable { pos: spec_start });
                    }

                    let name_bytes = &bytes[spec_start..spec_end];
                    if !is_name_start(name_bytes[0]) {
                        return Err(Error::InvalidName { pos: spec_start });
                    }
                    if let Some(offset) = name_bytes[1..].iter().position(|&b| !is_name_continue(b)) {
                        return Err(Error::InvalidName {
                            pos: spec_start + 1 + offset,
                        });
                    }

                    let name = &template[spec_start..spec_end];
                    if names.iter().any(|seen| *seen == name) {
                        return Err(Error::DuplicateVariable { pos: spec_start });
                    }
                    names.push(name);
                    spec_start = spec_end + 1;
                }

                expressions.push((open, close, names));
                i = close + 1;
            }
            _ => i += 1,
        }
    }

    let mut output = String::new();
    let mut literal_start = 0;

    for (open, close, names) in expressions {
        output.push_str(&template[literal_start..open]);

        let mut first = true;
        for name in names {
            if let Some(value) = vars.iter().rev().find(|(key, _)| *key == name).map(|(_, value)| *value) {
                if !first {
                    output.push(',');
                }
                encode_value(value, &mut output);
                first = false;
            }
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

fn encode_value(value: &str, output: &mut String) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let bytes = value.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];

        if is_unreserved(byte) {
            output.push(byte as char);
            i += 1;
        } else if byte == b'%' && i + 2 < bytes.len() && is_hex(bytes[i + 1]) && is_hex(bytes[i + 2]) {
            output.push('%');
            output.push((bytes[i + 1] as char).to_ascii_uppercase());
            output.push((bytes[i + 2] as char).to_ascii_uppercase());
            i += 3;
        } else {
            output.push('%');
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0F) as usize] as char);
            i += 1;
        }
    }
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

fn is_hex(byte: u8) -> bool {
    byte.is_ascii_hexdigit()
}