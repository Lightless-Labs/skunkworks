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
    Query,
    Continuation,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Modifier {
    None,
    Explode,
    Prefix(usize),
}

struct Specifier {
    name: String,
    start: usize,
    modifier: Modifier,
}

struct Expression {
    operator: Operator,
    specifiers: Vec<Specifier>,
}

fn is_name_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_name_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn parse_expression(content: &str, content_pos: usize, close_pos: usize) -> Result<Expression, Error> {
    let bytes = content.as_bytes();
    let (operator, offset) = if let Some(&b) = bytes.first() {
        let op = match b {
            b'+' => Some(Operator::Reserved),
            b'#' => Some(Operator::Fragment),
            b'.' => Some(Operator::Dot),
            b'/' => Some(Operator::Slash),
            b';' => Some(Operator::Semicolon),
            b'?' => Some(Operator::Query),
            b'&' => Some(Operator::Continuation),
            _ => None,
        };
        if let Some(op) = op {
            (op, 1)
        } else if b == b'!' || b == b'=' || b == b'@' || b == b'|' {
            return Err(Error::UnsupportedOperator { pos: content_pos });
        } else {
            (Operator::Simple, 0)
        }
    } else {
        (Operator::Simple, 0)
    };

    if offset == bytes.len() {
        return Err(Error::EmptyExpression { pos: close_pos });
    }

    let mut specs = Vec::new();
    let mut used = std::collections::HashSet::new();
    let mut start = offset;

    loop {
        let end = bytes[start..]
            .iter()
            .position(|&b| b == b',')
            .map(|i| start + i)
            .unwrap_or(bytes.len());

        if start == end {
            return Err(Error::EmptyVariable {
                pos: content_pos + start,
            });
        }

        let part = &content[start..end];
        let part_bytes = part.as_bytes();
        let modifier_at = part_bytes
            .iter()
            .position(|&b| b == b'*' || b == b':')
            .unwrap_or(part_bytes.len());

        let name_bytes = &part_bytes[..modifier_at];
        if name_bytes.is_empty() || !is_name_start(name_bytes[0]) {
            return Err(Error::InvalidName {
                pos: content_pos + start,
            });
        }
        if let Some(i) = name_bytes.iter().position(|&b| !is_name_continue(b)) {
            return Err(Error::InvalidName {
                pos: content_pos + start + i,
            });
        }

        let name = part[..modifier_at].to_owned();
        let suffix = &part[modifier_at..];
        let modifier = if suffix.is_empty() {
            Modifier::None
        } else if suffix == "*" {
            Modifier::Explode
        } else if let Some(digits) = suffix.strip_prefix(':') {
            let db = digits.as_bytes();
            let valid = (1..=4).contains(&db.len())
                && db[0].is_ascii_digit()
                && db[0] != b'0'
                && db.iter().all(u8::is_ascii_digit);
            if !valid {
                return Err(Error::InvalidModifier {
                    pos: content_pos + start + modifier_at,
                });
            }
            let n = digits.parse::<usize>().unwrap_or(0);
            if !(1..=9999).contains(&n) {
                return Err(Error::InvalidModifier {
                    pos: content_pos + start + modifier_at,
                });
            }
            Modifier::Prefix(n)
        } else {
            return Err(Error::InvalidModifier {
                pos: content_pos + start + modifier_at,
            });
        };

        if !used.insert(name.clone()) {
            return Err(Error::DuplicateVariable {
                pos: content_pos + start,
            });
        }

        specs.push(Specifier {
            name,
            start: content_pos + start,
            modifier,
        });

        if end == bytes.len() {
            break;
        }
        start = end + 1;
        if start == bytes.len() {
            return Err(Error::EmptyVariable {
                pos: close_pos,
            });
        }
    }

    Ok(Expression { operator, specifiers: specs })
}

fn encode(input: &str, reserved: bool) -> String {
    let bytes = input.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' && i + 2 < bytes.len()
            && bytes[i + 1].is_ascii_hexdigit()
            && bytes[i + 2].is_ascii_hexdigit()
        {
            out.push('%');
            out.push((bytes[i + 1] as char).to_ascii_uppercase());
            out.push((bytes[i + 2] as char).to_ascii_uppercase());
            i += 3;
            continue;
        }

        let unreserved = b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~');
        let is_reserved = reserved && matches!(
            b,
            b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@' | b'!' | b'$' | b'&'
                | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
        );

        if unreserved || is_reserved {
            out.push(b as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0f) as usize] as char);
        }
        i += 1;
    }
    out
}

