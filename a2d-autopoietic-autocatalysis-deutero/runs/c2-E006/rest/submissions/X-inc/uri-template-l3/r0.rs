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
struct Variable {
    name: String,
    start: usize,
}

#[derive(Debug, Clone)]
struct Expression {
    start: usize,
    end: usize,
    operator: Option<char>,
    variables: Vec<Variable>,
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

fn parse_expression(
    template: &str,
    open: usize,
    close: usize,
) -> Result<Expression, Error> {
    let bytes = template.as_bytes();

    if let Some(offset) = bytes[open + 1..close]
        .iter()
        .position(|&byte| byte == b'{')
    {
        return Err(Error::NestedOpen {
            pos: open + 1 + offset,
        });
    }

    let content_start = open + 1;
    let mut spec_start = content_start;
    let mut operator = None;

    if content_start < close {
        let first = bytes[content_start] as char;
        if matches!(first, '+' | '#' | '.' | '/' | ';' | '?' | '&') {
            operator = Some(first);
            spec_start += 1;
        } else if matches!(first, '=' | '!' | '@' | '|' | '$') {
            return Err(Error::UnsupportedOperator { pos: content_start });
        }
    }

    if spec_start == close {
        return Err(Error::EmptyExpression { pos: close });
    }

    let mut variables = Vec::new();
    let mut expression_names = Vec::<String>::new();
    let mut item_start = spec_start;

    loop {
        let item_end = bytes[item_start..close]
            .iter()
            .position(|&byte| byte == b',')
            .map_or(close, |offset| item_start + offset);

        if item_start == item_end {
            return Err(Error::EmptyVariable { pos: item_start });
        }

        let name_bytes = &bytes[item_start..item_end];
        if !is_name_start(name_bytes[0]) {
            return Err(Error::InvalidName { pos: item_start });
        }

        let invalid_offset = name_bytes
            .iter()
            .enumerate()
            .skip(1)
            .find_map(|(index, &byte)| {
                (!is_name_continue(byte)).then_some(index)
            });

        if let Some(offset) = invalid_offset {
            return Err(Error::InvalidName {
                pos: item_start + offset,
            });
        }

        let name = template[item_start..item_end].to_string();
        if expression_names.iter().any(|used| used == &name) {
            return Err(Error::DuplicateVariable { pos: item_start });
        }
        expression_names.push(name.clone());
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
        end: close,
        operator,
        variables,
    })
}

fn validate(template: &str) -> Result<Vec<Expression>, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'}' => return Err(Error::UnexpectedClose { pos: index }),
            b'{' => {
                let open = index;
                let Some(offset) = bytes[index + 1..]
                    .iter()
                    .position(|&byte| byte == b'}')
                else {
                    return Err(Error::Unclosed { pos: open });
                };
                let close = index + 1 + offset;
                expressions.push(parse_expression(template, open, close)?);
                index = close + 1;
            }
            _ => index += 1,
        }
    }

    Ok(expressions)
}

fn is_hex(byte: u8) -> bool {
    byte.is_ascii_hexdigit()
}

fn hex_upper(byte: u8) -> u8 {
    byte.to_ascii_uppercase()
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

fn is_reserved(byte: u8) -> bool {
    matches!(
        byte,
        b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@' | b'!' | b'$' | b'&'
            | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
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
            output.push(hex_upper(bytes[index + 1]) as char);
            output.push(hex_upper(bytes[index + 2]) as char);
            index += 3;
            continue;
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

    output
}

fn binding_value<'a>(name: &str, vars: &'a [(&str, &str)]) -> Option<&'a str> {
    vars.iter()
        .rev()
        .find_map(|(candidate, value)| (*candidate == name).then_some(*value))
}

fn expand_expression(expression: &Expression, vars: &[(&str, &str)]) -> String {
    let operator = expression.operator;
    let allow_reserved = matches!(operator, Some('+') | Some('#'));
    let mut fields = Vec::<String>::new();

    for variable in &expression.variables {
        let Some(value) = binding_value(&variable.name, vars) else {
            continue;
        };

        let encoded = encode(value, allow_reserved);
        let field = match operator {
            Some(';' | '?' | '&') if encoded.is_empty() && operator == Some(';') => {
                variable.name.clone()
            }
            Some(';' | '?' | '&') => format!("{}={}", variable.name, encoded),
            _ => encoded,
        };
        fields.push(field);
    }

    if fields.is_empty() {
        return String::new();
    }

    let separator = match operator {
        Some('.') => ".",
        Some('/') => "/",
        Some(';') => ";",
        Some('?') | Some('&') => "&",
        _ => ",",
    };

    let prefix = match operator {
        Some('#') => "#",
        Some('.') => ".",
        Some('/') => "/",
        Some(';') => ";",
        Some('?') => "?",
        Some('&') => "&",
        _ => "",
    };

    format!("{}{}", prefix, fields.join(separator))
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let expressions = validate(template)?;
    let mut output = String::new();
    let mut cursor = 0;

    for expression in expressions {
        output.push_str(&template[cursor..expression.start]);
        output.push_str(&expand_expression(&expression, vars));
        cursor = expression.end + 1;
    }

    output.push_str(&template[cursor..]);
    Ok(output)
}