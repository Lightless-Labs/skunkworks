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

struct Expression {
    start: usize,
    end: usize,
    names: Vec<(usize, String)>,
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn validate_expression(template: &str, start: usize, end: usize) -> Result<Expression, Error> {
    let bytes = template.as_bytes();
    let mut content_start = start + 1;

    if content_start == end {
        return Err(Error::EmptyExpression { pos: end });
    }

    if matches!(bytes[content_start], b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&') {
        return Err(Error::UnsupportedOperator { pos: content_start });
    }

    let mut names = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    let mut spec_start = content_start;

    loop {
        let spec_end = bytes[spec_start..end]
            .iter()
            .position(|&b| b == b',')
            .map(|offset| spec_start + offset)
            .unwrap_or(end);

        if spec_start == spec_end {
            return Err(Error::EmptyVariable { pos: spec_start });
        }

        if !is_name_start(bytes[spec_start]) {
            return Err(Error::InvalidName { pos: spec_start });
        }

        for pos in spec_start + 1..spec_end {
            if !is_name_byte(bytes[pos]) {
                return Err(Error::InvalidName { pos });
            }
        }

        let name = template[spec_start..spec_end].to_owned();
        if seen.iter().any(|existing| existing == &name) {
            return Err(Error::DuplicateVariable { pos: spec_start });
        }
        seen.push(name.clone());
        names.push((spec_start, name));

        if spec_end == end {
            break;
        }
        spec_start = spec_end + 1;
    }

    content_start = end;
    let _ = content_start;

    Ok(Expression { start, end, names })
}

fn validate_template(template: &str) -> Result<Vec<Expression>, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut pos = 0;

    while pos < bytes.len() {
        match bytes[pos] {
            b'}' => return Err(Error::UnexpectedClose { pos }),
            b'{' => {
                let start = pos;
                let Some(relative_end) = bytes[pos + 1..].iter().position(|&b| b == b'}') else {
                    return Err(Error::Unclosed { pos: start });
                };
                let end = pos + 1 + relative_end;

                if let Some(relative_open) = bytes[pos + 1..end].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen {
                        pos: pos + 1 + relative_open,
                    });
                }

                expressions.push(validate_expression(template, start, end)?);
                pos = end + 1;
            }
            _ => pos += 1,
        }
    }

    Ok(expressions)
}

fn is_hex(byte: u8) -> bool {
    byte.is_ascii_hexdigit()
}

fn uppercase_hex(byte: u8) -> u8 {
    byte.to_ascii_uppercase()
}

fn encode_value(value: &str, output: &mut String) {
    let bytes = value.as_bytes();
    let mut pos = 0;
    const HEX: &[u8; 16] = b"0123456789ABCDEF";

    while pos < bytes.len() {
        let byte = bytes[pos];

        if byte == b'%' && pos + 2 < bytes.len() && is_hex(bytes[pos + 1]) && is_hex(bytes[pos + 2]) {
            output.push('%');
            output.push(uppercase_hex(bytes[pos + 1]) as char);
            output.push(uppercase_hex(bytes[pos + 2]) as char);
            pos += 3;
        } else if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            output.push(byte as char);
            pos += 1;
        } else {
            output.push('%');
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
            pos += 1;
        }
    }
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let expressions = validate_template(template)?;
    let mut output = String::new();
    let mut cursor = 0;

    for expression in expressions {
        output.push_str(&template[cursor..expression.start]);

        let mut first = true;
        for (_, name) in expression.names {
            if let Some((_, value)) = vars.iter().rev().find(|(binding_name, _)| *binding_name == name) {
                if !first {
                    output.push(',');
                }
                encode_value(value, &mut output);
                first = false;
            }
        }

        cursor = expression.end + 1;
    }

    output.push_str(&template[cursor..]);
    Ok(output)
}