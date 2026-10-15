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
    Plus,
    Hash,
    Dot,
    Slash,
    Semicolon,
    Question,
    Ampersand,
}

#[derive(Clone, Copy)]
enum Modifier {
    None,
    Explode,
    Prefix(usize),
}

#[derive(Clone)]
struct Specifier {
    name: String,
    pos: usize,
    modifier: Modifier,
}

struct Expression {
    operator: Operator,
    specs: Vec<Specifier>,
}

enum Part {
    Literal(String),
    Expression(Expression),
}

fn is_name_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_name_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn parse_expression(content: &str, content_pos: usize, close_pos: usize) -> Result<Expression, Error> {
    let bytes = content.as_bytes();
    let (operator, start) = if bytes.is_empty() {
        return Err(Error::EmptyExpression { pos: close_pos });
    } else {
        match bytes[0] {
            b'+' => (Operator::Plus, 1),
            b'#' => (Operator::Hash, 1),
            b'.' => (Operator::Dot, 1),
            b'/' => (Operator::Slash, 1),
            b';' => (Operator::Semicolon, 1),
            b'?' => (Operator::Question, 1),
            b'&' => (Operator::Ampersand, 1),
            b'!' | b'=' | b'@' | b'|' => {
                return Err(Error::UnsupportedOperator {
                    pos: content_pos,
                });
            }
            _ => (Operator::Simple, 0),
        }
    };

    if start == bytes.len() {
        return Err(Error::EmptyExpression { pos: close_pos });
    }

    let mut specs = Vec::new();
    let mut used = std::collections::HashSet::<String>::new();
    let mut spec_start = start;

    for end in (start..=bytes.len()).filter(|&i| i == bytes.len() || bytes[i] == b',') {
        if end == spec_start {
            return Err(Error::EmptyVariable {
                pos: content_pos + spec_start,
            });
        }

        let spec = &content[spec_start..end];
        let spec_bytes = spec.as_bytes();
        let modifier_at = spec_bytes.iter().position(|&b| b == b'*' || b == b':');
        let name_end = modifier_at.unwrap_or(spec_bytes.len());

        if name_end == 0 {
            return Err(Error::InvalidName {
                pos: content_pos + spec_start,
            });
        }

        let name_bytes = &spec_bytes[..name_end];
        if !is_name_start(name_bytes[0]) {
            return Err(Error::InvalidName {
                pos: content_pos + spec_start,
            });
        }
        if let Some(offset) = name_bytes.iter().position(|&b| !is_name_continue(b)) {
            return Err(Error::InvalidName {
                pos: content_pos + spec_start + offset,
            });
        }

        let name = &spec[..name_end];
        let modifier = if let Some(offset) = modifier_at {
            let suffix = &spec[offset..];
            if suffix == "*" {
                Modifier::Explode
            } else if suffix.starts_with(':') {
                let digits = &suffix[1..];
                let valid_digits = !digits.is_empty()
                    && digits.len() <= 4
                    && digits.as_bytes()[0].is_ascii_digit()
                    && digits.as_bytes()[0] != b'0'
                    && digits.bytes().all(|b| b.is_ascii_digit());
                let value = if valid_digits {
                    digits.parse::<usize>().ok()
                } else {
                    None
                };
                match value {
                    Some(n) if (1..=9999).contains(&n) => Modifier::Prefix(n),
                    _ => {
                        return Err(Error::InvalidModifier {
                            pos: content_pos + spec_start + offset,
                        });
                    }
                }
            } else {
                return Err(Error::InvalidModifier {
                    pos: content_pos + spec_start + offset,
                });
            }
        } else {
            Modifier::None
        };

        if !used.insert(name.to_string()) {
            return Err(Error::DuplicateVariable {
                pos: content_pos + spec_start,
            });
        }

        specs.push(Specifier {
            name: name.to_string(),
            pos: content_pos + spec_start,
            modifier,
        });
        spec_start = end + 1;
    }

    Ok(Expression { operator, specs })
}

fn validate_template(template: &str) -> Result<Vec<Part>, Error> {
    let bytes = template.as_bytes();
    let mut parts = Vec::new();
    let mut i = 0;
    let mut literal_start = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'}' => return Err(Error::UnexpectedClose { pos: i }),
            b'{' => {
                if literal_start < i {
                    parts.push(Part::Literal(template[literal_start..i].to_string()));
                }
                let open = i;
                let close = match bytes[i + 1..].iter().position(|&b| b == b'}') {
                    Some(offset) => i + 1 + offset,
                    None => return Err(Error::Unclosed { pos: open }),
                };
                if let Some(offset) = bytes[i + 1..close].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen { pos: i + 1 + offset });
                }
                let content_pos = open + 1;
                let expression = parse_expression(
                    &template[content_pos..close],
                    content_pos,
                    close,
                )?;
                parts.push(Part::Expression(expression));
                i = close + 1;
                literal_start = i;
            }
            _ => i += 1,
        }
    }

    if literal_start < bytes.len() {
        parts.push(Part::Literal(template[literal_start..].to_string()));
    }
    Ok(parts)
}

