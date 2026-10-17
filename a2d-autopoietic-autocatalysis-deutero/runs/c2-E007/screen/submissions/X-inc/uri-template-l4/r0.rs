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
enum Modifier {
    Explode,
    Prefix(usize),
}

struct Specifier<'a> {
    name: &'a str,
    name_start: usize,
    start: usize,
    modifier: Option<Modifier>,
}

struct Expression<'a> {
    start: usize,
    end: usize,
    operator: Option<char>,
    specifiers: Vec<Specifier<'a>>,
}

fn is_name_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_name_continue(b: u8) -> bool {
    is_name_start(b) || b.is_ascii_digit()
}

fn parse_expression<'a>(
    template: &'a str,
    open: usize,
    close: usize,
) -> Result<Expression<'a>, Error> {
    let bytes = template.as_bytes();
    let mut content_start = open + 1;
    let mut operator = None;

    if content_start < close {
        let first = bytes[content_start] as char;
        if matches!(first, '+' | '#' | '.' | '/' | ';' | '?' | '&') {
            operator = Some(first);
            content_start += 1;
        } else if matches!(first, '=' | '!' | '@' | '|') {
            return Err(Error::UnsupportedOperator { pos: content_start });
        }
    }

    if content_start == close {
        return Err(Error::EmptyExpression { pos: close });
    }

    let mut specifiers = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut part_start = content_start;

    loop {
        let part_end = bytes[part_start..close]
            .iter()
            .position(|&b| b == b',')
            .map(|offset| part_start + offset)
            .unwrap_or(close);

        if part_start == part_end {
            return Err(Error::EmptyVariable { pos: part_start });
        }

        let part = &template[part_start..part_end];
        let modifier_offset = part
            .bytes()
            .position(|b| b == b'*' || b == b':')
            .unwrap_or(part.len());
        let name = &part[..modifier_offset];

        if name.is_empty() {
            return Err(Error::InvalidName { pos: part_start });
        }

        for (offset, byte) in name.bytes().enumerate() {
            let valid = if offset == 0 {
                is_name_start(byte)
            } else {
                is_name_continue(byte)
            };
            if !valid {
                return Err(Error::InvalidName {
                    pos: part_start + offset,
                });
            }
        }

        let modifier = if modifier_offset == part.len() {
            None
        } else {
            let modifier_pos = part_start + modifier_offset;
            let suffix = &part[modifier_offset..];
            if suffix == "*" {
                Some(Modifier::Explode)
            } else if let Some(digits) = suffix.strip_prefix(':') {
                let valid_digits = (1..=4).contains(&digits.len())
                    && digits
                        .bytes()
                        .all(|b| b.is_ascii_digit())
                    && digits.as_bytes()[0] != b'0';
                if !valid_digits {
                    return Err(Error::InvalidModifier { pos: modifier_pos });
                }
                let count = digits.parse::<usize>().unwrap_or(0);
                if !(1..=9999).contains(&count) {
                    return Err(Error::InvalidModifier { pos: modifier_pos });
                }
                Some(Modifier::Prefix(count))
            } else {
                return Err(Error::InvalidModifier { pos: modifier_pos });
            }
        };

        if !seen.insert(name.to_string()) {
            return Err(Error::DuplicateVariable { pos: part_start });
        }

        specifiers.push(Specifier {
            name,
            name_start: part_start,
            start: part_start,
            modifier,
        });

        if part_end == close {
            break;
        }
        part_start = part_end + 1;
    }

    Ok(Expression {
        start: open,
        end: close + 1,
        operator,
        specifiers,
    })
}

fn validate_template(template: &str) -> Result<Vec<Expression<'_>>, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'}' => return Err(Error::UnexpectedClose { pos: i }),
            b'{' => {
                let open = i;
                let close = match bytes[i + 1..].iter().position(|&b| b == b'}') {
                    Some(offset) => i + 1 + offset,
                    None => return Err(Error::Unclosed { pos: open }),
                };
                if let Some(offset) = bytes[open + 1..close].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen {
                        pos: open + 1 + offset,
                    });
                }
                expressions.push(parse_expression(template, open, close)?);
                i = close + 1;
            }
            _ => i += 1,
        }
    }

    Ok(expressions)
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~')
}

