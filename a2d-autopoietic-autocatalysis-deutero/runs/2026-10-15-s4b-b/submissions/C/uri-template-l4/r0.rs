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

#[derive(Clone)]
struct Spec {
    name: String,
    start: usize,
    modifier: Modifier,
}

#[derive(Clone, Copy)]
enum Modifier {
    None,
    Explode,
    Prefix(usize),
}

#[derive(Clone, Copy)]
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

struct Expression {
    operator: Operator,
    specs: Vec<Spec>,
}

fn is_name_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_name_continue(b: u8) -> bool {
    is_name_start(b) || b.is_ascii_digit()
}

fn parse_expression(content: &str, content_pos: usize, close_pos: usize) -> Result<Expression, Error> {
    let bytes = content.as_bytes();
    let (operator, offset) = if !bytes.is_empty() {
        match bytes[0] {
            b'+' => (Operator::Reserved, 1),
            b'#' => (Operator::Fragment, 1),
            b'.' => (Operator::Dot, 1),
            b'/' => (Operator::Slash, 1),
            b';' => (Operator::Semicolon, 1),
            b'?' => (Operator::Query, 1),
            b'&' => (Operator::Continuation, 1),
            b'!' | b'=' | b',' | b'@' | b'|' => {
                return Err(Error::UnsupportedOperator { pos: content_pos });
            }
            _ => (Operator::Simple, 0),
        }
    } else {
        (Operator::Simple, 0)
    };

    if offset == bytes.len() {
        return Err(Error::EmptyExpression { pos: close_pos });
    }

    let mut specs = Vec::new();
    let mut seen = std::collections::HashSet::<String>::new();
    let mut part_start = offset;

    loop {
        let part_end = bytes[part_start..]
            .iter()
            .position(|&b| b == b',')
            .map(|i| part_start + i)
            .unwrap_or(bytes.len());

        if part_start == part_end {
            return Err(Error::EmptyVariable {
                pos: content_pos + part_start,
            });
        }

        let part = &bytes[part_start..part_end];
        let modifier_at = part.iter().position(|&b| b == b'*' || b == b':');
        let name_end = modifier_at.map(|i| part_start + i).unwrap_or(part_end);

        if name_end == part_start {
            return Err(Error::InvalidName {
                pos: content_pos + part_start,
            });
        }

        let name_bytes = &bytes[part_start..name_end];
        let mut invalid_at = None;
        for (i, &b) in name_bytes.iter().enumerate() {
            if if i == 0 { !is_name_start(b) } else { !is_name_continue(b) } {
                invalid_at = Some(i);
                break;
            }
        }
        if let Some(i) = invalid_at {
            return Err(Error::InvalidName {
                pos: content_pos + part_start + i,
            });
        }

        let name = std::str::from_utf8(name_bytes).unwrap().to_owned();
        let modifier = if let Some(i) = modifier_at {
            let mpos = content_pos + part_start + i;
            let suffix = &bytes[part_start + i..part_end];
            if suffix == b"*" {
                Modifier::Explode
            } else if suffix.first() == Some(&b':') {
                let digits = &suffix[1..];
                if digits.is_empty()
                    || digits.len() > 4
                    || digits[0] == b'0'
                    || !digits.iter().all(u8::is_ascii_digit)
                {
                    return Err(Error::InvalidModifier { pos: mpos });
                }
                let mut n = 0usize;
                for &d in digits {
                    n = n * 10 + (d - b'0') as usize;
                }
                if !(1..=9999).contains(&n) {
                    return Err(Error::InvalidModifier { pos: mpos });
                }
                Modifier::Prefix(n)
            } else {
                return Err(Error::InvalidModifier { pos: mpos });
            }
        } else {
            Modifier::None
        };

        if !seen.insert(name.clone()) {
            return Err(Error::DuplicateVariable {
                pos: content_pos + part_start,
            });
        }

        specs.push(Spec {
            name,
            start: content_pos + part_start,
            modifier,
        });

        if part_end == bytes.len() {
            break;
        }
        part_start = part_end + 1;
        if part_start == bytes.len() {
            return Err(Error::EmptyVariable {
                pos: close_pos,
            });
        }
    }

    Ok(Expression { operator, specs })
}

