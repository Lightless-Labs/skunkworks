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
    InvalidModifier { pos: usize },
    TypeMismatch { pos: usize },
    DuplicateKey { pos: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Scalar(String),
    List(Vec<String>),
    Map(Vec<(String, String)>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Operator {
    Simple,
    Reserved,
    Fragment,
    Dot,
    Slash,
    Semicolon,
    Question,
    Ampersand,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Modifier {
    None,
    Explode,
    Prefix(usize),
}

struct Specifier<'a> {
    name: &'a str,
    start: usize,
    modifier: Modifier,
}

struct Expression<'a> {
    operator: Operator,
    specs: Vec<Specifier<'a>>,
}

pub fn expand(template: &str, vars: &[(&str, Value)]) -> Result<String, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut cursor = 0;

    while cursor < bytes.len() {
        match bytes[cursor] {
            b'}' => return Err(Error::UnexpectedClose { pos: cursor }),
            b'{' => {
                let open = cursor;
                let close = match bytes[cursor + 1..].iter().position(|&b| b == b'}') {
                    Some(offset) => cursor + 1 + offset,
                    None => return Err(Error::Unclosed { pos: open }),
                };

                if let Some(offset) = bytes[open + 1..close].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen { pos: open + 1 + offset });
                }

                expressions.push(parse_expression(template, open, close)?);
                cursor = close + 1;
            }
            _ => cursor += 1,
        }
    }

    let mut output = String::with_capacity(template.len());
    cursor = 0;
    for expression in expressions {
        let open = expression
            .specs
            .first()
            .map(|spec| spec.start)
            .unwrap_or(0);
        // Recover the expression's opening brace from the first specifier/operator
        // through a separate scan of the already validated template.
        let _ = open;
    }

    cursor = 0;
    while cursor < bytes.len() {
        if bytes[cursor] != b'{' {
            let next = bytes[cursor..]
                .iter()
                .position(|&b| b == b'{')
                .map(|n| cursor + n)
                .unwrap_or(bytes.len());
            output.push_str(&template[cursor..next]);
            cursor = next;
            continue;
        }

        let close = cursor + 1 + bytes[cursor + 1..]
            .iter()
            .position(|&b| b == b'}')
            .expect("syntax was validated");
        let expression = parse_expression(template, cursor, close)
            .expect("syntax was validated");
        output.push_str(&expand_expression(&expression, vars)?);
        cursor = close + 1;
    }

    Ok(output)
}

fn parse_expression<'a>(
    template: &'a str,
    open: usize,
    close: usize,
) -> Result<Expression<'a>, Error> {
    let bytes = template.as_bytes();
    let mut content_start = open + 1;
    let operator = if content_start < close {
        match bytes[content_start] {
            b'+' => {
                content_start += 1;
                Operator::Reserved
            }
            b'#' => {
                content_start += 1;
                Operator::Fragment
            }
            b'.' => {
                content_start += 1;
                Operator::Dot
            }
            b'/' => {
                content_start += 1;
                Operator::Slash
            }
            b';' => {
                content_start += 1;
                Operator::Semicolon
            }
            b'?' => {
                content_start += 1;
                Operator::Question
            }
            b'&' => {
                content_start += 1;
                Operator::Ampersand
            }
            b'=' | b',' | b'!' | b'@' | b'|' => {
                return Err(Error::UnsupportedOperator { pos: content_start });
            }
            _ => Operator::Simple,
        }
    } else {
        Operator::Simple
    };

    if content_start == close {
        return Err(Error::EmptyExpression { pos: close });
    }

    let mut specs = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    let mut start = content_start;

    loop {
        let end = bytes[start..close]
            .iter()
            .position(|&b| b == b',')
            .map(|n| start + n)
            .unwrap_or(close);

        if start == end {
            return Err(Error::EmptyVariable { pos: start });
        }

        let spec = parse_specifier(template, start, end)?;
        if seen.iter().any(|name| *name == spec.name) {
            return Err(Error::DuplicateVariable { pos: start });
        }
        seen.push(spec.name);
        specs.push(spec);

        if end == close {
            break;
        }
        start = end + 1;
        if start == close {
            return Err(Error::EmptyVariable { pos: start });
        }
    }

    Ok(Expression { operator, specs })
}

