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
    end: usize,
    names: Vec<(usize, String)>,
}

fn is_operator(byte: u8) -> bool {
    matches!(byte, b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&')
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_byte(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

fn validate_expression(template: &str, open: usize, close: usize) -> Result<Expression, Error> {
    let bytes = template.as_bytes();
    let content_start = open + 1;
    let mut start = content_start;

    if start < close && is_operator(bytes[start]) {
        return Err(Error::UnsupportedOperator { pos: start });
    }

    if start == close {
        return Err(Error::EmptyExpression { pos: close });
    }

    let mut names = Vec::new();
    let mut used = Vec::<String>::new();

    loop {
        let comma = bytes[start..close]
            .iter()
            .position(|&byte| byte == b',')
            .map(|offset| start + offset)
            .unwrap_or(close);

        if comma == start {
            return Err(Error::EmptyVariable { pos: start });
        }

        let name_bytes = &bytes[start..comma];
        if !is_name_start(name_bytes[0]) {
            return Err(Error::InvalidName { pos: start });
        }
        if let Some(offset) = name_bytes.iter().position(|&byte| !is_name_byte(byte)) {
            return Err(Error::InvalidName { pos: start + offset });
        }

        let name = template[start..comma].to_owned();
        if used.iter().any(|previous| previous == &name) {
            return Err(Error::DuplicateVariable { pos: start });
        }
        used.push(name.clone());
        names.push((start, name));

        if comma == close {
            break;
        }
        start = comma + 1;
    }

    Ok(Expression { end: close, names })
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
                    Some(offset) => open + 1 + offset,
                    None => return Err(Error::Unclosed { pos: open }),
                };

                if let Some(offset) = bytes[open + 1..close]
                    .iter()
                    .position(|&byte| byte == b'{')
                {
                    return Err(Error::NestedOpen {
                        pos: open + 1 + offset,
                    });
                }

                expressions.push(validate_expression(template, open, close)?);
                cursor = close + 1;
            }
            _ => cursor += 1,
        }
    }

    Ok(expressions)
}

fn encode_value(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = String::new();
    let mut index = 0;

    while index < bytes.len() {
        let byte = bytes[index];

        if byte == b'%' && index + 2 < bytes.len() {
            let high = bytes[index + 1];
            let low = bytes[index + 2];
            if high.is_ascii_hexdigit() && low.is_ascii_hexdigit() {
                output.push('%');
                output.push((high as char).to_ascii_uppercase());
                output.push((low as char).to_ascii_uppercase());
                index += 3;
                continue;
            }
        }

        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            output.push(byte as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            output.push('%');
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
        }

        index += 1;
    }

    output
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let expressions = validate_template(template)?;
    let mut output = String::new();
    let mut cursor = 0;

    for expression in expressions {
        let open = template[cursor..]
            .find('{')
            .map(|offset| cursor + offset)
            .expect("validated expression must have an opening brace");

        output.push_str(&template[cursor..open]);

        let mut contributed = false;
        for (_, name) in expression.names {
            if let Some((_, value)) = vars.iter().rev().find(|(key, _)| *key == name) {
                if contributed {
                    output.push(',');
                }
                output.push_str(&encode_value(value));
                contributed = true;
            }
        }

        cursor = expression.end + 1;
    }

    output.push_str(&template[cursor..]);
    Ok(output)
}

fn main() {}