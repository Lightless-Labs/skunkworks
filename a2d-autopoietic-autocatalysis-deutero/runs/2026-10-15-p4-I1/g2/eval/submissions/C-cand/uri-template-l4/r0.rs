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
    Continuation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Modifier {
    None,
    Explode,
    Prefix(usize),
}

#[derive(Debug, Clone)]
struct Specifier<'a> {
    name: &'a str,
    modifier: Modifier,
    pos: usize,
}

#[derive(Debug, Clone)]
struct Expression<'a> {
    operator: Operator,
    specs: Vec<Specifier<'a>>,
    close_pos: usize,
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn parse_expression(content: &str, content_pos: usize, close_pos: usize) -> Result<Expression<'_>, Error> {
    let bytes = content.as_bytes();
    let mut operator = Operator::Simple;
    let mut start = 0;

    if let Some(&first) = bytes.first() {
        operator = match first {
            b'+' => Operator::Reserved,
            b'#' => Operator::Fragment,
            b'.' => Operator::Dot,
            b'/' => Operator::Slash,
            b';' => Operator::Semicolon,
            b'?' => Operator::Query,
            b'&' => Operator::Continuation,
            b'!' | b'=' | b'@' | b'|' => {
                return Err(Error::UnsupportedOperator {
                    pos: content_pos,
                });
            }
            _ => Operator::Simple,
        };
        if operator != Operator::Simple {
            start = 1;
        }
    }

    if start == bytes.len() {
        return Err(Error::EmptyExpression { pos: close_pos });
    }

    let mut specs = Vec::new();
    let mut cursor = start;

    loop {
        let relative_end = content[cursor..]
            .find(',')
            .map(|offset| cursor + offset)
            .unwrap_or(content.len());
        let spec = &content[cursor..relative_end];

        if spec.is_empty() {
            return Err(Error::EmptyVariable {
                pos: content_pos + cursor,
            });
        }

        let spec_bytes = spec.as_bytes();
        let modifier_at = spec_bytes
            .iter()
            .position(|&byte| byte == b'*' || byte == b':')
            .unwrap_or(spec_bytes.len());
        let name = &spec[..modifier_at];

        if name.is_empty() {
            return Err(Error::InvalidName {
                pos: content_pos + cursor,
            });
        }

        let name_bytes = name.as_bytes();
        if !is_name_start(name_bytes[0]) {
            return Err(Error::InvalidName {
                pos: content_pos + cursor,
            });
        }
        if let Some(index) = name_bytes.iter().position(|&byte| !is_name_continue(byte)) {
            return Err(Error::InvalidName {
                pos: content_pos + cursor + index,
            });
        }

        let modifier = if modifier_at == spec_bytes.len() {
            Modifier::None
        } else {
            let modifier_pos = content_pos + cursor + modifier_at;
            let suffix = &spec[modifier_at..];
            if suffix == "*" {
                Modifier::Explode
            } else if let Some(digits) = suffix.strip_prefix(':') {
                let digit_bytes = digits.as_bytes();
                let valid = (1..=4).contains(&digit_bytes.len())
                    && digit_bytes[0].is_ascii_digit()
                    && digit_bytes[0] != b'0'
                    && digit_bytes.iter().all(u8::is_ascii_digit);
                if !valid {
                    return Err(Error::InvalidModifier { pos: modifier_pos });
                }
                let mut value = 0usize;
                for &digit in digit_bytes {
                    value = value * 10 + (digit - b'0') as usize;
                }
                if !(1..=9999).contains(&value) {
                    return Err(Error::InvalidModifier { pos: modifier_pos });
                }
                Modifier::Prefix(value)
            } else {
                return Err(Error::InvalidModifier { pos: modifier_pos });
            }
        };

        if specs.iter().any(|existing: &Specifier<'_>| existing.name == name) {
            return Err(Error::DuplicateVariable {
                pos: content_pos + cursor,
            });
        }

        specs.push(Specifier {
            name,
            modifier,
            pos: content_pos + cursor,
        });

        if relative_end == content.len() {
            break;
        }
        cursor = relative_end + 1;
    }

    Ok(Expression {
        operator,
        specs,
        close_pos,
    })
}

fn parse_template(template: &str) -> Result<Vec<TemplatePart<'_>>, Error> {
    let mut parts = Vec::new();
    let mut literal_start = 0;
    let mut cursor = 0;

    while cursor < template.len() {
        let byte = template.as_bytes()[cursor];
        if byte == b'}' {
            return Err(Error::UnexpectedClose { pos: cursor });
        }
        if byte != b'{' {
            cursor += template[cursor..].chars().next().unwrap().len_utf8();
            continue;
        }

        if literal_start < cursor {
            parts.push(TemplatePart::Literal(&template[literal_start..cursor]));
        }

        let open_pos = cursor;
        let content_start = cursor + 1;
        let close_pos = match template[content_start..].find('}') {
            Some(offset) => content_start + offset,
            None => return Err(Error::Unclosed { pos: open_pos }),
        };

        if let Some(offset) = template[content_start..close_pos].find('{') {
            return Err(Error::NestedOpen {
                pos: content_start + offset,
            });
        }

        let expression = parse_expression(
            &template[content_start..close_pos],
            content_start,
            close_pos,
        )?;
        parts.push(TemplatePart::Expression(expression));
        cursor = close_pos + 1;
        literal_start = cursor;
    }

    if literal_start < template.len() {
        parts.push(TemplatePart::Literal(&template[literal_start..]));
    }
    Ok(parts)
}