fn validate_template(template: &str) -> Result<Vec<(usize, usize, Expression)>, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'}' => return Err(Error::UnexpectedClose { pos: i }),
            b'{' => {
                let open = i;
                let close = match bytes[i + 1..].iter().position(|&b| b == b'}') {
                    Some(n) => i + 1 + n,
                    None => return Err(Error::Unclosed { pos: open }),
                };
                if let Some(n) = bytes[open + 1..close].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen { pos: open + 1 + n });
                }
                let content = &template[open + 1..close];
                let expr = parse_expression(content, open + 1, close)?;
                expressions.push((open, close + 1, expr));
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

fn hex_upper(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b,
        b'a'..=b'f' => b - b'a' + b'A',
        _ => b,
    }
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
            out.push(hex_upper(bytes[i + 1]) as char);
            out.push(hex_upper(bytes[i + 2]) as char);
            i += 3;
        } else if is_unreserved(b) || (reserved && is_reserved(b)) {
            out.push(b as char);
            i += 1;
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 15) as usize] as char);
            i += 1;
        }
    }
    out
}

fn prefix_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn selected_value<'a>(name: &str, vars: &'a [(&str, Value)]) -> Option<&'a Value> {
    vars.iter().rev().find(|(key, _)| *key == name).map(|(_, value)| value)
}

fn named_field(name: &str, value: String, operator: Operator) -> String {
    match operator {
        Operator::Semicolon if value.is_empty() => name.to_owned(),
        _ => format!("{name}={value}"),
    }
}

fn expand_expr(expr: &Expression, vars: &[(&str, Value)]) -> Result<String, Error> {
    let reserved = matches!(expr.operator, Operator::Reserved | Operator::Fragment);
    let mut fields = Vec::<String>::new();

    for spec in &expr.specs {
        let value = match selected_value(&spec.name, vars) {
            Some(v) => v,
            None => continue,
        };

        if matches!(spec.modifier, Modifier::Prefix(_))
            && !matches!(value, Value::Scalar(_))
        {
            return Err(Error::TypeMismatch { pos: spec.start });
        }

        let explode = matches!(spec.modifier, Modifier::Explode);
        match value {
            Value::Scalar(s) => {
                let s = match spec.modifier {
                    Modifier::Prefix(n) => prefix_chars(s, n),
                    _ => s.clone(),
                };
                let encoded = encode(&s, reserved);
                if matches!(expr.operator, Operator::Semicolon | Operator::Query | Operator::Continuation) {
                    fields.push(named_field(&spec.name, encoded, expr.operator));
                } else {
                    fields.push(encoded);
                }
            }
            Value::List(items) => {
                if items.is_empty() {
                    continue;
                }
                if explode {
                    for item in items {
                        let encoded = encode(item, reserved);
                        if matches!(expr.operator, Operator::Semicolon | Operator::Query | Operator::Continuation) {
                            fields.push(named_field(&spec.name, encoded, expr.operator));
                        } else {
                            fields.push(encoded);
                        }
                    }
                } else {
                    let joined = items.iter().map(|s| encode(s, reserved)).collect::<Vec<_>>().join(",");
                    let field = if matches!(expr.operator, Operator::Semicolon | Operator::Query | Operator::Continuation) {
                        named_field(&spec.name, joined, expr.operator)
                    } else {
                        joined
                    };
                    fields.push(field);
                }
            }
            Value::Map(entries) => {
                let mut keys = std::collections::HashSet::<&str>::new();
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
                        let pair = if matches!(expr.operator, Operator::Semicolon) && v.is_empty() {
                            k
                        } else {
                            format!("{k}={v}")
                        };
                        fields.push(pair);
                    }
                } else {
                    let mut components = Vec::with_capacity(entries.len() * 2);
                    for (key, val) in entries {
                        components.push(encode(key, reserved));
                        components.push(encode(val, reserved));
                    }
                    let joined = components.join(",");
                    let field = if matches!(expr.operator, Operator::Semicolon | Operator::Query | Operator::Continuation) {
                        named_field(&spec.name, joined, expr.operator)
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
    let expressions = validate_template(template)?;
    let mut out = String::new();
    let mut cursor = 0;

    for (start, end, expr) in expressions {
        out.push_str(&template[cursor..start]);
        out.push_str(&expand_expr(&expr, vars)?);
        cursor = end;
    }
    out.push_str(&template[cursor..]);
    Ok(out)
}

fn main() {}