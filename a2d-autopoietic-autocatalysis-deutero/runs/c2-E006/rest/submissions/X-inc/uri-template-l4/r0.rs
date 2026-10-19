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
    Fragment,
    Dot,
    Slash,
    Semicolon,
    Query,
    Ampersand,
}

#[derive(Clone)]
struct Specifier {
    name: String,
    name_start: usize,
    start: usize,
    modifier: Modifier,
}

#[derive(Clone, Copy)]
enum Modifier {
    None,
    Explode,
    Prefix(usize),
}

fn byte_pos(template: &str, offset: usize) -> usize {
    template[..offset].len()
}

fn parse_specifier(content: &str, start: usize) -> Result<Specifier, Error> {
    let modifier_at = content.find(['*', ':']);
    let (name, suffix, modifier_pos) = match modifier_at {
        Some(i) => (&content[..i], &content[i..], start + i),
        None => (content, "", start + content.len()),
    };

    if name.is_empty() {
        return Err(Error::InvalidName { pos: start });
    }

    for (i, ch) in name.char_indices() {
        let valid = if i == 0 {
            ch.is_ascii_alphabetic() || ch == '_'
        } else {
            ch.is_ascii_alphanumeric() || ch == '_'
        };
        if !valid {
            return Err(Error::InvalidName { pos: start + i });
        }
    }

    let modifier = if suffix.is_empty() {
        Modifier::None
    } else if suffix == "*" {
        Modifier::Explode
    } else if let Some(digits) = suffix.strip_prefix(':') {
        if digits.is_empty()
            || digits.len() > 4
            || !digits.bytes().all(|b| b.is_ascii_digit())
            || digits.as_bytes()[0] == b'0'
        {
            return Err(Error::InvalidModifier { pos: modifier_pos });
        }
        let n = digits.parse::<usize>().unwrap_or(0);
        if !(1..=9999).contains(&n) {
            return Err(Error::InvalidModifier { pos: modifier_pos });
        }
        Modifier::Prefix(n)
    } else {
        return Err(Error::InvalidModifier { pos: modifier_pos });
    };

    Ok(Specifier {
        name: name.to_owned(),
        name_start: start,
        start,
        modifier,
    })
}

fn parse_expression(
    template: &str,
    open: usize,
    close: usize,
) -> Result<(Operator, Vec<Specifier>), Error> {
    let inner_start = open + 1;
    let content = &template[inner_start..close];

    if let Some(i) = content.find('{') {
        return Err(Error::NestedOpen {
            pos: inner_start + i,
        });
    }

    let (operator, consumed) = match content.as_bytes().first().copied() {
        Some(b'+') => (Operator::Plus, 1),
        Some(b'#') => (Operator::Fragment, 1),
        Some(b'.') => (Operator::Dot, 1),
        Some(b'/') => (Operator::Slash, 1),
        Some(b';') => (Operator::Semicolon, 1),
        Some(b'?') => (Operator::Query, 1),
        Some(b'&') => (Operator::Ampersand, 1),
        _ => (Operator::Simple, 0),
    };

    if content.is_empty() || content.len() == consumed {
        return Err(Error::EmptyExpression { pos: close });
    }

    let specs_content = &content[consumed..];
    let specs_start = inner_start + consumed;
    let mut specs = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut offset = 0;

    for part in specs_content.split(',') {
        if part.is_empty() {
            return Err(Error::EmptyVariable {
                pos: specs_start + offset,
            });
        }
        let spec = parse_specifier(part, specs_start + offset)?;
        if !seen.insert(spec.name.clone()) {
            return Err(Error::DuplicateVariable { pos: spec.start });
        }
        specs.push(spec);
        offset += part.len() + 1;
    }

    Ok((operator, specs))
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

fn hex_digit(value: u8) -> u8 {
    if value < 10 {
        b'0' + value
    } else {
        b'A' + (value - 10)
    }
}

fn encode(value: &str, reserved: bool) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];
        if byte == b'%' && i + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex_value(bytes[i + 1]), hex_value(bytes[i + 2])) {
                out.extend_from_slice(&[
                    b'%',
                    hex_digit(high),
                    hex_digit(low),
                ]);
                i += 3;
                continue;
            }
        }

        if is_unreserved(byte) || (reserved && is_reserved(byte)) {
            out.push(byte);
        } else {
            out.extend_from_slice(&[b'%', hex_digit(byte >> 4), hex_digit(byte & 0x0f)]);
        }
        i += 1;
    }

    String::from_utf8(out).unwrap_or_default()
}

fn selected_value<'a>(name: &str, vars: &'a [(&str, Value)]) -> Option<&'a Value> {
    vars.iter()
        .rev()
        .find(|(binding, _)| *binding == name)
        .map(|(_, value)| value)
}

fn encoded_component(value: &str, operator: Operator) -> String {
    encode(value, matches!(operator, Operator::Plus | Operator::Fragment))
}

