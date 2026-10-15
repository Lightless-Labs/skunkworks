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
    Dot,
    Slash,
    Semicolon,
    Query,
    Continuation,
}

#[derive(Debug, Clone)]
struct Variable {
    name: String,
    start: usize,
}

#[derive(Debug, Clone)]
struct Expression {
    start: usize,
    end: usize,
    operator: Operator,
    variables: Vec<Variable>,
}

fn is_supported_operator(byte: u8) -> bool {
    matches!(byte, b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&')
}

fn operator_for(byte: u8) -> Operator {
    match byte {
        b'+' => Operator::Reserved,
        b'#' => Operator::Fragment,
        b'.' => Operator::Dot,
        b'/' => Operator::Slash,
        b';' => Operator::Semicolon,
        b'?' => Operator::Query,
        b'&' => Operator::Continuation,
        _ => Operator::Simple,
    }
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

fn parse_expression(template: &str, open: usize, close: usize) -> Result<Expression, Error> {
    let bytes = template.as_bytes();

    if let Some(offset) = bytes[open + 1..close].iter().position(|&b| b == b'{') {
        return Err(Error::NestedOpen {
            pos: open + 1 + offset,
        });
    }

    let mut content_start = open + 1;
    let operator = if content_start < close && is_supported_operator(bytes[content_start]) {
        let op_byte = bytes[content_start];
        content_start += 1;
        operator_for(op_byte)
    } else {
        Operator::Simple
    };

    if content_start == close {
        return Err(Error::EmptyExpression { pos: close });
    }

    let mut variables = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut item_start = content_start;

    loop {
        let item_end = bytes[item_start..close]
            .iter()
            .position(|&b| b == b',')
            .map(|offset| item_start + offset)
            .unwrap_or(close);

        if item_start == item_end {
            return Err(Error::EmptyVariable { pos: item_start });
        }

        if !is_name_start(bytes[item_start]) {
            return Err(Error::InvalidName { pos: item_start });
        }

        let mut cursor = item_start + 1;
        while cursor < item_end && is_name_continue(bytes[cursor]) {
            cursor += 1;
        }
        if cursor < item_end {
            return Err(Error::InvalidName { pos: cursor });
        }

        let name = template[item_start..item_end].to_owned();
        if !seen.insert(name.clone()) {
            return Err(Error::DuplicateVariable { pos: item_start });
        }
        variables.push(Variable {
            name,
            start: item_start,
        });

        if item_end == close {
            break;
        }
        item_start = item_end + 1;
    }

    Ok(Expression {
        start: open,
        end: close + 1,
        operator,
        variables,
    })
}

fn validate(template: &str) -> Result<Vec<Expression>, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut cursor = 0;

    while cursor < bytes.len() {
        match bytes[cursor] {
            b'}' => return Err(Error::UnexpectedClose { pos: cursor }),
            b'{' => {
                let open = cursor;
                let Some(offset) = bytes[open + 1..].iter().position(|&b| b == b'}') else {
                    return Err(Error::Unclosed { pos: open });
                };
                let close = open + 1 + offset;
                expressions.push(parse_expression(template, open, close)?);
                cursor = close + 1;
            }
            _ => cursor += 1,
        }
    }

    Ok(expressions)
}

fn lookup<'a>(name: &str, vars: &'a [(&str, &str)]) -> Option<&'a str> {
    vars.iter()
        .rev()
        .find_map(|(binding_name, value)| (*binding_name == name).then_some(*value))
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

fn encode(value: &str, allow_reserved: bool) -> String {
    let bytes = value.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut cursor = 0;

    while cursor < bytes.len() {
        let byte = bytes[cursor];

        if byte == b'%' && cursor + 2 < bytes.len() {
            if let (Some(high), Some(low)) =
                (hex_value(bytes[cursor + 1]), hex_value(bytes[cursor + 2]))
            {
                result.push(b'%');
                result.push(hex_digit(high));
                result.push(hex_digit(low));
                cursor += 3;
                continue;
            }
        }

        if is_unreserved(byte) || (allow_reserved && is_reserved(byte)) {
            result.push(byte);
        } else {
            result.push(b'%');
            result.push(hex_digit(byte >> 4));
            result.push(hex_digit(byte & 0x0f));
        }
        cursor += 1;
    }

    String::from_utf8(result).expect("percent encoding produces valid UTF-8")
}

fn expand_expression(expression: &Expression, vars: &[(&str, &str)], output: &mut String) {
    let allow_reserved = matches!(expression.operator, Operator::Reserved | Operator::Fragment);
    let mut fields: Vec<(String, String)> = Vec::new();

    for variable in &expression.variables {
        let _ = variable.start;
        if let Some(value) = lookup(&variable.name, vars) {
            fields.push((
                variable.name.clone(),
                encode(value, allow_reserved),
            ));
        }
    }

    if fields.is_empty() {
        return;
    }

    match expression.operator {
        Operator::Simple | Operator::Reserved => {
            for (index, (_, value)) in fields.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                output.push_str(value);
            }
        }
        Operator::Fragment => {
            output.push('#');
            for (index, (_, value)) in fields.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                output.push_str(value);
            }
        }
        Operator::Dot | Operator::Slash => {
            let separator = if expression.operator == Operator::Dot {
                '.'
            } else {
                '/'
            };
            output.push(separator);
            for (index, (_, value)) in fields.iter().enumerate() {
                if index > 0 {
                    output.push(separator);
                }
                output.push_str(value);
            }
        }
        Operator::Semicolon => {
            output.push(';');
            for (index, (name, value)) in fields.iter().enumerate() {
                if index > 0 {
                    output.push(';');
                }
                output.push_str(name);
                if !value.is_empty() {
                    output.push('=');
                    output.push_str(value);
                }
            }
        }
        Operator::Query | Operator::Continuation => {
            output.push(if expression.operator == Operator::Query {
                '?'
            } else {
                '&'
            });
            for (index, (name, value)) in fields.iter().enumerate() {
                if index > 0 {
                    output.push('&');
                }
                output.push_str(name);
                output.push('=');
                output.push_str(value);
            }
        }
    }
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let expressions = validate(template)?;
    let mut output = String::with_capacity(template.len());
    let mut literal_start = 0;

    for expression in &expressions {
        output.push_str(&template[literal_start..expression.start]);
        expand_expression(expression, vars, &mut output);
        literal_start = expression.end;
    }

    output.push_str(&template[literal_start..]);
    Ok(output)
}

fn main() {}