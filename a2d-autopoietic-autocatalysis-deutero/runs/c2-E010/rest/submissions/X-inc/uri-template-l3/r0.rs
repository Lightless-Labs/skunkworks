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

#[derive(Debug, Clone)]
struct Expression {
    start: usize,
    end: usize,
    operator: Option<u8>,
    variables: Vec<(String, usize)>,
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_byte(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

fn validate_expression(template: &str, start: usize, end: usize) -> Result<Expression, Error> {
    let bytes = template.as_bytes();
    let mut content_start = start + 1;
    let operator = if matches!(
        bytes.get(content_start),
        Some(b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&')
    ) {
        let op = bytes[content_start];
        content_start += 1;
        Some(op)
    } else {
        None
    };

    let content = &bytes[content_start..end];
    if content.is_empty() {
        return Err(Error::EmptyExpression { pos: end });
    }

    let mut variables = Vec::new();
    let mut seen = Vec::<String>::new();
    let mut item_start = content_start;

    loop {
        let item_end = bytes[item_start..end]
            .iter()
            .position(|&b| b == b',')
            .map(|offset| item_start + offset)
            .unwrap_or(end);

        if item_start == item_end {
            return Err(Error::EmptyVariable { pos: item_start });
        }

        let name_bytes = &bytes[item_start..item_end];
        if !is_name_start(name_bytes[0]) {
            return Err(Error::InvalidName { pos: item_start });
        }
        if let Some(offset) = name_bytes.iter().position(|&b| !is_name_byte(b)) {
            return Err(Error::InvalidName {
                pos: item_start + offset,
            });
        }

        let name = template[item_start..item_end].to_owned();
        if seen.iter().any(|existing| existing == &name) {
            return Err(Error::DuplicateVariable { pos: item_start });
        }
        seen.push(name.clone());
        variables.push((name, item_start));

        if item_end == end {
            break;
        }
        item_start = item_end + 1;
    }

    Ok(Expression {
        start,
        end,
        operator,
        variables,
    })
}

fn validate_template(template: &str) -> Result<Vec<Expression>, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut cursor = 0;

    while cursor < bytes.len() {
        match bytes[cursor] {
            b'}' => return Err(Error::UnexpectedClose { pos: cursor }),
            b'{' => {
                let start = cursor;
                let Some(relative_end) = bytes[cursor + 1..]
                    .iter()
                    .position(|&b| b == b'}')
                else {
                    return Err(Error::Unclosed { pos: start });
                };
                let end = cursor + 1 + relative_end;

                if let Some(offset) = bytes[start + 1..end].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen {
                        pos: start + 1 + offset,
                    });
                }

                expressions.push(validate_expression(template, start, end)?);
                cursor = end + 1;
            }
            _ => cursor += 1,
        }
    }

    Ok(expressions)
}

fn lookup<'a>(vars: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    vars.iter()
        .rev()
        .find(|(binding_name, _)| *binding_name == name)
        .map(|(_, value)| *value)
}

fn is_hex(byte: u8) -> bool {
    byte.is_ascii_hexdigit()
}

fn hex_value(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte,
        b'a'..=b'f' => byte - b'a' + b'A',
        _ => byte,
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
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let bytes = value.as_bytes();
    let mut output = String::new();
    let mut index = 0;

    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'%' && index + 2 < bytes.len() && is_hex(bytes[index + 1]) && is_hex(bytes[index + 2]) {
            output.push('%');
            output.push(hex_value(bytes[index + 1]) as char);
            output.push(hex_value(bytes[index + 2]) as char);
            index += 3;
        } else if is_unreserved(byte) || (allow_reserved && is_reserved(byte)) {
            output.push(byte as char);
            index += 1;
        } else {
            output.push('%');
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
            index += 1;
        }
    }

    output
}

fn expand_expression(expression: &Expression, vars: &[(&str, &str)]) -> String {
    let operator = expression.operator;
    let allow_reserved = matches!(operator, Some(b'+') | Some(b'#'));
    let named = matches!(operator, Some(b';') | Some(b'?') | Some(b'&'));

    let mut fields = Vec::new();
    for (name, _) in &expression.variables {
        if let Some(value) = lookup(vars, name) {
            let encoded = encode(value, allow_reserved);
            let field = if named {
                if operator == Some(b';') && encoded.is_empty() {
                    name.clone()
                } else {
                    format!("{name}={encoded}")
                }
            } else {
                encoded
            };
            fields.push(field);
        }
    }

    if fields.is_empty() {
        return String::new();
    }

    let (separator, prefix) = match operator {
        Some(b'#') => (",", "#"),
        Some(b'.') => (".", "."),
        Some(b'/') => ("/", "/"),
        Some(b';') => (";", ";"),
        Some(b'?') => ("&", "?"),
        Some(b'&') => ("&", "&"),
        _ => (",", ""),
    };

    format!("{prefix}{}", fields.join(separator))
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let expressions = validate_template(template)?;
    if expressions.is_empty() {
        return Ok(template.to_owned());
    }

    let mut output = String::new();
    let mut cursor = 0;

    for expression in &expressions {
        output.push_str(&template[cursor..expression.start]);
        output.push_str(&expand_expression(expression, vars));
        cursor = expression.end + 1;
    }

    output.push_str(&template[cursor..]);
    Ok(output)
}