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
    modifier: Modifier,
    pos: usize,
}

struct Expression<'a> {
    operator: Operator,
    specs: Vec<Specifier<'a>>,
    open_pos: usize,
    close_pos: usize,
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

fn parse_expression(
    content: &str,
    content_start: usize,
    open_pos: usize,
    close_pos: usize,
) -> Result<Expression<'_>, Error> {
    let bytes = content.as_bytes();
    let (operator, offset) = match bytes.first().copied() {
        Some(b'+') => (Operator::Reserved, 1),
        Some(b'#') => (Operator::Fragment, 1),
        Some(b'.') => (Operator::Dot, 1),
        Some(b'/') => (Operator::Slash, 1),
        Some(b';') => (Operator::Semicolon, 1),
        Some(b'?') => (Operator::Question, 1),
        Some(b'&') => (Operator::Ampersand, 1),
        Some(b'=' | b',' | b'!' | b'@' | b'|' | b'$') => {
            return Err(Error::UnsupportedOperator { pos: content_start });
        }
        _ => (Operator::Simple, 0),
    };

    if offset == bytes.len() {
        return Err(Error::EmptyExpression { pos: close_pos });
    }

    let mut specs = Vec::new();
    let mut seen = Vec::<&str>::new();
    let mut part_start = offset;

    loop {
        let part_end = bytes[part_start..]
            .iter()
            .position(|&b| b == b',')
            .map(|n| part_start + n)
            .unwrap_or(bytes.len());

        if part_start == part_end {
            return Err(Error::EmptyVariable {
                pos: content_start + part_start,
            });
        }

        let part = &content[part_start..part_end];
        let part_bytes = part.as_bytes();
        let modifier_index = part_bytes
            .iter()
            .position(|&b| b == b'*' || b == b':')
            .unwrap_or(part_bytes.len());
        let name = &part[..modifier_index];

        if name.is_empty() {
            return Err(Error::InvalidName {
                pos: content_start + part_start,
            });
        }

        let name_bytes = name.as_bytes();
        if !is_name_start(name_bytes[0]) {
            return Err(Error::InvalidName {
                pos: content_start + part_start,
            });
        }
        for (i, &byte) in name_bytes.iter().enumerate().skip(1) {
            if !is_name_continue(byte) {
                return Err(Error::InvalidName {
                    pos: content_start + part_start + i,
                });
            }
        }

        let suffix = &part[modifier_index..];
        let modifier = if suffix.is_empty() {
            Modifier::None
        } else if suffix == "*" {
            Modifier::Explode
        } else if let Some(digits) = suffix.strip_prefix(':') {
            let digit_bytes = digits.as_bytes();
            let valid = !digit_bytes.is_empty()
                && digit_bytes.len() <= 4
                && (b'1'..=b'9').contains(&digit_bytes[0])
                && digit_bytes.iter().all(u8::is_ascii_digit);

            if !valid {
                return Err(Error::InvalidModifier {
                    pos: content_start + part_start + modifier_index,
                });
            }

            let count = digits.parse::<usize>().unwrap_or(0);
            if !(1..=9999).contains(&count) {
                return Err(Error::InvalidModifier {
                    pos: content_start + part_start + modifier_index,
                });
            }
            Modifier::Prefix(count)
        } else {
            return Err(Error::InvalidModifier {
                pos: content_start + part_start + modifier_index,
            });
        };

        if seen.contains(&name) {
            return Err(Error::DuplicateVariable {
                pos: content_start + part_start,
            });
        }
        seen.push(name);
        specs.push(Specifier {
            name,
            modifier,
            pos: content_start + part_start,
        });

        if part_end == bytes.len() {
            break;
        }
        part_start = part_end + 1;
        if part_start == bytes.len() {
            return Err(Error::EmptyVariable { pos: close_pos });
        }
    }

    Ok(Expression {
        operator,
        specs,
        open_pos,
        close_pos,
    })
}

fn syntax_pass(template: &str) -> Result<Vec<Expression<'_>>, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'}' => return Err(Error::UnexpectedClose { pos: i }),
            b'{' => {
                let open = i;
                let Some(relative_close) = bytes[i + 1..].iter().position(|&b| b == b'}') else {
                    return Err(Error::Unclosed { pos: open });
                };
                let close = i + 1 + relative_close;

                if let Some(relative_open) = bytes[open + 1..close]
                    .iter()
                    .position(|&b| b == b'{')
                {
                    return Err(Error::NestedOpen {
                        pos: open + 1 + relative_open,
                    });
                }

                let content_start = open + 1;
                let content = &template[content_start..close];
                expressions.push(parse_expression(content, content_start, open, close)?);
                i = close + 1;
            }
            _ => i += 1,
        }
    }

    Ok(expressions)
}

fn selected_value<'a>(name: &str, vars: &'a [(&str, Value)]) -> Option<&'a Value> {
    vars.iter()
        .rev()
        .find(|(binding_name, _)| *binding_name == name)
        .map(|(_, value)| value)
}

