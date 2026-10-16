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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Operator {
    Simple,
    Reserved,
    Fragment,
    Dot,
    Slash,
    Semicolon,
    Query,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Modifier {
    None,
    Explode,
    Prefix(usize),
}

#[derive(Debug, Clone)]
struct Specifier {
    name: String,
    pos: usize,
    modifier: Modifier,
}

#[derive(Debug, Clone)]
struct Expression {
    op: Operator,
    specs: Vec<Specifier>,
}

#[derive(Debug, Clone)]
enum Part {
    Literal(String),
    Expression(Expression),
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

fn parse_expression(content: &str, base: usize, close: usize) -> Result<Expression, Error> {
    let bytes = content.as_bytes();
    let (op, offset) = if let Some(&first) = bytes.first() {
        let recognized = match first {
            b'+' => Some(Operator::Reserved),
            b'#' => Some(Operator::Fragment),
            b'.' => Some(Operator::Dot),
            b'/' => Some(Operator::Slash),
            b';' => Some(Operator::Semicolon),
            b'?' => Some(Operator::Query),
            b'&' => Some(Operator::Continue),
            _ => None,
        };
        if let Some(op) = recognized {
            (op, 1)
        } else {
            (Operator::Simple, 0)
        }
    } else {
        (Operator::Simple, 0)
    };

    if offset == bytes.len() {
        return Err(Error::EmptyExpression { pos: close });
    }

    let mut specs = Vec::new();
    let mut seen = Vec::<String>::new();
    let mut start = offset;

    loop {
        let end = content[start..]
            .find(',')
            .map(|relative| start + relative)
            .unwrap_or(bytes.len());

        if end == start {
            return Err(Error::EmptyVariable { pos: base + start });
        }

        let spec = &content[start..end];
        let spec_bytes = spec.as_bytes();
        let modifier_at = spec_bytes.iter().position(|&b| b == b'*' || b == b':');
        let name_end = modifier_at.unwrap_or(spec_bytes.len());

        if name_end == 0 {
            return Err(Error::InvalidName { pos: base + start });
        }

        for (i, &byte) in spec_bytes[..name_end].iter().enumerate() {
            let valid = if i == 0 {
                is_name_start(byte)
            } else {
                is_name_continue(byte)
            };
            if !valid {
                return Err(Error::InvalidName {
                    pos: base + start + i,
                });
            }
        }

        let modifier = if let Some(mod_index) = modifier_at {
            let suffix = &spec_bytes[mod_index..];
            if suffix == b"*" {
                Modifier::Explode
            } else if suffix.first() == Some(&b':') {
                let digits = &suffix[1..];
                if digits.is_empty()
                    || digits.len() > 4
                    || digits[0] == b'0'
                    || !digits.iter().all(u8::is_ascii_digit)
                {
                    return Err(Error::InvalidModifier {
                        pos: base + start + mod_index,
                    });
                }
                let mut number = 0usize;
                for &digit in digits {
                    number = number * 10 + usize::from(digit - b'0');
                }
                if !(1..=9999).contains(&number) {
                    return Err(Error::InvalidModifier {
                        pos: base + start + mod_index,
                    });
                }
                Modifier::Prefix(number)
            } else {
                return Err(Error::InvalidModifier {
                    pos: base + start + mod_index,
                });
            }
        } else {
            Modifier::None
        };

        let name = spec[..name_end].to_owned();
        if seen.iter().any(|existing| existing == &name) {
            return Err(Error::DuplicateVariable { pos: base + start });
        }
        seen.push(name.clone());
        specs.push(Specifier {
            name,
            pos: base + start,
            modifier,
        });

        if end == bytes.len() {
            break;
        }
        start = end + 1;
        if start == bytes.len() {
            return Err(Error::EmptyVariable { pos: base + start });
        }
    }

    Ok(Expression { op, specs })
}

fn parse_template(template: &str) -> Result<Vec<Part>, Error> {
    let bytes = template.as_bytes();
    let mut parts = Vec::new();
    let mut literal_start = 0;
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'}' => return Err(Error::UnexpectedClose { pos: index }),
            b'{' => {
                if literal_start < index {
                    parts.push(Part::Literal(template[literal_start..index].to_owned()));
                }
                let open = index;
                let close = match bytes[index + 1..].iter().position(|&byte| byte == b'}') {
                    Some(relative) => index + 1 + relative,
                    None => return Err(Error::Unclosed { pos: open }),
                };

                if let Some(relative) = bytes[index + 1..close]
                    .iter()
                    .position(|&byte| byte == b'{')
                {
                    return Err(Error::NestedOpen {
                        pos: index + 1 + relative,
                    });
                }

                let content_start = index + 1;
                let content = &template[content_start..close];
                let expression = parse_expression(content, content_start, close)?;
                parts.push(Part::Expression(expression));
                index = close + 1;
                literal_start = index;
            }
            _ => index += 1,
        }
    }

    if literal_start < bytes.len() {
        parts.push(Part::Literal(template[literal_start..].to_owned()));
    }
    Ok(parts)
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

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        _ => (b'A' + value - 10) as char,
    }
}

