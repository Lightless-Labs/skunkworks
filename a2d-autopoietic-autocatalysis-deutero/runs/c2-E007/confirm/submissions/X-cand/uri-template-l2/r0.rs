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
    names: Vec<(String, usize)>,
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_byte(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

fn validate_expression(
    template: &str,
    open: usize,
    close: usize,
) -> Result<Expression, Error> {
    let bytes = template.as_bytes();
    let content_start = open + 1;

    if let Some(relative) = bytes[content_start..close]
        .iter()
        .position(|&byte| byte == b'{')
    {
        return Err(Error::NestedOpen {
            pos: content_start + relative,
        });
    }

    let mut spec_start = content_start;
    let operator = if spec_start < close
        && matches!(bytes[spec_start], b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&')
    {
        let byte = bytes[spec_start];
        let pos = spec_start;
        spec_start += 1;

        match byte {
            b'+' => Some('+'),
            b'#' => Some('#'),
            _ => return Err(Error::UnsupportedOperator { pos }),
        }
    } else {
        None
    };

    if spec_start == close {
        return Err(Error::EmptyExpression { pos: close });
    }

    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut part_start = spec_start;

    loop {
        let part_end = bytes[part_start..close]
            .iter()
            .position(|&byte| byte == b',')
            .map_or(close, |relative| part_start + relative);

        if part_start == part_end {
            return Err(Error::EmptyVariable { pos: part_start });
        }

        let first = bytes[part_start];
        if !is_name_start(first) {
            return Err(Error::InvalidName { pos: part_start });
        }

        for pos in part_start + 1..part_end {
            if !is_name_byte(bytes[pos]) {
                return Err(Error::InvalidName { pos });
            }
        }

        let name = template[part_start..part_end].to_owned();
        if !seen.insert(name.clone()) {
            return Err(Error::DuplicateVariable { pos: part_start });
        }
        names.push((name, part_start));

        if part_end == close {
            break;
        }
        part_start = part_end + 1;
    }

    Ok(Expression {
        start: open,
        end: close,
        operator,
        names,
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
                let open = cursor;
                let close = match bytes[open + 1..]
                    .iter()
                    .position(|&byte| byte == b'}')
                {
                    Some(relative) => open + 1 + relative,
                    None => return Err(Error::Unclosed { pos: open }),
                };

                expressions.push(validate_expression(template, open, close)?);
                cursor = close + 1;
            }
            _ => cursor += 1,
        }
    }

    Ok(expressions)
}

fn encode_value(value: &str, allow_reserved: bool) -> String {
    let bytes = value.as_bytes();
    let mut output = String::new();
    let mut cursor = 0;

    while cursor < bytes.len() {
        let byte = bytes[cursor];

        if byte == b'%' && cursor + 2 < bytes.len() {
            let high = bytes[cursor + 1];
            let low = bytes[cursor + 2];
            if high.is_ascii_hexdigit() && low.is_ascii_hexdigit() {
                output.push('%');
                output.push((high as char).to_ascii_uppercase());
                output.push((low as char).to_ascii_uppercase());
                cursor += 3;
                continue;
            }
        }

        let unreserved = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~');
        let reserved = allow_reserved
            && matches!(
                byte,
                b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@' | b'!' | b'$' | b'&'
                    | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
            );

        if unreserved || reserved {
            output.push(byte as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            output.push('%');
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0F) as usize] as char);
        }
        cursor += 1;
    }

    output
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let expressions = validate_template(template)?;
    let mut output = String::new();
    let mut cursor = 0;

    for expression in expressions {
        output.push_str(&template[cursor..expression.start]);

        let allow_reserved = matches!(expression.operator, Some('+') | Some('#'));
        let mut fields = Vec::new();

        for (name, _) in expression.names {
            if let Some((_, value)) = vars.iter().rev().find(|(key, _)| *key == name) {
                fields.push(encode_value(value, allow_reserved));
            }
        }

        if !fields.is_empty() {
            if expression.operator == Some('#') {
                output.push('#');
            }
            output.push_str(&fields.join(","));
        }

        cursor = expression.end + 1;
    }

    output.push_str(&template[cursor..]);
    Ok(output)
}