fn is_reserved(byte: u8) -> bool {
    matches!(
        byte,
        b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@' | b'!' | b'$' | b'&' | b'\''
            | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
    )
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
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
    let mut result = String::new();
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];

        if byte == b'%' {
            if i + 2 < bytes.len()
                && hex_value(bytes[i + 1]).is_some()
                && hex_value(bytes[i + 2]).is_some()
            {
                result.push('%');
                result.push(hex_digit(hex_value(bytes[i + 1]).unwrap_or(0)) as char);
                result.push(hex_digit(hex_value(bytes[i + 2]).unwrap_or(0)) as char);
                i += 3;
                continue;
            }
            result.push_str("%25");
        } else if is_unreserved(byte) || (reserved && is_reserved(byte)) {
            result.push(byte as char);
        } else {
            result.push('%');
            result.push(hex_digit(byte >> 4) as char);
            result.push(hex_digit(byte & 0x0f) as char);
        }
        i += 1;
    }

    result
}

fn prefix_chars(value: &str, count: usize) -> &str {
    match value.char_indices().nth(count) {
        Some((index, _)) => &value[..index],
        None => value,
    }
}

fn naming_field(name: &str, encoded_value: String, operator: Operator) -> String {
    if operator == Operator::Semicolon && encoded_value.is_empty() {
        name.to_owned()
    } else {
        format!("{name}={encoded_value}")
    }
}

fn expand_expression(expression: &Expression<'_>, vars: &[(&str, Value)]) -> Result<String, Error> {
    let op = expression.operator;
    let reserved = matches!(op, Operator::Reserved | Operator::Fragment);
    let separator = match op {
        Operator::Simple | Operator::Reserved | Operator::Fragment => ',',
        Operator::Dot => '.',
        Operator::Slash => '/',
        Operator::Semicolon => ';',
        Operator::Question | Operator::Ampersand => '&',
    };

    let mut fields = Vec::<String>::new();

    for spec in &expression.specs {
        let Some(value) = selected_value(spec.name, vars) else {
            continue;
        };

        match value {
            Value::Scalar(raw) => {
                let raw = match spec.modifier {
                    Modifier::Prefix(count) => prefix_chars(raw, count),
                    _ => raw.as_str(),
                };
                let encoded = encode(raw, reserved);
                fields.push(match op {
                    Operator::Semicolon | Operator::Question | Operator::Ampersand => {
                        naming_field(spec.name, encoded, op)
                    }
                    _ => encoded,
                });
            }
            Value::List(items) => {
                if matches!(spec.modifier, Modifier::Prefix(_)) {
                    return Err(Error::TypeMismatch { pos: spec.pos });
                }
                if items.is_empty() {
                    continue;
                }

                if spec.modifier == Modifier::Explode {
                    for item in items {
                        let encoded = encode(item, reserved);
                        fields.push(match op {
                            Operator::Semicolon | Operator::Question | Operator::Ampersand => {
                                naming_field(spec.name, encoded, op)
                            }
                            _ => encoded,
                        });
                    }
                } else {
                    let joined = items
                        .iter()
                        .map(|item| encode(item, reserved))
                        .collect::<Vec<_>>()
                        .join(",");
                    fields.push(match op {
                        Operator::Semicolon | Operator::Question | Operator::Ampersand => {
                            naming_field(spec.name, joined, op)
                        }
                        _ => joined,
                    });
                }
            }
            Value::Map(entries) => {
                if matches!(spec.modifier, Modifier::Prefix(_)) {
                    return Err(Error::TypeMismatch { pos: spec.pos });
                }

                let mut keys = Vec::<&str>::new();
                for (key, _) in entries {
                    if keys.contains(&key.as_str()) {
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
                        let field = if op == Operator::Semicolon && encoded_value.is_empty() {
                            encoded_key
                        } else {
                            format!("{encoded_key}={encoded_value}")
                        };
                        fields.push(field);
                    }
                } else {
                    let mut components = Vec::with_capacity(entries.len() * 2);
                    for (key, value) in entries {
                        components.push(encode(key, reserved));
                        components.push(encode(value, reserved));
                    }
                    let joined = components.join(",");
                    fields.push(match op {
                        Operator::Semicolon | Operator::Question | Operator::Ampersand => {
                            naming_field(spec.name, joined, op)
                        }
                        _ => joined,
                    });
                }
            }
        }
    }

    if fields.is_empty() {
        return Ok(String::new());
    }

    let mut result = String::new();
    match op {
        Operator::Simple | Operator::Reserved => {}
        Operator::Fragment => result.push('#'),
        Operator::Dot => result.push('.'),
        Operator::Slash => result.push('/'),
        Operator::Semicolon => result.push(';'),
        Operator::Question => result.push('?'),
        Operator::Ampersand => result.push('&'),
    }
    result.push_str(&fields.join(&separator.to_string()));
    Ok(result)
}

pub fn expand(template: &str, vars: &[(&str, Value)]) -> Result<String, Error> {
    let expressions = syntax_pass(template)?;
    let mut result = String::with_capacity(template.len());
    let mut cursor = 0;

    for expression in &expressions {
        result.push_str(&template[cursor..expression.open_pos]);
        result.push_str(&expand_expression(expression, vars)?);
        cursor = expression.close_pos + 1;
    }

    result.push_str(&template[cursor..]);
    Ok(result)
}