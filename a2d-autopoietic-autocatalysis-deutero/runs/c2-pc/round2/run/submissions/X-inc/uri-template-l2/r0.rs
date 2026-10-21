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
    operator: Option<char>,
    names: Vec<(usize, usize)>,
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

fn validate(template: &str) -> Result<Vec<Expression>, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'}' => return Err(Error::UnexpectedClose { pos: i }),
            b'{' => {
                let start = i;
                let close = match bytes[i + 1..].iter().position(|&b| b == b'}') {
                    Some(offset) => i + 1 + offset,
                    None => return Err(Error::Unclosed { pos: start }),
                };

                if let Some(offset) = bytes[i + 1..close].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen { pos: i + 1 + offset });
                }

                let mut content_start = start + 1;
                let mut operator = None;
                if content_start < close
                    && matches!(
                        bytes[content_start],
                        b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&'
                    )
                {
                    let op = bytes[content_start] as char;
                    if !matches!(op, '+' | '#') {
                        return Err(Error::UnsupportedOperator {
                            pos: content_start,
                        });
                    }
                    operator = Some(op);
                    content_start += 1;
                }

                if content_start == close {
                    return Err(Error::EmptyExpression { pos: close });
                }

                let mut names = Vec::new();
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

                    if names.iter().any(|&(begin, end)| {
                        &bytes[begin..end] == name_bytes
                    }) {
                        return Err(Error::DuplicateVariable { pos: spec_start });
                    }
                    names.push((spec_start, spec_end));

                    if spec_end == close {
                        break;
                    }
                    spec_start = spec_end + 1;
                }

                expressions.push(Expression {
                    start,
                    end: close,
                    operator,
                    names,
                });
                i = close + 1;
            }
            _ => i += 1,
        }
    }

    Ok(expressions)
}

fn encode(value: &str, reserved: bool) -> String {
    let bytes = value.as_bytes();
    let mut out = String::new();
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];

        if byte == b'%' && i + 2 < bytes.len() {
            let a = bytes[i + 1];
            let b = bytes[i + 2];
            if a.is_ascii_hexdigit() && b.is_ascii_hexdigit() {
                out.push('%');
                out.push((a as char).to_ascii_uppercase());
                out.push((b as char).to_ascii_uppercase());
                i += 3;
                continue;
            }
        }

        let unreserved = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~');
        let reserved_byte = reserved
            && matches!(
                byte,
                b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@' | b'!' | b'$' | b'&'
                    | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
            );

        if unreserved || reserved_byte {
            out.push(byte as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push('%');
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
        }
        i += 1;
    }

    out
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let expressions = validate(template)?;
    let bytes = template.as_bytes();
    let mut output = String::new();
    let mut literal_start = 0;

    for expression in expressions {
        output.push_str(&template[literal_start..expression.start]);

        let reserved = matches!(expression.operator, Some('+') | Some('#'));
        let mut fields = Vec::new();

        for &(start, end) in &expression.names {
            let name = &template[start..end];
            if let Some((_, value)) = vars.iter().rev().find(|(key, _)| *key == name) {
                fields.push(encode(value, reserved));
            }
        }

        if !fields.is_empty() {
            if expression.operator == Some('#') {
                output.push('#');
            }
            output.push_str(&fields.join(","));
        }

        literal_start = expression.end + 1;
    }

    output.push_str(&template[literal_start..]);
    let _ = bytes;
    Ok(output)
}