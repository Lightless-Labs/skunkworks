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
    pos: usize,
}

#[derive(Debug)]
struct Expression<'a> {
    operator: Option<u8>,
    variables: Vec<Variable<'a>>,
}

#[derive(Debug)]
enum Part<'a> {
    Literal(&'a str),
    Expression(Expression<'a>),
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

fn parse_expression<'a>(
    template: &'a str,
    content_start: usize,
    close: usize,
) -> Result<Expression<'a>, Error> {
    let bytes = template.as_bytes();
    let mut start = content_start;
    let mut operator = None;

    if start < close && b"+#./;?&".contains(&bytes[start]) {
        let op = bytes[start];
        if op != b'+' && op != b'#' {
            return Err(Error::UnsupportedOperator { pos: start });
        }
        operator = Some(op);
        start += 1;
    }

    if start == close {
        return Err(Error::EmptyExpression { pos: close });
    }

    let mut variables = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    let mut spec_start = start;

    loop {
        let spec_end = bytes[spec_start..close]
            .iter()
            .position(|&byte| byte == b',')
            .map_or(close, |offset| spec_start + offset);

        if spec_start == spec_end {
            return Err(Error::EmptyVariable { pos: spec_start });
        }

        let spec = &template[spec_start..spec_end];
        let spec_bytes = spec.as_bytes();
        if !is_name_start(spec_bytes[0]) {
            return Err(Error::InvalidName { pos: spec_start });
        }
        if let Some(offset) = spec_bytes.iter().position(|&byte| !is_name_continue(byte)) {
            return Err(Error::InvalidName {
                pos: spec_start + offset,
            });
        }

        if seen.iter().any(|&name| name == spec) {
            return Err(Error::DuplicateVariable { pos: spec_start });
        }
        seen.push(spec);
        variables.push(Variable {
            name: spec,
            pos: spec_start,
        });

        if spec_end == close {
            break;
        }
        spec_start = spec_end + 1;
    }

    Ok(Expression {
        operator,
        variables,
    })
}

fn parse_template(template: &str) -> Result<Vec<Part<'_>>, Error> {
    let bytes = template.as_bytes();
    let mut parts = Vec::new();
    let mut cursor = 0;
    let mut literal_start = 0;

    while cursor < bytes.len() {
        match bytes[cursor] {
            b'}' => return Err(Error::UnexpectedClose { pos: cursor }),
            b'{' => {
                if literal_start < cursor {
                    parts.push(Part::Literal(&template[literal_start..cursor]));
                }

                let open = cursor;
                let Some(relative_close) = bytes[cursor + 1..].iter().position(|&b| b == b'}') else {
                    return Err(Error::Unclosed { pos: open });
                };
                let close = cursor + 1 + relative_close;

                if let Some(offset) = bytes[open + 1..close].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen {
                        pos: open + 1 + offset,
                    });
                }

                parts.push(Part::Expression(parse_expression(
                    template,
                    open + 1,
                    close,
                )?));
                cursor = close + 1;
                literal_start = cursor;
            }
            _ => cursor += 1,
        }
    }

    if literal_start < bytes.len() {
        parts.push(Part::Literal(&template[literal_start..]));
    }

    Ok(parts)
}

fn lookup<'a>(name: &str, vars: &'a [(&str, &str)]) -> Option<&'a str> {
    vars.iter()
        .rev()
        .find_map(|&(binding, value)| (binding == name).then_some(value))
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

fn encode_value(value: &str, allow_reserved: bool, output: &mut String) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let bytes = value.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
            {
                output.push('%');
                output.push(HEX[high as usize] as char);
                output.push(HEX[low as usize] as char);
                index += 3;
                continue;
            }
        }

        if is_unreserved(byte) || (allow_reserved && is_reserved(byte)) {
            output.push(byte as char);
        } else {
            output.push('%');
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0F) as usize] as char);
        }
        index += 1;
    }
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let parts = parse_template(template)?;
    let mut output = String::new();

    for part in parts {
        match part {
            Part::Literal(literal) => output.push_str(literal),
            Part::Expression(expression) => {
                let allow_reserved = matches!(expression.operator, Some(b'+' | b'#'));
                let mut fields = Vec::new();

                for variable in expression.variables {
                    if let Some(value) = lookup(variable.name, vars) {
                        let mut field = String::new();
                        encode_value(value, allow_reserved, &mut field);
                        fields.push(field);
                    }
                }

                if !fields.is_empty() {
                    if expression.operator == Some(b'#') {
                        output.push('#');
                    }
                    for (index, field) in fields.iter().enumerate() {
                        if index > 0 {
                            output.push(',');
                        }
                        output.push_str(field);
                    }
                }
            }
        }
    }

    Ok(output)
}

fn main() {}