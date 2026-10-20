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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Operator {
    Simple,
    Reserved,
    Fragment,
    Label,
    Path,
    Matrix,
    Query,
    Continuation,
}

struct Expression {
    operator: Operator,
    variables: Vec<String>,
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

fn parse_expression(content: &str, content_pos: usize, close_pos: usize) -> Result<Expression, Error> {
    let bytes = content.as_bytes();
    let (operator, offset) = if let Some(&first) = bytes.first() {
        let recognized = match first {
            b'+' => Some(Operator::Reserved),
            b'#' => Some(Operator::Fragment),
            b'.' => Some(Operator::Label),
            b'/' => Some(Operator::Path),
            b';' => Some(Operator::Matrix),
            b'?' => Some(Operator::Query),
            b'&' => Some(Operator::Continuation),
            _ => None,
        };
        if let Some(operator) = recognized {
            (operator, 1)
        } else {
            (Operator::Simple, 0)
        }
    } else {
        (Operator::Simple, 0)
    };

    let remaining = &content[offset..];
    let remaining_bytes = remaining.as_bytes();
    if remaining_bytes.is_empty() {
        return Err(Error::EmptyExpression { pos: close_pos });
    }

    let mut variables = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    let mut start = 0;

    for end in (0..=remaining_bytes.len())
        .filter(|&index| index == remaining_bytes.len() || remaining_bytes[index] == b',')
    {
        if end == start {
            return Err(Error::EmptyVariable {
                pos: content_pos + offset + start,
            });
        }

        let specifier = &remaining[start..end];
        let specifier_bytes = specifier.as_bytes();
        let mut invalid_at = None;

        if !is_name_start(specifier_bytes[0]) {
            invalid_at = Some(0);
        } else if let Some(index) = specifier_bytes
            .iter()
            .enumerate()
            .skip(1)
            .find_map(|(index, &byte)| (!is_name_continue(byte)).then_some(index))
        {
            invalid_at = Some(index);
        }

        if let Some(index) = invalid_at {
            return Err(Error::InvalidName {
                pos: content_pos + offset + start + index,
            });
        }

        if seen.iter().any(|&name| name == specifier) {
            return Err(Error::DuplicateVariable {
                pos: content_pos + offset + start,
            });
        }

        seen.push(specifier);
        variables.push(specifier.to_owned());
        start = end + 1;
    }

    Ok(Expression {
        operator,
        variables,
    })
}

fn is_hex(byte: u8) -> bool {
    byte.is_ascii_hexdigit()
}

fn encode_value(value: &str, reserved: bool) -> String {
    let bytes = value.as_bytes();
    let mut output = String::new();
    let mut index = 0;

    while index < bytes.len() {
        let byte = bytes[index];

        if byte == b'%' && index + 2 < bytes.len() && is_hex(bytes[index + 1]) && is_hex(bytes[index + 2]) {
            output.push('%');
            output.push((bytes[index + 1] as char).to_ascii_uppercase());
            output.push((bytes[index + 2] as char).to_ascii_uppercase());
            index += 3;
            continue;
        }

        let unreserved = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~');
        let reserved_byte = matches!(
            byte,
            b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@' | b'!' | b'$' | b'&' | b'\''
                | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
        );

        if unreserved || (reserved && reserved_byte) {
            output.push(byte as char);
        } else {
            output.push('%');
            output.push(char::from_digit((byte >> 4) as u32, 16).unwrap().to_ascii_uppercase());
            output.push(char::from_digit((byte & 0x0f) as u32, 16).unwrap().to_ascii_uppercase());
        }
        index += 1;
    }

    output
}

fn expand_expression(expression: &Expression, vars: &[(&str, &str)]) -> String {
    let mut fields = Vec::new();
    let reserved = matches!(expression.operator, Operator::Reserved | Operator::Fragment);

    for name in &expression.variables {
        if let Some((_, value)) = vars.iter().rev().find(|(binding, _)| *binding == name) {
            let encoded = encode_value(value, reserved);
            let field = match expression.operator {
                Operator::Matrix => {
                    if encoded.is_empty() {
                        name.clone()
                    } else {
                        format!("{name}={encoded}")
                    }
                }
                Operator::Query | Operator::Continuation => format!("{name}={encoded}"),
                _ => encoded,
            };
            fields.push(field);
        }
    }

    if fields.is_empty() {
        return String::new();
    }

    let (prefix, separator) = match expression.operator {
        Operator::Simple | Operator::Reserved => ("", ","),
        Operator::Fragment => ("#", ","),
        Operator::Label => (".", "."),
        Operator::Path => ("/", "/"),
        Operator::Matrix => (";", ";"),
        Operator::Query => ("?", "&"),
        Operator::Continuation => ("&", "&"),
    };

    let mut output = String::from(prefix);
    output.push_str(&fields.join(separator));
    output
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut cursor = 0;

    while cursor < bytes.len() {
        match bytes[cursor] {
            b'}' => return Err(Error::UnexpectedClose { pos: cursor }),
            b'{' => {
                let open_pos = cursor;
                let close_rel = bytes[cursor + 1..]
                    .iter()
                    .position(|&byte| byte == b'}')
                    .ok_or(Error::Unclosed { pos: open_pos })?;
                let close_pos = cursor + 1 + close_rel;

                if let Some(inner_rel) = bytes[cursor + 1..close_pos]
                    .iter()
                    .position(|&byte| byte == b'{')
                {
                    return Err(Error::NestedOpen {
                        pos: cursor + 1 + inner_rel,
                    });
                }

                let content = &template[cursor + 1..close_pos];
                let expression = parse_expression(content, cursor + 1, close_pos)?;
                expressions.push((open_pos, close_pos + 1, expression));
                cursor = close_pos + 1;
            }
            _ => cursor += 1,
        }
    }

    let mut output = String::new();
    let mut literal_start = 0;
    for (start, end, expression) in expressions {
        output.push_str(&template[literal_start..start]);
        output.push_str(&expand_expression(&expression, vars));
        literal_start = end;
    }
    output.push_str(&template[literal_start..]);
    Ok(output)
}