fn encode(value: &str, reserved: bool) -> String {
    let bytes = value.as_bytes();
    let mut result = String::new();
    let mut index = 0;

    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
        {
            result.push('%');
            result.push(hex_digit(high));
            result.push(hex_digit(low));
            index += 3;
            continue;
        }

        if is_unreserved(byte) || (reserved && is_reserved(byte)) {
            result.push(byte as char);
        } else {
            result.push('%');
            result.push(hex_digit(byte >> 4));
            result.push(hex_digit(byte & 0x0f));
        }
        index += 1;
    }

    result
}

fn lookup<'a>(name: &str, vars: &'a [(&str, Value)]) -> Option<&'a Value> {
    vars.iter()
        .rev()
        .find(|(binding, _)| *binding == name)
        .map(|(_, value)| value)
}

fn scalar_field(name: &str, value: String, op: Operator) -> String {
    match op {
        Operator::Semicolon => {
            if value.is_empty() {
                name.to_owned()
            } else {
                format!("{name}={value}")
            }
        }
        Operator::Query | Operator::Continue => format!("{name}={value}"),
        _ => value,
    }
}

fn expand_expression(expression: &Expression, vars: &[(&str, Value)]) -> Result<String, Error> {
    let reserved = matches!(expression.op, Operator::Reserved | Operator::Fragment);
    let explode_separator = match expression.op {
        Operator::Dot => ".",
        Operator::Slash => "/",
        Operator::Semicolon => ";",
        Operator::Query | Operator::Continue => "&",
        _ => ",",
    };

    let mut fields = Vec::<String>::new();

    for spec in &expression.specs {
        let Some(value) = lookup(&spec.name, vars) else {
            continue;
        };

        if matches!(spec.modifier, Modifier::Prefix(_))
            && !matches!(value, Value::Scalar(_))
        {
            return Err(Error::TypeMismatch { pos: spec.pos });
        }

        match value {
            Value::Scalar(text) => {
                let selected = match spec.modifier {
                    Modifier::Prefix(count) => text.chars().take(count).collect::<String>(),
                    _ => text.clone(),
                };
                fields.push(scalar_field(&spec.name, encode(&selected, reserved), expression.op));
            }
            Value::List(items) => {
                if items.is_empty() {
                    continue;
                }
                if spec.modifier == Modifier::Explode {
                    for item in items {
                        fields.push(scalar_field(&spec.name, encode(item, reserved), expression.op));
                    }
                } else {
                    let joined = items
                        .iter()
                        .map(|item| encode(item, reserved))
                        .collect::<Vec<_>>()
                        .join(",");
                    fields.push(scalar_field(&spec.name, joined, expression.op));
                }
            }
            Value::Map(entries) => {
                let mut keys = Vec::<&str>::new();
                for (key, _) in entries {
                    if keys.iter().any(|existing| *existing == key) {
                        return Err(Error::DuplicateKey { pos: spec.pos });
                    }
                    keys.push(key);
                }

                if entries.is_empty() {
                    continue;
                }

                if spec.modifier == Modifier::Explode {
                    for (key, value) in entries {
                        let encoded_key = encode(key, reserved);
                        let encoded_value = encode(value, reserved);
                        if expression.op == Operator::Semicolon && encoded_value.is_empty() {
                            fields.push(encoded_key);
                        } else {
                            fields.push(format!("{encoded_key}={encoded_value}"));
                        }
                    }
                } else {
                    let mut components = Vec::with_capacity(entries.len() * 2);
                    for (key, value) in entries {
                        components.push(encode(key, reserved));
                        components.push(encode(value, reserved));
                    }
                    fields.push(scalar_field(
                        &spec.name,
                        components.join(","),
                        expression.op,
                    ));
                }
            }
        }
    }

    if fields.is_empty() {
        return Ok(String::new());
    }

    let (prefix, separator) = match expression.op {
        Operator::Simple | Operator::Reserved => ("", ","),
        Operator::Fragment => ("#", ","),
        Operator::Dot => (".", "."),
        Operator::Slash => ("/", "/"),
        Operator::Semicolon => (";", ";"),
        Operator::Query => ("?", "&"),
        Operator::Continue => ("&", "&"),
    };

    let _ = explode_separator;
    Ok(format!("{prefix}{}", fields.join(separator)))
}

pub fn expand(template: &str, vars: &[(&str, Value)]) -> Result<String, Error> {
    let parts = parse_template(template)?;
    let mut output = String::new();

    for part in parts {
        match part {
            Part::Literal(text) => output.push_str(&text),
            Part::Expression(expression) => {
                output.push_str(&expand_expression(&expression, vars)?);
            }
        }
    }

    Ok(output)
}