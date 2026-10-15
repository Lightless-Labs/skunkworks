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
    Question,
    Ampersand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Modifier {
    None,
    Explode,
    Prefix(usize),
}

#[derive(Debug)]
struct Specifier<'a> {
    name: &'a str,
    start: usize,
    modifier: Modifier,
}

#[derive(Debug)]
struct Expression<'a> {
    operator: Operator,
    specs: Vec<Specifier<'a>>,
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn parse_specifier<'a>(
    part: &'a str,
    start: usize,
) -> Result<Specifier<'a>, Error> {
    if part.is_empty() {
        return Err(Error::EmptyVariable { pos: start });
    }

    let bytes = part.as_bytes();
    let modifier_index = bytes
        .iter()
        .position(|&byte| byte == b'*' || byte == b':')
        .unwrap_or(bytes.len());

    let name = &part[..modifier_index];

    if name.is_empty() {
        return Err(Error::InvalidName { pos: start });
    }

    let name_bytes = name.as_bytes();
    if !is_name_start(name_bytes[0]) {
        return Err(Error::InvalidName { pos: start });
    }
    for (index, &byte) in name_bytes.iter().enumerate().skip(1) {
        if !is_name_continue(byte) {
            return Err(Error::InvalidName { pos: start + index });
        }
    }

    let modifier = if modifier_index == bytes.len() {
        Modifier::None
    } else {
        let suffix = &part[modifier_index..];
        match suffix.as_bytes().first().copied() {
            Some(b'*') if suffix == "*" => Modifier::Explode,
            Some(b':') => {
                let digits = &suffix[1..];
                let valid = (1..=4).contains(&digits.len())
                    && digits.as_bytes()[0].is_ascii_digit()
                    && digits.as_bytes()[0] != b'0'
                    && digits.as_bytes().iter().all(u8::is_ascii_digit);

                if !valid {
                    return Err(Error::InvalidModifier {
                        pos: start + modifier_index,
                    });
                }

                let count = digits.parse::<usize>().map_err(|_| {
                    Error::InvalidModifier {
                        pos: start + modifier_index,
                    }
                })?;
                if !(1..=9999).contains(&count) {
                    return Err(Error::InvalidModifier {
                        pos: start + modifier_index,
                    });
                }
                Modifier::Prefix(count)
            }
            _ => {
                return Err(Error::InvalidModifier {
                    pos: start + modifier_index,
                });
            }
        }
    };

    Ok(Specifier {
        name,
        start,
        modifier,
    })
}

fn parse_expression(content: &str, content_start: usize, close: usize) -> Result<Expression<'_>, Error> {
    let (operator, body, body_start) = if let Some(first) = content.as_bytes().first().copied() {
        let operator = match first {
            b'+' => Some(Operator::Reserved),
            b'#' => Some(Operator::Fragment),
            b'.' => Some(Operator::Dot),
            b'/' => Some(Operator::Slash),
            b';' => Some(Operator::Semicolon),
            b'?' => Some(Operator::Question),
            b'&' => Some(Operator::Ampersand),
            b'=' | b'!' | b'@' | b'|' => {
                return Err(Error::UnsupportedOperator { pos: content_start });
            }
            _ => None,
        };
        if let Some(operator) = operator {
            (operator, &content[1..], content_start + 1)
        } else {
            (Operator::Simple, content, content_start)
        }
    } else {
        (Operator::Simple, content, content_start)
    };

    if body.is_empty() {
        return Err(Error::EmptyExpression { pos: close });
    }

    let mut specs = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut offset = 0;

    for part in body.split(',') {
        let spec = parse_specifier(part, body_start + offset)?;
        if !seen.insert(spec.name) {
            return Err(Error::DuplicateVariable { pos: spec.start });
        }
        specs.push(spec);
        offset += part.len() + 1;
    }

    Ok(Expression { operator, specs })
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

fn hex_digit(value: u8) -> u8 {
    if value < 10 {
        b'0' + value
    } else {
        b'A' + value - 10
    }
}

fn encode(input: &str, reserved: bool) -> String {
    let bytes = input.as_bytes();
    let mut output = String::new();
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

        let unreserved = byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'.' | b'_' | b'~');
        if unreserved || (reserved && is_reserved(byte)) {
            output.push(byte as char);
        } else {
            output.push('%');
            output.push(hex_digit(byte >> 4) as char);
            output.push(hex_digit(byte & 0x0f) as char);
        }
        index += 1;
    }

    output
}

fn prefix_chars(input: &str, count: usize) -> &str {
    input
        .char_indices()
        .nth(count)
        .map_or(input, |(index, _)| &input[..index])
}

fn selected_value<'a>(name: &str, vars: &'a [(&str, Value)]) -> Option<&'a Value> {
    vars.iter()
        .rev()
        .find(|(binding, _)| *binding == name)
        .map(|(_, value)| value)
}

