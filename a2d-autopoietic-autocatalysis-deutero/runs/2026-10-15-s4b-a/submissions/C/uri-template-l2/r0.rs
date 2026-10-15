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
    operator: Option<u8>,
    variables: Vec<Variable<'a>>,
}

enum Segment<'a> {
    Literal(&'a str),
    Expression(Expression<'a>),
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn parse_expression<'a>(
    template: &'a str,
    content_start: usize,
    close: usize,
) -> Result<Expression<'a>, Error> {
    let bytes = template.as_bytes();

    if let Some(relative) = bytes[content_start..close]
        .iter()
        .position(|&byte| byte == b'{')
    {
        return Err(Error::NestedOpen {
            pos: content_start + relative,
        });
    }

    let mut start = content_start;
    let operator = if start < close && b"+#./;?&".contains(&bytes[start]) {
        let op = bytes[start];
        start += 1;
        if op != b'+' && op != b'#' {
            return Err(Error::UnsupportedOperator { pos: start - 1 });
        }
        Some(op)
    } else {
        None
    };

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
            .map(|relative| spec_start + relative)
            .unwrap_or(close);

        if spec_start == spec_end {
            return Err(Error::EmptyVariable { pos: spec_start });
        }

        let spec = &template[spec_start..spec_end];
        let spec_bytes = spec.as_bytes();

        if !is_name_start(spec_bytes[0]) {
            return Err(Error::InvalidName { pos: spec_start });
        }

        for (index, &byte) in spec_bytes.iter().enumerate().skip(1) {
            if !is_name_continue(byte) {
                return Err(Error::InvalidName {
                    pos: spec_start + index,
                });
            }
        }

        if seen.iter().any(|&name| name == spec) {
            return Err(Error::DuplicateVariable { pos: spec_start });
        }

        seen.push(spec);
        variables.push(Variable {
            name: spec,
            start: spec_start,
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

fn parse_template(template: &str) -> Result<Vec<Segment<'_>>, Error> {
    let bytes = template.as_bytes();
    let mut segments = Vec::new();
    let mut literal_start = 0;
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'}' => return Err(Error::UnexpectedClose { pos: index }),
            b'{' => {
                if literal_start < index {
                    segments.push(Segment::Literal(&template[literal_start..index]));
                }

                let open = index;
                let Some(relative_close) = bytes[index + 1..]
                    .iter()
                    .position(|&byte| byte == b'}')
                else {
                    return Err(Error::Unclosed { pos: open });
                };
                let close = index + 1 + relative_close;

                segments.push(Segment::Expression(parse_expression(
                    template,
                    open + 1,
                    close,
                )?));

                index = close + 1;
                literal_start = index;
            }
            _ => index += 1,
        }
    }

    if literal_start < template.len() {
        segments.push(Segment::Literal(&template[literal_start..]));
    }

    Ok(segments)
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
    if value < 10 {
        b'0' + value
    } else {
        b'A' + (value - 10)
    }
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"-._~".contains(&byte)
}

fn is_reserved(byte: u8) -> bool {
    b":/?#[]@!$&'()*+,;=".contains(&byte)
}

fn encode_value(value: &str, allow_reserved: bool, output: &mut String) {
    let bytes = value.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        let byte = bytes[index];

        if byte == b'%' && index + 2 < bytes.len() {
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

        if is_unreserved(byte) || (allow_reserved && is_reserved(byte)) {
            output.push(byte as char);
        } else {
            output.push('%');
            output.push(hex_digit(byte >> 4) as char);
            output.push(hex_digit(byte & 0x0f) as char);
        }

        index += 1;
    }
}

fn lookup<'a>(name: &str, vars: &'a [(&str, &str)]) -> Option<&'a str> {
    vars.iter()
        .rev()
        .find(|(binding_name, _)| *binding_name == name)
        .map(|(_, value)| *value)
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let segments = parse_template(template)?;
    let mut output = String::new();

    for segment in segments {
        match segment {
            Segment::Literal(text) => output.push_str(text),
            Segment::Expression(expression) => {
                let mut fields = Vec::new();
                for variable in expression.variables {
                    let _ = variable.start;
                    if let Some(value) = lookup(variable.name, vars) {
                        fields.push(value);
                    }
                }

                if fields.is_empty() {
                    continue;
                }

                if expression.operator == Some(b'#') {
                    output.push('#');
                }

                let allow_reserved = matches!(expression.operator, Some(b'+') | Some(b'#'));
                for (index, value) in fields.into_iter().enumerate() {
                    if index > 0 {
                        output.push(',');
                    }
                    encode_value(value, allow_reserved, &mut output);
                }
            }
        }
    }

    Ok(output)
}

fn main() {}