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

struct Expression<'a> {
    operator: Option<char>,
    names: Vec<&'a str>,
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let expressions = validate(template)?;
    let mut output = String::new();
    let mut cursor = 0;

    for (open, close, expression) in expressions {
        output.push_str(&template[cursor..open]);
        append_expression(&mut output, expression, vars);
        cursor = close + 1;
    }

    output.push_str(&template[cursor..]);
    Ok(output)
}

fn validate(template: &str) -> Result<Vec<(usize, usize, Expression<'_>)>, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut pos = 0;

    while pos < bytes.len() {
        match bytes[pos] {
            b'}' => return Err(Error::UnexpectedClose { pos }),
            b'{' => {
                let open = pos;
                let Some(relative_close) = bytes[pos + 1..].iter().position(|&b| b == b'}') else {
                    return Err(Error::Unclosed { pos: open });
                };
                let close = pos + 1 + relative_close;

                if let Some(relative_open) = bytes[pos + 1..close].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen {
                        pos: pos + 1 + relative_open,
                    });
                }

                let expression = parse_expression(template, open, close)?;
                expressions.push((open, close, expression));
                pos = close + 1;
            }
            _ => pos += 1,
        }
    }

    Ok(expressions)
}

fn parse_expression(
    template: &str,
    open: usize,
    close: usize,
) -> Result<Expression<'_>, Error> {
    let bytes = template.as_bytes();
    let mut start = open + 1;
    let operator = match bytes[start] {
        b'+' => {
            start += 1;
            Some('+')
        }
        b'#' => {
            start += 1;
            Some('#')
        }
        b'.' | b'/' | b';' | b'?' | b'&' => {
            return Err(Error::UnsupportedOperator { pos: start });
        }
        _ => None,
    };

    if start == close {
        return Err(Error::EmptyExpression { pos: close });
    }

    let mut names = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    let mut spec_start = start;

    loop {
        let spec_end = template[spec_start..close]
            .find(',')
            .map_or(close, |relative| spec_start + relative);

        if spec_start == spec_end {
            return Err(Error::EmptyVariable { pos: spec_start });
        }

        let name = &template[spec_start..spec_end];
        validate_name(name, spec_start)?;

        if seen.iter().any(|existing| *existing == name) {
            return Err(Error::DuplicateVariable { pos: spec_start });
        }

        seen.push(name);
        names.push(name);

        if spec_end == close {
            break;
        }
        spec_start = spec_end + 1;
    }

    Ok(Expression { operator, names })
}

fn validate_name(name: &str, start: usize) -> Result<(), Error> {
    for (index, &byte) in name.as_bytes().iter().enumerate() {
        let valid = if index == 0 {
            byte.is_ascii_alphabetic() || byte == b'_'
        } else {
            byte.is_ascii_alphanumeric() || byte == b'_'
        };

        if !valid {
            return Err(Error::InvalidName { pos: start + index });
        }
    }

    Ok(())
}

fn append_expression(output: &mut String, expression: Expression<'_>, vars: &[(&str, &str)]) {
    let mut fields = Vec::new();

    for name in expression.names {
        if let Some(value) = vars
            .iter()
            .rev()
            .find(|(binding_name, _)| *binding_name == name)
            .map(|(_, value)| *value)
        {
            fields.push(value);
        }
    }

    if fields.is_empty() {
        return;
    }

    if expression.operator == Some('#') {
        output.push('#');
    }

    let reserved = matches!(expression.operator, Some('+') | Some('#'));

    for (index, value) in fields.iter().enumerate() {
        if index > 0 {
            output.push(',');
        }
        encode_value(output, value, reserved);
    }
}

fn encode_value(output: &mut String, value: &str, reserved: bool) {
    let bytes = value.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        let byte = bytes[index];

        if byte == b'%'
            && index + 2 < bytes.len()
            && is_hex(bytes[index + 1])
            && is_hex(bytes[index + 2])
        {
            output.push('%');
            output.push((bytes[index + 1] as char).to_ascii_uppercase());
            output.push((bytes[index + 2] as char).to_ascii_uppercase());
            index += 3;
            continue;
        }

        if is_unreserved(byte) || (reserved && is_reserved(byte)) {
            output.push(byte as char);
        } else {
            output.push('%');
            output.push(hex_digit(byte >> 4));
            output.push(hex_digit(byte & 0x0f));
        }

        index += 1;
    }
}

fn is_hex(byte: u8) -> bool {
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

fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        _ => (b'A' + value - 10) as char,
    }
}