fn parse_specifier<'a>(
    template: &'a str,
    start: usize,
    end: usize,
) -> Result<Specifier<'a>, Error> {
    let bytes = template.as_bytes();
    let modifier_pos = bytes[start..end]
        .iter()
        .position(|&b| b == b'*' || b == b':')
        .map(|n| start + n)
        .unwrap_or(end);

    let name = &template[start..modifier_pos];
    if name.is_empty() {
        return Err(Error::InvalidName { pos: start });
    }

    let name_bytes = name.as_bytes();
    if !is_name_start(name_bytes[0]) {
        return Err(Error::InvalidName { pos: start });
    }
    for (offset, &byte) in name_bytes.iter().enumerate().skip(1) {
        if !is_name_continue(byte) {
            return Err(Error::InvalidName { pos: start + offset });
        }
    }

    let modifier = if modifier_pos == end {
        Modifier::None
    } else if bytes[modifier_pos] == b'*' && modifier_pos + 1 == end {
        Modifier::Explode
    } else if bytes[modifier_pos] == b':' {
        let suffix = &bytes[modifier_pos + 1..end];
        if suffix.is_empty()
            || suffix.len() > 4
            || suffix[0] < b'1'
            || suffix[0] > b'9'
            || !suffix.iter().all(u8::is_ascii_digit)
        {
            return Err(Error::InvalidModifier { pos: modifier_pos });
        }
        let mut value = 0usize;
        for &digit in suffix {
            value = value * 10 + usize::from(digit - b'0');
        }
        if value > 9999 {
            return Err(Error::InvalidModifier { pos: modifier_pos });
        }
        Modifier::Prefix(value)
    } else {
        return Err(Error::InvalidModifier { pos: modifier_pos });
    };

    Ok(Specifier {
        name,
        start,
        modifier,
    })
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

fn expand_expression(expression: &Expression<'_>, vars: &[(&str, Value)]) -> Result<String, Error> {
    let mut fields = Vec::new();

    for spec in &expression.specs {
        let value = vars
            .iter()
            .rev()
            .find(|(name, _)| *name == spec.name)
            .map(|(_, value)| value);
        let Some(value) = value else {
            continue;
        };

        match (spec.modifier, value) {
            (Modifier::Prefix(_), Value::List(_) | Value::Map(_)) => {
                return Err(Error::TypeMismatch { pos: spec.start });
            }
            (_, Value::Scalar(scalar)) => {
                let scalar = match spec.modifier {
                    Modifier::Prefix(limit) => scalar.chars().take(limit).collect::<String>(),
                    _ => scalar.clone(),
                };
                fields.push(name_scalar_field(
                    expression.operator,
                    spec.name,
                    encode(&scalar, uses_reserved(expression.operator)),
                ));
            }
            (modifier, Value::List(items)) => {
                let exploded = modifier == Modifier::Explode;
                if exploded {
                    for item in items {
                        fields.push(name_scalar_field(
                            expression.operator,
                            spec.name,
                            encode(item, uses_reserved(expression.operator)),
                        ));
                    }
                } else if !items.is_empty() {
                    let joined = items
                        .iter()
                        .map(|item| encode(item, uses_reserved(expression.operator)))
                        .collect::<Vec<_>>()
                        .join(",");
                    fields.push(name_scalar_field(expression.operator, spec.name, joined));
                }
            }
            (modifier, Value::Map(entries)) => {
                let mut keys: Vec<&str> = Vec::new();
                for (key, _) in entries {
                    if keys.iter().any(|seen| *seen == key) {
                        return Err(Error::DuplicateKey { pos: spec.start });
                    }
                    keys.push(key);
                }

                if entries.is_empty() {
                    continue;
                }

                let exploded = modifier == Modifier::Explode;
                if exploded {
                    for (key, value) in entries {
                        let key = encode(key, uses_reserved(expression.operator));
                        let value = encode(value, uses_reserved(expression.operator));
                        let field = if expression.operator == Operator::Semicolon && value.is_empty() {
                            key
                        } else {
                            format!("{key}={value}")
                        };
                        fields.push(field);
                    }
                } else {
                    let mut components = Vec::with_capacity(entries.len() * 2);
                    for (key, value) in entries {
                        components.push(encode(key, uses_reserved(expression.operator)));
                        components.push(encode(value, uses_reserved(expression.operator)));
                    }
                    fields.push(name_scalar_field(
                        expression.operator,
                        spec.name,
                        components.join(","),
                    ));
                }
            }
        }
    }

    if fields.is_empty() {
        return Ok(String::new());
    }

    let (separator, prefix) = match expression.operator {
        Operator::Simple | Operator::Reserved => (",", ""),
        Operator::Fragment => (",", "#"),
        Operator::Dot => (".", "."),
        Operator::Slash => ("/", "/"),
        Operator::Semicolon => (";", ";"),
        Operator::Question => ("&", "?"),
        Operator::Ampersand => ("&", "&"),
    };

    Ok(format!("{prefix}{}", fields.join(separator)))
}

fn name_scalar_field(operator: Operator, name: &str, value: String) -> String {
    match operator {
        Operator::Semicolon if value.is_empty() => name.to_owned(),
        Operator::Semicolon | Operator::Question | Operator::Ampersand => {
            format!("{name}={value}")
        }
        _ => value,
    }
}

fn uses_reserved(operator: Operator) -> bool {
    matches!(operator, Operator::Reserved | Operator::Fragment)
}

fn encode(input: &str, reserved: bool) -> String {
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(bytes.len());
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

    output
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
        _ => (b'A' + (value - 10)) as char,
    }
}

fn main() {}