fn selected_value<'a>(name: &str, vars: &'a [(&str, Value)]) -> Option<&'a Value> {
    vars.iter()
        .rev()
        .find(|(binding_name, _)| *binding_name == name)
        .map(|(_, value)| value)
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~')
}

fn is_reserved(b: u8) -> bool {
    matches!(
        b,
        b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@' | b'!' | b'$' | b'&' | b'\''
            | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
    )
}

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn push_percent(out: &mut String, b: u8) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    out.push('%');
    out.push(HEX[(b >> 4) as usize] as char);
    out.push(HEX[(b & 0x0f) as usize] as char);
}

fn encode(input: &str, reserved: bool) -> String {
    let bytes = input.as_bytes();
    let mut out = String::new();
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex_value(bytes[i + 1]), hex_value(bytes[i + 2])) {
                out.push('%');
                const HEX: &[u8; 16] = b"0123456789ABCDEF";
                out.push(HEX[hi as usize] as char);
                out.push(HEX[lo as usize] as char);
                i += 3;
                continue;
            }
        }

        if is_unreserved(b) || (reserved && is_reserved(b)) {
            out.push(b as char);
        } else {
            push_percent(&mut out, b);
        }
        i += 1;
    }
    out
}

fn named_field(operator: Operator, name: &str, value: String) -> String {
    match operator {
        Operator::Semicolon if value.is_empty() => name.to_string(),
        _ => format!("{name}={value}"),
    }
}

fn append_value_fields(
    expression: &Expression,
    spec: &Specifier,
    value: &Value,
    fields: &mut Vec<String>,
) -> Result<(), Error> {
    let reserved = matches!(expression.operator, Operator::Plus | Operator::Hash);
    let named = matches!(
        expression.operator,
        Operator::Semicolon | Operator::Question | Operator::Ampersand
    );

    match value {
        Value::Scalar(scalar) => {
            let scalar = match spec.modifier {
                Modifier::Prefix(n) => scalar.chars().take(n).collect::<String>(),
                Modifier::None | Modifier::Explode => scalar.clone(),
            };
            let encoded = encode(&scalar, reserved);
            fields.push(if named {
                named_field(expression.operator, &spec.name, encoded)
            } else {
                encoded
            });
        }
        Value::List(items) => {
            if matches!(spec.modifier, Modifier::Prefix(_)) {
                return Err(Error::TypeMismatch { pos: spec.pos });
            }
            if items.is_empty() {
                return Ok(());
            }
            let exploded = matches!(spec.modifier, Modifier::Explode);
            if exploded {
                for item in items {
                    let encoded = encode(item, reserved);
                    fields.push(if named {
                        named_field(expression.operator, &spec.name, encoded)
                    } else {
                        encoded
                    });
                }
            } else {
                let joined = items
                    .iter()
                    .map(|item| encode(item, reserved))
                    .collect::<Vec<_>>()
                    .join(",");
                fields.push(if named {
                    named_field(expression.operator, &spec.name, joined)
                } else {
                    joined
                });
            }
        }
        Value::Map(entries) => {
            if matches!(spec.modifier, Modifier::Prefix(_)) {
                return Err(Error::TypeMismatch { pos: spec.pos });
            }
            let mut keys = std::collections::HashSet::new();
            for (key, _) in entries {
                if !keys.insert(key) {
                    return Err(Error::DuplicateKey { pos: spec.pos });
                }
            }
            if entries.is_empty() {
                return Ok(());
            }
            let exploded = matches!(spec.modifier, Modifier::Explode);
            if exploded {
                for (key, value) in entries {
                    let encoded_key = encode(key, reserved);
                    let encoded_value = encode(value, reserved);
                    let field = if expression.operator == Operator::Semicolon
                        && encoded_value.is_empty()
                    {
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
                fields.push(if named {
                    named_field(expression.operator, &spec.name, joined)
                } else {
                    joined
                });
            }
        }
    }
    Ok(())
}

fn expand_expression(expression: &Expression, vars: &[(&str, Value)]) -> Result<String, Error> {
    let mut fields = Vec::new();
    for spec in &expression.specs {
        if let Some(value) = selected_value(&spec.name, vars) {
            append_value_fields(expression, spec, value, &mut fields)?;
        }
    }

    if fields.is_empty() {
        return Ok(String::new());
    }

    let (prefix, separator) = match expression.operator {
        Operator::Simple | Operator::Plus => ("", ","),
        Operator::Hash => ("#", ","),
        Operator::Dot => (".", "."),
        Operator::Slash => ("/", "/"),
        Operator::Semicolon => (";", ";"),
        Operator::Question => ("?", "&"),
        Operator::Ampersand => ("&", "&"),
    };

    Ok(format!("{prefix}{}", fields.join(separator)))
}

pub fn expand(template: &str, vars: &[(&str, Value)]) -> Result<String, Error> {
    let parts = validate_template(template)?;
    let mut output = String::new();
    for part in parts {
        match part {
            Part::Literal(literal) => output.push_str(&literal),
            Part::Expression(expression) => {
                output.push_str(&expand_expression(&expression, vars)?);
            }
        }
    }
    Ok(output)
}

fn main() {}