fn chosen_value<'a>(name: &str, vars: &'a [(&str, Value)]) -> Option<&'a Value> {
    vars.iter().rev().find(|(key, _)| *key == name).map(|(_, value)| value)
}

fn prefix_chars(value: &str, n: usize) -> String {
    value.chars().take(n).collect()
}

fn scalar_field(name: &str, value: String, operator: Operator) -> String {
    let encoded = encode(&value, matches!(operator, Operator::Reserved | Operator::Fragment));
    match operator {
        Operator::Semicolon if encoded.is_empty() => name.to_owned(),
        Operator::Semicolon | Operator::Query | Operator::Continuation => {
            format!("{name}={encoded}")
        }
        _ => encoded,
    }
}

fn expand_expression(expr: &Expression, vars: &[(&str, Value)]) -> Result<String, Error> {
    let mut fields = Vec::new();
    for spec in &expr.specifiers {
        let Some(value) = chosen_value(&spec.name, vars) else {
            continue;
        };

        if matches!(spec.modifier, Modifier::Prefix(_))
            && !matches!(value, Value::Scalar(_))
        {
            return Err(Error::TypeMismatch { pos: spec.start });
        }

        let explode = matches!(spec.modifier, Modifier::Explode);
        let reserved = matches!(expr.operator, Operator::Reserved | Operator::Fragment);

        match value {
            Value::Scalar(s) => {
                let value = match spec.modifier {
                    Modifier::Prefix(n) => prefix_chars(s, n),
                    _ => s.clone(),
                };
                fields.push(scalar_field(&spec.name, value, expr.operator));
            }
            Value::List(items) => {
                if items.is_empty() {
                    continue;
                }
                if explode {
                    for item in items {
                        fields.push(scalar_field(&spec.name, item.clone(), expr.operator));
                    }
                } else {
                    let joined = items
                        .iter()
                        .map(|s| encode(s, reserved))
                        .collect::<Vec<_>>()
                        .join(",");
                    match expr.operator {
                        Operator::Semicolon if joined.is_empty() => fields.push(spec.name.clone()),
                        Operator::Semicolon | Operator::Query | Operator::Continuation => {
                            fields.push(format!("{}={joined}", spec.name));
                        }
                        _ => fields.push(joined),
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
                if explode {
                    for (key, val) in entries {
                        let k = encode(key, reserved);
                        let v = encode(val, reserved);
                        if expr.operator == Operator::Semicolon && v.is_empty() {
                            fields.push(k);
                        } else {
                            fields.push(format!("{k}={v}"));
                        }
                    }
                } else {
                    let components = entries
                        .iter()
                        .flat_map(|(k, v)| [encode(k, reserved), encode(v, reserved)])
                        .collect::<Vec<_>>();
                    let joined = components.join(",");
                    match expr.operator {
                        Operator::Semicolon if joined.is_empty() => fields.push(spec.name.clone()),
                        Operator::Semicolon | Operator::Query | Operator::Continuation => {
                            fields.push(format!("{}={joined}", spec.name));
                        }
                        _ => fields.push(joined),
                    }
                }
            }
        }
    }

    if fields.is_empty() {
        return Ok(String::new());
    }

    let (prefix, separator) = match expr.operator {
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
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'}' => return Err(Error::UnexpectedClose { pos: i }),
            b'{' => {
                let open = i;
                let Some(rel_close) = bytes[i + 1..].iter().position(|&b| b == b'}') else {
                    return Err(Error::Unclosed { pos: open });
                };
                let close = i + 1 + rel_close;
                if let Some(inner) = bytes[i + 1..close].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen { pos: i + 1 + inner });
                }
                let content = &template[i + 1..close];
                let expr = parse_expression(content, open + 1, close)?;
                expressions.push((open, close + 1, expr));
                i = close + 1;
            }
            _ => i += 1,
        }
    }

    let mut out = String::new();
    let mut cursor = 0;
    for (start, end, expr) in expressions {
        out.push_str(&template[cursor..start]);
        out.push_str(&expand_expression(&expr, vars)?);
        cursor = end;
    }
    out.push_str(&template[cursor..]);
    Ok(out)
}

fn main() {}