#[derive(Debug, Clone)]
enum TemplatePart<'a> {
    Literal(&'a str),
    Expression(Expression<'a>),
}

fn lookup<'a>(name: &str, vars: &'a [(&str, Value)]) -> Option<&'a Value> {
    vars.iter()
        .rev()
        .find(|(binding_name, _)| *binding_name == name)
        .map(|(_, value)| value)
}

fn encode(input: &str, reserved: bool) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let bytes = input.as_bytes();
    let mut output = String::new();
    let mut index = 0;

    while index < bytes.len() {
        let byte = bytes[index];

        if byte == b'%' && index + 2 < bytes.len() {
            let high = hex_value(bytes[index + 1]);
            let low = hex_value(bytes[index + 2]);
            if let (Some(high), Some(low)) = (high, low) {
                output.push('%');
                output.push(HEX[high as usize] as char);
                output.push(HEX[low as usize] as char);
                index += 3;
                continue;
            }
        }

        if is_unreserved(byte) || (reserved && is_reserved(byte)) {
            output.push(byte as char);
        } else {
            output.push('%');
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
        }
        index += 1;
    }

    output
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
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

fn apply_prefix(value: &str, modifier: Modifier, pos: usize) -> Result<String, Error> {
    match modifier {
        Modifier::Prefix(limit) => Ok(value.chars().take(limit).collect()),
        Modifier::None | Modifier::Explode => Ok(value.to_owned()),
    }
}

fn named_field(name: &str, value: String, operator: Operator) -> String {
    match operator {
        Operator::Semicolon if value.is_empty() => name.to_owned(),
        _ => format!("{name}={value}"),
    }
}

fn expand_expression(expression: &Expression<'_>, vars: &[(&str, Value)]) -> Result<String, Error> {
    let operator = expression.operator;
    let reserved = matches!(operator, Operator::Reserved | Operator::Fragment);
    let mut fields = Vec::new();

    for spec in &expression.specs {
        let Some(value) = lookup(spec.name, vars) else {
            continue;
        };

        if matches!(spec.modifier, Modifier::Prefix(_))
            && !matches!(value, Value::Scalar(_))
        {
            return Err(Error::TypeMismatch { pos: spec.pos });
        }

        match value {
            Value::Scalar(raw) => {
                let raw = apply_prefix(raw, spec.modifier, spec.pos)?;
                let encoded = encode(&raw, reserved);
                let field = if matches!(operator, Operator::Semicolon | Operator::Query | Operator::Continuation) {
                    named_field(spec.name, encoded, operator)
                } else {
                    encoded
                };
                fields.push(field);
            }
            Value::List(items) => {
                if items.is_empty() {
                    continue;
                }
                let explode = matches!(spec.modifier, Modifier::Explode);
                if explode {
                    for item in items {
                        let encoded = encode(item, reserved);
                        let field = if matches!(operator, Operator::Semicolon | Operator::Query | Operator::Continuation) {
                            named_field(spec.name, encoded, operator)
                        } else {
                            encoded
                        };
                        fields.push(field);
                    }
                } else {
                    let joined = items
                        .iter()
                        .map(|item| encode(item, reserved))
                        .collect::<Vec<_>>()
                        .join(",");
                    let field = if matches!(operator, Operator::Semicolon | Operator::Query | Operator::Continuation) {
                        named_field(spec.name, joined, operator)
                    } else {
                        joined
                    };
                    fields.push(field);
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

                let explode = matches!(spec.modifier, Modifier::Explode);
                if explode {
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
                    let mut components = Vec::with_capacity(entries.len() * 2);
                    for (key, value) in entries {
                        components.push(encode(key, reserved));
                        components.push(encode(value, reserved));
                    }
                    let joined = components.join(",");
                    let field = if matches!(operator, Operator::Semicolon | Operator::Query | Operator::Continuation) {
                        named_field(spec.name, joined, operator)
                    } else {
                        joined
                    };
                    fields.push(field);
                }
            }
        }
    }

    if fields.is_empty() {
        return Ok(String::new());
    }

    let (prefix, separator) = match operator {
        Operator::Simple | Operator::Reserved => ("", ","),
        Operator::Fragment => ("#", ","),
        Operator::Dot => (".", "."),
        Operator::Slash => ("/", "/"),
        Operator::Semicolon => (";", ";"),
        Operator::Query => ("?", "&"),
        Operator::Continuation => ("&", "&"),
    };

    Ok(format!("{prefix}{}", fields.join(separator)))
}

pub fn expand(template: &str, vars: &[(&str, Value)]) -> Result<String, Error> {
    let parts = parse_template(template)?;
    let mut output = String::new();

    for part in parts {
        match part {
            TemplatePart::Literal(literal) => output.push_str(literal),
            TemplatePart::Expression(expression) => {
                output.push_str(&expand_expression(&expression, vars)?);
            }
        }
    }

    Ok(output)
}

fn main() {}