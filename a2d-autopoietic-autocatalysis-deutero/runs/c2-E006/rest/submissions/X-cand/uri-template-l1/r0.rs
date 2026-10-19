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
                let close = match bytes[open + 1..].iter().position(|&b| b == b'}') {
                    Some(offset) => open + 1 + offset,
                    None => return Err(Error::Unclosed { pos: open }),
                };

                if let Some(offset) = bytes[open + 1..close].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen {
                        pos: open + 1 + offset,
                    });
                }

                let mut content_start = open + 1;
                if content_start < close
                    && matches!(bytes[content_start], b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&')
                {
                    return Err(Error::UnsupportedOperator { pos: content_start });
                }

                if content_start == close {
                    return Err(Error::EmptyExpression { pos: close });
                }

                let mut names = Vec::new();
                let mut seen: Vec<&str> = Vec::new();
                loop {
                    let spec_start = content_start;
                    let spec_end = bytes[spec_start..close]
                        .iter()
                        .position(|&b| b == b',')
                        .map(|offset| spec_start + offset)
                        .unwrap_or(close);

                    if spec_start == spec_end {
                        return Err(Error::EmptyVariable { pos: spec_start });
                    }

                    let name_bytes = &bytes[spec_start..spec_end];
                    let valid_first = |b: u8| b.is_ascii_alphabetic() || b == b'_';
                    let valid_rest = |b: u8| b.is_ascii_alphanumeric() || b == b'_';

                    if !valid_first(name_bytes[0]) {
                        return Err(Error::InvalidName { pos: spec_start });
                    }
                    if let Some(offset) = name_bytes[1..].iter().position(|&b| !valid_rest(b)) {
                        return Err(Error::InvalidName {
                            pos: spec_start + 1 + offset,
                        });
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
                    content_start = spec_end + 1;
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

        let mut fields = Vec::new();
        for name in names {
            if let Some((_, value)) = vars.iter().rev().find(|(binding, _)| *binding == name) {
                fields.push(encode_value(value));
            }
        }
        output.push_str(&fields.join(","));
        literal_start = close + 1;
    }

    output.push_str(&template[literal_start..]);
    Ok(output)
}

fn encode_value(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = String::new();
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];

        if b == b'%' && i + 2 < bytes.len() && is_hex(bytes[i + 1]) && is_hex(bytes[i + 2]) {
            output.push('%');
            output.push((bytes[i + 1] as char).to_ascii_uppercase());
            output.push((bytes[i + 2] as char).to_ascii_uppercase());
            i += 3;
        } else if is_unreserved(b) {
            output.push(b as char);
            i += 1;
        } else {
            output.push('%');
            output.push(hex_digit(b >> 4));
            output.push(hex_digit(b & 0x0f));
            i += 1;
        }
    }

    output
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~')
}

fn is_hex(b: u8) -> bool {
    b.is_ascii_hexdigit()
}

fn hex_digit(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        _ => (b'A' + nibble - 10) as char,
    }
}