fn named_field(name: &str, value: &str, operator: Operator) -> String {
    let encoded = encoded_component(value, operator);
    if operator == Operator::Semicolon && encoded.is_empty() {
        name.to_owned()
    } else {
        format!("{name}={encoded}")
    }
}

fn expand_expression(
    operator: Operator,
    specs: &[Specifier],
    vars: &[(&str, Value)],
) -> Result<String, Error> {
    let mut fields = Vec::new();

    for spec in specs {
        let Some(value) = selected_value(&spec.name, vars) else {
            continue;
        };

        if matches!(spec.modifier, Modifier::Prefix(_))
            && !matches!(value, Value::Scalar(_))
        {
            return Err(Error::TypeMismatch { pos: spec.start });
        }

        match value {
            Value::Scalar(scalar) => {
                let value = match spec.modifier {
                    Modifier::Prefix(n) => scalar.chars().take(n).collect::<String>(),
                    _ => scalar.clone(),
                };
                let encoded = encoded_component(&value, operator);
                match operator {
                    Operator::Semicolon | Operator::Query | Operator::Ampersand => {
                        fields.push(named_field(&spec.name, &value, operator));
                    }
                    _ => fields.push(encoded),
                }
            }
            Value::List(items) => {
                let explode = matches!(spec.modifier, Modifier::Explode);
                if items.is_empty() {
                    continue;
                }
                if explode {
                    for item in items {
                        match operator {
                            Operator::Semicolon | Operator::Query | Operator::Ampersand => {
                                fields.push(named_field(&spec.name, item, operator));
                            }
                            _ => fields.push(encoded_component(item, operator)),
                        }
                    }
                } else {
                    let joined = items
                        .iter()
                        .map(|item| encoded_component(item, operator))
                        .collect::<Vec<_>>()
                        .join(",");
                    match operator {
                        Operator::Semicolon | Operator::Query | Operator::Ampersand => {
                            fields.push(named_encoded_field(&spec.name, joined, operator));
                        }
                        _ => fields.push(joined),
                    }
                }
            }
            Value::Map(entries) => {
                let mut keys = std::collections::HashSet::new();
                for (key, _) in entries {
                    if !keys.insert(key) {
                        return Err(Error::DuplicateKey { pos: spec.start });
                    }
                }

                if entries.is_empty() {
                    continue;
                }

                let explode = matches!(spec.modifier, Modifier::Explode);
                if explode {
                    for (key, value) in entries {
                        let encoded_key = encoded_component(key, operator);
                        let encoded_value = encoded_component(value, operator);
                        if operator == Operator::Semicolon && encoded_value.is_empty() {
                            fields.push(encoded_key);
                        } else {
                            fields.push(format!("{encoded_key}={encoded_value}"));
                        }
                    }
                } else {
                    let components = entries
                        .iter()
                        .flat_map(|(key, value)| {
                            [
                                encoded_component(key, operator),
                                encoded_component(value, operator),
                            ]
                        })
                        .collect::<Vec<_>>()
                        .join(",");
                    match operator {
                        Operator::Semicolon | Operator::Query | Operator::Ampersand => {
                            fields.push(named_encoded_field(&spec.name, components, operator));
                        }
                        _ => fields.push(components),
                    }
                }
            }
        }
    }

    if fields.is_empty() {
        return Ok(String::new());
    }

    let (prefix, separator) = match operator {
        Operator::Simple | Operator::Plus => ("", ","),
        Operator::Fragment => ("#", ","),
        Operator::Dot => (".", "."),
        Operator::Slash => ("/", "/"),
        Operator::Semicolon => (";", ";"),
        Operator::Query => ("?", "&"),
        Operator::Ampersand => ("&", "&"),
    };

    Ok(format!("{prefix}{}", fields.join(separator)))
}

fn named_encoded_field(name: &str, encoded: String, operator: Operator) -> String {
    if operator == Operator::Semicolon && encoded.is_empty() {
        name.to_owned()
    } else {
        format!("{name}={encoded}")
    }
}

pub fn expand(template: &str, vars: &[(&str, Value)]) -> Result<String, Error> {
    let mut expressions = Vec::new();
    let mut i = 0;

    while i < template.len() {
        match template.as_bytes()[i] {
            b'}' => return Err(Error::UnexpectedClose { pos: i }),
            b'{' => {
                let open = i;
                let Some(relative_close) = template[i + 1..].find('}') else {
                    return Err(Error::Unclosed { pos: open });
                };
                let close = i + 1 + relative_close;
                let parsed = parse_expression(template, open, close)?;
                expressions.push((open, close, parsed));
                i = close + 1;
            }
            _ => {
                let ch = template[i..].chars().next().unwrap_or('\0');
                i += ch.len_utf8();
            }
        }
    }

    let mut output = String::new();
    let mut cursor = 0;
    for (open, close, (operator, specs)) in expressions {
        output.push_str(&template[cursor..open]);
        output.push_str(&expand_expression(operator, &specs, vars)?);
        cursor = close + 1;
    }
    output.push_str(&template[cursor..]);

    Ok(output)
}