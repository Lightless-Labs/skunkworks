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

                if let Some(relative_open) = bytes[open + 1..close].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen {
                        pos: open + 1 + relative_open,
                    });
                }

                let mut content_start = open + 1;
                let content_end = close;

                if content_start < content_end
                    && matches!(bytes[content_start], b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&')
                {
                    return Err(Error::UnsupportedOperator { pos: content_start });
                }

                if content_start == content_end {
                    return Err(Error::EmptyExpression { pos: close });
                }

                let mut start = content_start;
                let mut seen: Vec<&str> = Vec::new();

                loop {
                    let end = bytes[start..content_end]
                        .iter()
                        .position(|&b| b == b',')
                        .map(|offset| start + offset)
                        .unwrap_or(content_end);

                    if start == end {
                        return Err(Error::EmptyVariable { pos: start });
                    }

                    let name_bytes = &bytes[start..end];
                    let valid_first =
                        name_bytes[0].is_ascii_alphabetic() || name_bytes[0] == b'_';
                    if !valid_first {
                        return Err(Error::InvalidName { pos: start });
                    }

                    if let Some(offset) = name_bytes.iter().enumerate().skip(1).find_map(
                        |(offset, &b)| {
                            (!b.is_ascii_alphanumeric() && b != b'_').then_some(offset)
                        },
                    ) {
                        return Err(Error::InvalidName { pos: start + offset });
                    }

                    let name = &template[start..end];
                    if seen.contains(&name) {
                        return Err(Error::DuplicateVariable { pos: start });
                    }
                    seen.push(name);

                    if end == content_end {
                        break;
                    }
                    start = end + 1;
                }

                expressions.push((open, close, content_start, content_end));
                i = close + 1;
            }
            _ => i += 1,
        }
    }

    let mut output = String::new();
    let mut literal_start = 0;

    for (open, close, content_start, content_end) in expressions {
        output.push_str(&template[literal_start..open]);

        let mut first_field = true;
        let mut start = content_start;

        loop {
            let end = bytes[start..content_end]
                .iter()
                .position(|&b| b == b',')
                .map(|offset| start + offset)
                .unwrap_or(content_end);
            let name = &template[start..end];

            if let Some((_, value)) = vars.iter().rev().find(|(binding, _)| *binding == name) {
                if !first_field {
                    output.push(',');
                }
                first_field = false;
                encode_value(value, &mut output);
            }

            if end == content_end {
                break;
            }
            start = end + 1;
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
            output.push(hex_upper(bytes[i + 1]));
            output.push(hex_upper(bytes[i + 2]));
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

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

fn is_hex(byte: u8) -> bool {
    byte.is_ascii_hexdigit()
}

fn hex_upper(byte: u8) -> char {
    (byte as char).to_ascii_uppercase()
}

fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        _ => (b'A' + value - 10) as char,
    }
}