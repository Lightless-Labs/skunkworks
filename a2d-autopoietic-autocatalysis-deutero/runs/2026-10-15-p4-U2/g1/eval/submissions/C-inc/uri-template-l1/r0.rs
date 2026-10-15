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
    start: usize,
    end: usize,
    variables: Vec<(String, usize)>,
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

                if let Some(relative_inner) = bytes[open + 1..close].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen {
                        pos: open + 1 + relative_inner,
                    });
                }

                let mut content_start = open + 1;
                if content_start == close {
                    return Err(Error::EmptyExpression { pos: close });
                }

                if matches!(bytes[content_start], b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&') {
                    return Err(Error::UnsupportedOperator { pos: content_start });
                }

                let mut specifiers = Vec::new();
                let mut spec_start = content_start;
                loop {
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

                    let name = std::str::from_utf8(name_bytes).expect("validated ASCII name").to_owned();
                    if specifiers.iter().any(|(existing, _): &(String, usize)| existing == &name) {
                        return Err(Error::DuplicateVariable { pos: spec_start });
                    }
                    specifiers.push((name, spec_start));

                    if spec_end == close {
                        break;
                    }
                    spec_start = spec_end + 1;
                }

                expressions.push(Expression {
                    start: open,
                    end: close,
                    variables: specifiers,
                });
                i = close + 1;
            }
            _ => i += 1,
        }
    }

    let mut output = String::new();
    let mut cursor = 0;
    for expression in expressions {
        output.push_str(&template[cursor..expression.start]);

        let mut fields = Vec::new();
        for (name, _) in expression.variables {
            if let Some((_, value)) = vars.iter().rev().find(|(binding_name, _)| *binding_name == name) {
                fields.push(encode_value(value));
            }
        }
        output.push_str(&fields.join(","));

        cursor = expression.end + 1;
    }
    output.push_str(&template[cursor..]);

    Ok(output)
}

fn encode_value(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut encoded = String::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() && is_hex(bytes[i + 1]) && is_hex(bytes[i + 2]) {
            encoded.push('%');
            encoded.push((bytes[i + 1] as char).to_ascii_uppercase());
            encoded.push((bytes[i + 2] as char).to_ascii_uppercase());
            i += 3;
        } else {
            let byte = bytes[i];
            if is_unreserved(byte) {
                encoded.push(byte as char);
            } else {
                const HEX: &[u8; 16] = b"0123456789ABCDEF";
                encoded.push('%');
                encoded.push(HEX[(byte >> 4) as usize] as char);
                encoded.push(HEX[(byte & 0x0F) as usize] as char);
            }
            i += 1;
        }
    }

    encoded
}

fn is_hex(byte: u8) -> bool {
    byte.is_ascii_hexdigit()
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

fn main() {}