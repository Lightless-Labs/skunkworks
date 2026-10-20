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
    operator: Option<u8>,
    names: Vec<(usize, String)>,
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut cursor = 0;

    while cursor < bytes.len() {
        match bytes[cursor] {
            b'}' => return Err(Error::UnexpectedClose { pos: cursor }),
            b'{' => {
                let start = cursor;
                let Some(relative_end) = bytes[cursor + 1..].iter().position(|&b| b == b'}') else {
                    return Err(Error::Unclosed { pos: start });
                };
                let end = cursor + 1 + relative_end;

                if let Some(relative_open) = bytes[start + 1..end].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen {
                        pos: start + 1 + relative_open,
                    });
                }

                let mut content_start = start + 1;
                let operator = if content_start < end
                    && matches!(
                        bytes[content_start],
                        b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&'
                    )
                {
                    let op = bytes[content_start];
                    if !matches!(op, b'+' | b'#') {
                        return Err(Error::UnsupportedOperator { pos: content_start });
                    }
                    content_start += 1;
                    Some(op)
                } else {
                    None
                };

                if content_start == end {
                    return Err(Error::EmptyExpression { pos: end });
                }

                let mut names = Vec::new();
                let mut spec_start = content_start;
                let mut seen = std::collections::HashSet::new();

                loop {
                    let spec_end = bytes[spec_start..end]
                        .iter()
                        .position(|&b| b == b',')
                        .map(|relative| spec_start + relative)
                        .unwrap_or(end);

                    if spec_start == spec_end {
                        return Err(Error::EmptyVariable { pos: spec_start });
                    }

                    let spec = &bytes[spec_start..spec_end];
                    if !is_name_start(spec[0]) {
                        return Err(Error::InvalidName { pos: spec_start });
                    }
                    if let Some(offset) = spec[1..].iter().position(|&b| !is_name_continue(b)) {
                        return Err(Error::InvalidName {
                            pos: spec_start + 1 + offset,
                        });
                    }

                    let name = template[spec_start..spec_end].to_owned();
                    if !seen.insert(name.clone()) {
                        return Err(Error::DuplicateVariable { pos: spec_start });
                    }
                    names.push((spec_start, name));

                    if spec_end == end {
                        break;
                    }
                    spec_start = spec_end + 1;
                }

                expressions.push(Expression {
                    start,
                    end,
                    operator,
                    names,
                });
                cursor = end + 1;
            }
            _ => cursor += 1,
        }
    }

    let mut output = String::new();
    let mut literal_start = 0;

    for expression in expressions {
        output.push_str(&template[literal_start..expression.start]);

        let mut fields = Vec::new();
        for (_, name) in &expression.names {
            if let Some(value) = lookup_last(vars, name) {
                fields.push(encode_value(value, expression.operator.is_some()));
            }
        }

        if !fields.is_empty() {
            if expression.operator == Some(b'#') {
                output.push('#');
            }
            output.push_str(&fields.join(","));
        }

        literal_start = expression.end + 1;
    }

    output.push_str(&template[literal_start..]);
    Ok(output)
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn lookup_last<'a>(vars: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    vars.iter()
        .rev()
        .find(|(binding_name, _)| *binding_name == name)
        .map(|(_, value)| *value)
}

fn encode_value(value: &str, allow_reserved: bool) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let bytes = value.as_bytes();
    let mut encoded = String::new();
    let mut cursor = 0;

    while cursor < bytes.len() {
        let byte = bytes[cursor];

        if byte == b'%' && cursor + 2 < bytes.len() && is_ascii_hex(bytes[cursor + 1])
            && is_ascii_hex(bytes[cursor + 2])
        {
            encoded.push('%');
            encoded.push((bytes[cursor + 1] as char).to_ascii_uppercase());
            encoded.push((bytes[cursor + 2] as char).to_ascii_uppercase());
            cursor += 3;
            continue;
        }

        if is_unreserved(byte) || (allow_reserved && is_reserved(byte)) {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push(HEX[(byte >> 4) as usize] as char);
            encoded.push(HEX[(byte & 0x0F) as usize] as char);
        }
        cursor += 1;
    }

    encoded
}

fn is_ascii_hex(byte: u8) -> bool {
    byte.is_ascii_hexdigit()
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