fn named_field(name: &str, value: String, semicolon: bool) -> String {
    if semicolon && value.is_empty() {
        name.to_owned()
    } else {
        format!("{name}={value}")
    }
}

fn expand_expression(
    expression: &Expression<'_>,
    vars: &[(&str, Value)],
) -> Result<String, Error> {
    let operator = expression.operator;
    let reserved = matches!(operator, Operator::Reserved | Operator::Fragment);
    let mut fields = Vec::new();

    for spec in &expression.specs {
        let Some(value) = selected_value(spec.name, vars) else {
            continue;
        };

        if matches!(spec.modifier, Modifier::Prefix(_))
            && !matches!(value, Value::Scalar(_))
        {
            return Err(Error::TypeMismatch { pos: spec.start });
        }

        match value {
            Value::Scalar(value) => {
                let value = match spec.modifier {
                    Modifier::Prefix(count) => prefix_chars(value, count),
                    _ => value,
                };
                let encoded = encode(value, reserved);
                if matches!(operator, Operator::Semicolon | Operator::Question | Operator::Ampersand) {
                    fields.push(named_field(
                        spec.name,
                        encoded,
                        operator == Operator::Semicolon,
                    ));
                } else {
                    fields.push(encoded);
                }
            }
            Value::List(items) => {
                if items.is_empty() {
                    continue;
                }
                if matches!(spec.modifier, Modifier::Explode) {
                    for item in items {
                        let encoded = encode(item, reserved);
                        if matches!(operator, Operator::Semicolon | Operator::Question | Operator::Ampersand) {
                            fields.push(named_field(
                                spec.name,
                                encoded,
                                operator == Operator::Semicolon,
                            ));
                        } else {
                            fields.push(encoded);
                        }
                    }
                } else {
                    let joined = items
                        .iter()
                        .map(|item| encode(item, reserved))
                        .collect::<Vec<_>>()
                        .join(",");
                    if matches!(operator, Operator::Semicolon | Operator::Question | Operator::Ampersand) {
                        fields.push(named_field(
                            spec.name,
                            joined,
                            operator == Operator::Semicolon,
                        ));
                    } else {
                        fields.push(joined);
                    }
                }
            }
            Value::Map(entries) => {
                let mut keys = std::collections::HashSet::new();
                for (key, _) in entries {
                    if !keys.insert(key.as_str()) {
                        return Err(Error::DuplicateKey { pos: spec.start });
                    }
                }

                if entries.is_empty() {
                    continue;
                }

                if matches!(spec.modifier, Modifier::Explode) {
                    for (key, value) in entries {
                        let encoded_key = encode(key, reserved);
                        let encoded_value = encode(value, reserved);
                        let field = if operator == Operator::Semicolon && encoded_value.is_empty() {
                            encoded_key
                        } else {
                            format!("{encoded_key}={encoded_value}")
                        };
                        fields.push(field);
                    }
                } else {
                    let flattened = entries
                        .iter()
                        .flat_map(|(key, value)| [encode(key, reserved), encode(value, reserved)])
                        .collect::<Vec<_>>()
                        .join(",");
                    if matches!(operator, Operator::Semicolon | Operator::Question | Operator::Ampersand) {
                        fields.push(named_field(
                            spec.name,
                            flattened,
                            operator == Operator::Semicolon,
                        ));
                    } else {
                        fields.push(flattened);
                    }
                }
            }
        }
    }

    if fields.is_empty() {
        return Ok(String::new());
    }

    let (separator, prefix) = match operator {
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

pub fn expand(template: &str, vars: &[(&str, Value)]) -> Result<String, Error> {
    let mut expressions = Vec::new();
    let mut cursor = 0;

    while cursor < template.len() {
        let Some(relative_open) = template[cursor..].find(['{', '}']) else {
            break;
        };
        let position = cursor + relative_open;
        let byte = template.as_bytes()[position];

        if byte == b'}' {
            return Err(Error::UnexpectedClose { pos: position });
        }

        let open = position;
        let after_open = open + 1;
        let Some(relative_close) = template[after_open..].find('}') else {
            return Err(Error::Unclosed { pos: open });
        };
        let close = after_open + relative_close;

        if let Some(relative_nested) = template[after_open..close].find('{') {
            return Err(Error::NestedOpen {
                pos: after_open + relative_nested,
            });
        }

        let content = &template[after_open..close];
        expressions.push((open, close, parse_expression(content, after_open, close)?));
        cursor = close + 1;
    }

    let mut output = String::new();
    cursor = 0;
    for (open, close, expression) in expressions {
        output.push_str(&template[cursor..open]);
        output.push_str(&expand_expression(&expression, vars)?);
        cursor = close + 1;
    }
    output.push_str(&template[cursor..]);

    Ok(output)
}