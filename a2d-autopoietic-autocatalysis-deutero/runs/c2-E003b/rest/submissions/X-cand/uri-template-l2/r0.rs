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

#[derive(Clone)]
struct Expression {
    start: usize,
    end: usize,
    operator: Option<char>,
    names: Vec<(usize, usize)>,
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_byte(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

fn validate(template: &str) -> Result<Vec<Expression>, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut cursor = 0;

    while cursor < bytes.len() {
        match bytes[cursor] {
            b'}' => return Err(Error::UnexpectedClose { pos: cursor }),
            b'{' => {
                let start = cursor;
                let close = match bytes[start + 1..].iter().position(|&b| b == b'}') {
                    Some(offset) => start + 1 + offset,
                    None => return Err(Error::Unclosed { pos: start }),
                };

                if let Some(offset) = bytes[start + 1..close].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen { pos: start + 1 + offset });
                }

                let mut content_start = start + 1;
                let operator = match bytes.get(content_start).copied() {
                    Some(b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&') => {
                        let op = bytes[content_start] as char;
                        if op != '+' && op != '#' {
                            return Err(Error::UnsupportedOperator { pos: content_start });
                        }
                        content_start += 1;
                        Some(op)
                    }
                    _ => None,
                };

                if content_start == close {
                    return Err(Error::EmptyExpression { pos: close });
                }

                let mut names = Vec::new();
                let mut seen = Vec::<&str>::new();
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

                    if !is_name_start(bytes[spec_start]) {
                        return Err(Error::InvalidName { pos: spec_start });
                    }

                    for index in spec_start + 1..spec_end {
                        if !is_name_byte(bytes[index]) {
                            return Err(Error::InvalidName { pos: index });
                        }
                    }

                    let name = &template[spec_start..spec_end];
                    if seen.iter().any(|&previous| previous == name) {
                        return Err(Error::DuplicateVariable { pos: spec_start });
                    }
                    seen.push(name);
                    names.push((spec_start, spec_end));

                    if spec_end == close {
                        break;
                    }
                    spec_start = spec_end + 1;
                }

                expressions.push(Expression {
                    start,
                    end: close + 1,
                    operator,
                    names,
                });
                cursor = close + 1;
            }
            _ => cursor += 1,
        }
    }

    Ok(expressions)
}

fn is_hex(byte: u8) -> bool {
    byte.is_ascii_hexdigit()
}

fn hex_value(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        _ => 0,
    }
}

fn upper_hex(value: u8) -> u8 {
    if value < 10 {
        b'0' + value
    } else {
        b'A' + (value - 10)
    }
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

fn is_reserved(byte: u8) -> bool {
    matches!(
        byte,
        b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@' | b'!' | b'$' | b'&' | b'\''
            | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
    )
}

fn encode(value: &str, allow_reserved: bool) -> String {
    let bytes = value.as_bytes();
    let mut result = String::with_capacity(bytes.len());

    let mut i = 0;
    while i < bytes.len() {
        let byte = bytes[i];

        if byte == b'%' && i + 2 < bytes.len() && is_hex(bytes[i + 1]) && is_hex(bytes[i + 2]) {
            result.push('%');
            result.push(upper_hex(hex_value(bytes[i + 1])) as char);
            result.push(upper_hex(hex_value(bytes[i + 2])) as char);
            i += 3;
        } else if is_unreserved(byte) || (allow_reserved && is_reserved(byte)) {
            result.push(byte as char);
            i += 1;
        } else {
            result.push('%');
            result.push(upper_hex(byte >> 4) as char);
            result.push(upper_hex(byte & 0x0f) as char);
            i += 1;
        }
    }

    result
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let expressions = validate(template)?;
    let mut output = String::with_capacity(template.len());
    let mut literal_start = 0;

    for expression in expressions {
        output.push_str(&template[literal_start..expression.start]);

        let allow_reserved = matches!(expression.operator, Some('+' | '#'));
        let mut fields = Vec::new();

        for (name_start, name_end) in expression.names {
            let name = &template[name_start..name_end];
            if let Some((_, value)) = vars.iter().rev().find(|(key, _)| *key == name) {
                fields.push(encode(value, allow_reserved));
            }
        }

        if !fields.is_empty() {
            if expression.operator == Some('#') {
                output.push('#');
            }
            output.push_str(&fields.join(","));
        }

        literal_start = expression.end;
    }

    output.push_str(&template[literal_start..]);
    Ok(output)
}