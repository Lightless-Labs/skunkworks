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
    names: Vec<(usize, usize)>,
}

fn validate(template: &str) -> Result<Vec<Expression>, Error> {
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

                if let Some(relative_open) =
                    bytes[open + 1..close].iter().position(|&b| b == b'{')
                {
                    return Err(Error::NestedOpen {
                        pos: open + 1 + relative_open,
                    });
                }

                let mut content_start = open + 1;
                if matches!(bytes.get(content_start), Some(b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&')) {
                    return Err(Error::UnsupportedOperator { pos: content_start });
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
                        .map_or(close, |offset| spec_start + offset);

                    if spec_start == spec_end {
                        return Err(Error::EmptyVariable { pos: spec_start });
                    }

                    let first = bytes[spec_start];
                    if !first.is_ascii_alphabetic() && first != b'_' {
                        return Err(Error::InvalidName { pos: spec_start });
                    }

                    for (offset, &byte) in bytes[spec_start + 1..spec_end].iter().enumerate() {
                        if !byte.is_ascii_alphanumeric() && byte != b'_' {
                            return Err(Error::InvalidName {
                                pos: spec_start + 1 + offset,
                            });
                        }
                    }

                    if names.iter().any(|&(start, end)| {
                        &bytes[start..end] == &bytes[spec_start..spec_end]
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
                    start: open,
                    end: close,
                    names,
                });
                i = close + 1;
            }
            _ => i += 1,
        }
    }

    Ok(expressions)
}

fn encode(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let bytes = value.as_bytes();
    let mut output = String::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];
        if byte == b'%' && i + 2 < bytes.len() {
            let hi = bytes[i + 1];
            let lo = bytes[i + 2];
            if hi.is_ascii_hexdigit() && lo.is_ascii_hexdigit() {
                output.push('%');
                output.push((hi as char).to_ascii_uppercase());
                output.push((lo as char).to_ascii_uppercase());
                i += 3;
                continue;
            }
        }

        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            output.push(byte as char);
        } else {
            output.push('%');
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
        }
        i += 1;
    }

    output
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let expressions = validate(template)?;
    let mut output = String::new();
    let mut cursor = 0;

    for expression in expressions {
        output.push_str(&template[cursor..expression.start]);

        let mut fields = Vec::new();
        for (start, end) in expression.names {
            let name = &template[start..end];
            if let Some((_, value)) = vars.iter().rev().find(|(key, _)| *key == name) {
                fields.push(encode(value));
            }
        }

        if !fields.is_empty() {
            output.push_str(&fields.join(","));
        }
        cursor = expression.end + 1;
    }

    output.push_str(&template[cursor..]);
    Ok(output)
}