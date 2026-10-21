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
struct Variable<'a> {
    name: &'a str,
    start: usize,
}

#[derive(Debug)]
struct Expression<'a> {
    start: usize,
    end: usize,
    operator: Option<u8>,
    variables: Vec<Variable<'a>>,
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

fn parse_expression<'a>(
    template: &'a str,
    open: usize,
    close: usize,
) -> Result<Expression<'a>, Error> {
    let bytes = template.as_bytes();
    let content_start = open + 1;

    if let Some(relative) = bytes[content_start..close].iter().position(|&b| b == b'{') {
        return Err(Error::NestedOpen {
            pos: content_start + relative,
        });
    }

    let mut position = content_start;
    let operator = match bytes.get(position).copied() {
        Some(b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&') => {
            let op = bytes[position];
            position += 1;
            if op != b'+' && op != b'#' {
                return Err(Error::UnsupportedOperator { pos: position - 1 });
            }
            Some(op)
        }
        _ => None,
    };

    if position == close {
        return Err(Error::EmptyExpression { pos: close });
    }

    let mut variables = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    let mut spec_start = position;

    loop {
        let spec_end = bytes[spec_start..close]
            .iter()
            .position(|&b| b == b',')
            .map(|relative| spec_start + relative)
            .unwrap_or(close);

        if spec_start == spec_end {
            return Err(Error::EmptyVariable { pos: spec_start });
        }

        let name_bytes = &bytes[spec_start..spec_end];
        if !is_name_start(name_bytes[0]) {
            return Err(Error::InvalidName { pos: spec_start });
        }
        for (offset, &byte) in name_bytes.iter().enumerate().skip(1) {
            if !is_name_continue(byte) {
                return Err(Error::InvalidName {
                    pos: spec_start + offset,
                });
            }
        }

        let name = &template[spec_start..spec_end];
        if seen.iter().any(|&existing| existing == name) {
            return Err(Error::DuplicateVariable { pos: spec_start });
        }
        seen.push(name);
        variables.push(Variable {
            name,
            start: spec_start,
        });

        if spec_end == close {
            break;
        }
        spec_start = spec_end + 1;
    }

    Ok(Expression {
        start: open,
        end: close + 1,
        operator,
        variables,
    })
}

fn parse_template(template: &str) -> Result<Vec<Expression<'_>>, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut position = 0;

    while position < bytes.len() {
        match bytes[position] {
            b'}' => return Err(Error::UnexpectedClose { pos: position }),
            b'{' => {
                let close = match bytes[position + 1..]
                    .iter()
                    .position(|&byte| byte == b'}')
                {
                    Some(relative) => position + 1 + relative,
                    None => return Err(Error::Unclosed { pos: position }),
                };
                let expression = parse_expression(template, position, close)?;
                position = expression.end;
                expressions.push(expression);
            }
            _ => position += 1,
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
        b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@' | b'!' | b'$' | b'&' | b'\'' | b'('
            | b')' | b'*' | b'+' | b',' | b';' | b'='
    )
}

fn is_hex(byte: u8) -> bool {
    byte.is_ascii_hexdigit()
}

fn hex_digit(value: u8) -> u8 {
    if value < 10 {
        b'0' + value
    } else {
        b'A' + (value - 10)
    }
}

fn push_percent_encoded(output: &mut String, byte: u8) {
    output.push('%');
    output.push(hex_digit(byte >> 4) as char);
    output.push(hex_digit(byte & 0x0f) as char);
}

fn encode_value(value: &str, allow_reserved: bool) -> String {
    let bytes = value.as_bytes();
    let mut output = String::new();
    let mut position = 0;

    while position < bytes.len() {
        let byte = bytes[position];

        if byte == b'%'
            && position + 2 < bytes.len()
            && is_hex(bytes[position + 1])
            && is_hex(bytes[position + 2])
        {
            output.push('%');
            output.push((bytes[position + 1] as char).to_ascii_uppercase());
            output.push((bytes[position + 2] as char).to_ascii_uppercase());
            position += 3;
        } else if is_unreserved(byte) || (allow_reserved && is_reserved(byte)) {
            output.push(byte as char);
            position += 1;
        } else {
            push_percent_encoded(&mut output, byte);
            position += 1;
        }
    }

    output
}

fn lookup<'a>(vars: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    vars.iter()
        .rev()
        .find_map(|&(binding_name, value)| (binding_name == name).then_some(value))
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let expressions = parse_template(template)?;
    let mut output = String::new();
    let mut position = 0;

    for expression in expressions {
        output.push_str(&template[position..expression.start]);

        let mut fields = Vec::new();
        for variable in &expression.variables {
            let _ = variable.start;
            if let Some(value) = lookup(vars, variable.name) {
                fields.push(encode_value(value, expression.operator.is_some()));
            }
        }

        if !fields.is_empty() {
            if expression.operator == Some(b'#') {
                output.push('#');
            }
            output.push_str(&fields.join(","));
        }
        position = expression.end;
    }

    output.push_str(&template[position..]);
    Ok(output)
}