fn is_reserved(b: u8) -> bool {
    matches!(
        b,
        b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@' | b'!' | b'$' | b'&'
            | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
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

fn encode(input: &str, allow_reserved: bool) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let bytes = input.as_bytes();
    let mut out = String::new();
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' && i + 2 < bytes.len() {
            if hex_value(bytes[i + 1]).is_some() && hex_value(bytes[i + 2]).is_some() {
                out.push('%');
                out.push(bytes[i + 1].to_ascii_uppercase() as char);
                out.push(bytes[i + 2].to_ascii_uppercase() as char);
                i += 3;
                continue;
            }
        }
        if is_unreserved(b) || (allow_reserved && is_reserved(b)) {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0f) as usize] as char);
        }
        i += 1;
    }
    out
}

fn encode_prefix(input: &str, count: usize, allow_reserved: bool) -> String {
    let prefix: String = input.chars().take(count).collect();
    encode(&prefix, allow_reserved)
}

fn lookup<'a>(vars: &'a [(&str, Value)], name: &str) -> Option<&'a Value> {
    vars.iter()
        .rev()
        .find(|(binding, _)| *binding == name)
        .map(|(_, value)| value)
}

fn encode_scalar(value: &str, modifier: Option<Modifier>, allow_reserved: bool) -> String {
    match modifier {
        Some(Modifier::Prefix(count)) => encode_prefix(value, count, allow_reserved),
        _ => encode(value, allow_reserved),
    }
}

fn named_field(name: &str, value: String, operator: char) -> String {
    if operator == ';' && value.is_empty() {
        name.to_string()
    } else {
        format!("{name}={value}")
    }
}

fn expand_expression(expr: &Expression<'_>, vars: &[(&str, Value)]) -> Result<String, Error> {
    let operator = expr.operator.unwrap_or('\0');
    let allow_reserved = matches!(operator, '+' | '#');
    let mut fields = Vec::new();

    for spec in &expr.specifiers {
        let Some(value) = lookup(vars, spec.name) else {
            continue;
        };

        if matches!(spec.modifier, Some(Modifier::Prefix(_)))
            && !matches!(value, Value::Scalar(_))
        {
            return Err(Error::TypeMismatch { pos: spec.start });
        }

        match value {
            Value::Scalar(s) => {
                let encoded = encode_scalar(s, spec.modifier, allow_reserved);
                if matches!(operator, ';' | '?' | '&') {
                    fields.push(named_field(spec.name, encoded, operator));
                } else {
                    fields.push(encoded);
                }
            }
            Value::List(items) => {
                let exploded = spec.modifier == Some(Modifier::Explode);
                if items.is_empty() {
                    continue;
                }
                if exploded {
                    for item in items {
                        let encoded = encode(item, allow_reserved);
                        if matches!(operator, ';' | '?' | '&') {
                            fields.push(named_field(spec.name, encoded, operator));
                        } else {
                            fields.push(encoded);
                        }
                    }
                } else {
                    let joined = items
                        .iter()
                        .map(|item| encode(item, allow_reserved))
                        .collect::<Vec<_>>()
                        .join(",");
                    if matches!(operator, ';' | '?' | '&') {
                        fields.push(named_field(spec.name, joined, operator));
                    } else {
                        fields.push(joined);
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
                if spec.modifier == Some(Modifier::Explode) {
                    for (key, val) in entries {
                        let encoded_key = encode(key, allow_reserved);
                        let encoded_value = encode(val, allow_reserved);
                        if operator == ';' && encoded_value.is_empty() {
                            fields.push(encoded_key);
                        } else {
                            fields.push(format!("{encoded_key}={encoded_value}"));
                        }
                    }
                } else {
                    let mut components = Vec::with_capacity(entries.len() * 2);
                    for (key, val) in entries {
                        components.push(encode(key, allow_reserved));
                        components.push(encode(val, allow_reserved));
                    }
                    let joined = components.join(",");
                    if matches!(operator, ';' | '?' | '&') {
                        fields.push(named_field(spec.name, joined, operator));
                    } else {
                        fields.push(joined);
                    }
                }
            }
        }
    }

    if fields.is_empty() {
        return Ok(String::new());
    }

    let (prefix, separator) = match operator {
        '+' => ("", ","),
        '#' => ("#", ","),
        '.' => (".", "."),
        '/' => ("/", "/"),
        ';' => (";", ";"),
        '?' => ("?", "&"),
        '&' => ("&", "&"),
        _ => ("", ","),
    };

    Ok(format!("{prefix}{}", fields.join(separator)))
}

pub fn expand(template: &str, vars: &[(&str, Value)]) -> Result<String, Error> {
    let expressions = validate_template(template)?;
    let mut out = String::new();
    let mut cursor = 0;

    for expr in &expressions {
        out.push_str(&template[cursor..expr.start]);
        out.push_str(&expand_expression(expr, vars)?);
        cursor = expr.end;
    }

    out.push_str(&template[cursor..]);
    Ok(out)
}