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

#[derive(Clone, Copy)]
struct Variable<'a> {
    name: &'a str,
    start: usize,
}

struct Expression<'a> {
    operator: Option<char>,
    variables: Vec<Variable<'a>>,
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

fn parse_expression(content: &str, content_start: usize, close_pos: usize) -> Result<Expression<'_>, Error> {
    let bytes = content.as_bytes();
    let mut offset = 0;
    let mut operator = None;

    if let Some(&byte) = bytes.first() {
        if matches!(byte, b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&') {
            if !matches!(byte, b'+' | b'#') {
                return Err(Error::UnsupportedOperator {
                    pos: content_start,
                });
            }
            operator = Some(byte as char);
            offset = 1;
        }
    }

    if offset == bytes.len() {
        return Err(Error::EmptyExpression { pos: close_pos });
    }

    let mut variables = Vec::new();
    let mut seen = Vec::<&str>::new();
    let mut segment_start = offset;

    loop {
        let segment_end = bytes[segment_start..]
            .iter()
            .position(|&byte| byte == b',')
            .map(|relative| segment_start + relative)
            .unwrap_or(bytes.len());

        if segment_start == segment_end {
            return Err(Error::EmptyVariable {
                pos: content_start + segment_start,
            });
        }

        let name = &content[segment_start..segment_end];
        let name_bytes = name.as_bytes();

        if !is_name_start(name_bytes[0]) {
            return Err(Error::InvalidName {
                pos: content_start + segment_start,
            });
        }

        for (index, &byte) in name_bytes.iter().enumerate().skip(1) {
            if !is_name_continue(byte) {
                return Err(Error::InvalidName {
                    pos: content_start + segment_start + index,
                });
            }
        }

        if seen.iter().any(|previous| *previous == name) {
            return Err(Error::DuplicateVariable {
                pos: content_start + segment_start,
            });
        }

        seen.push(name);
        variables.push(Variable {
            name,
            start: content_start + segment_start,
        });

        if segment_end == bytes.len() {
            break;
        }
        segment_start = segment_end + 1;
    }

    Ok(Expression {
        operator,
        variables,
    })
}

fn validate_template(template: &str) -> Result<Vec<Expression<'_>>, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut cursor = 0;

    while cursor < bytes.len() {
        match bytes[cursor] {
            b'}' => return Err(Error::UnexpectedClose { pos: cursor }),
            b'{' => {
                let open_pos = cursor;
                let Some(relative_close) = bytes[cursor + 1..]
                    .iter()
                    .position(|&byte| byte == b'}')
                else {
                    return Err(Error::Unclosed { pos: open_pos });
                };

                let close_pos = cursor + 1 + relative_close;
                if let Some(relative_open) = bytes[cursor + 1..close_pos]
                    .iter()
                    .position(|&byte| byte == b'{')
                {
                    return Err(Error::NestedOpen {
                        pos: cursor + 1 + relative_open,
                    });
                }

                let content_start = open_pos + 1;
                let content = &template[content_start..close_pos];
                expressions.push(parse_expression(content, content_start, close_pos)?);
                cursor = close_pos + 1;
            }
            _ => cursor += 1,
        }
    }

    Ok(expressions)
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

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn hex_digit(value: u8) -> u8 {
    match value {
        0..=9 => b'0' + value,
        _ => b'A' + (value - 10),
    }
}

fn encode_value(value: &str, allow_reserved: bool) -> String {
    let bytes = value.as_bytes();
    let mut output = String::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        let byte = bytes[index];

        if byte == b'%' {
            if index + 2 < bytes.len() {
                if let (Some(high), Some(low)) =
                    (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
                {
                    output.push('%');
                    output.push(hex_digit(high) as char);
                    output.push(hex_digit(low) as char);
                    index += 3;
                    continue;
                }
            }
            output.push_str("%25");
            index += 1;
            continue;
        }

        if is_unreserved(byte) || (allow_reserved && is_reserved(byte)) {
            output.push(byte as char);
        } else {
            output.push('%');
            output.push(hex_digit(byte >> 4) as char);
            output.push(hex_digit(byte & 0x0f) as char);
        }
        index += 1;
    }

    output
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let expressions = validate_template(template)?;
    let mut output = String::with_capacity(template.len());
    let mut cursor = 0;

    for expression in expressions {
        let open_pos = expression.variables.first().map_or(cursor, |variable| {
            let _ = variable;
            // The expression's first name starts after its opening brace and optional operator.
            cursor
        });
        let open_pos = template[cursor..]
            .find('{')
            .map(|relative| cursor + relative)
            .unwrap_or(open_pos);

        output.push_str(&template[cursor..open_pos]);

        let allow_reserved = matches!(expression.operator, Some('+' | '#'));
        let mut fields = Vec::new();

        for variable in &expression.variables {
            if let Some((_, value)) = vars.iter().rev().find(|(name, _)| *name == variable.name) {
                fields.push(encode_value(value, allow_reserved));
            }
        }

        if !fields.is_empty() {
            if expression.operator == Some('#') {
                output.push('#');
            }
            for (index, field) in fields.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                output.push_str(field);
            }
        }

        let close_pos = template[open_pos + 1..]
            .find('}')
            .map(|relative| open_pos + 1 + relative)
            .expect("validated template has a closing brace");
        cursor = close_pos + 1;
    }

    output.push_str(&template[cursor..]);
    Ok(output)
}