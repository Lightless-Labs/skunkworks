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
}

#[derive(Debug, Clone, Copy)]
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

#[derive(Debug)]
struct Expression<'a> {
    operator: Operator,
    vars: Vec<(&'a str, usize)>,
}

fn parse_expression(content: &str, content_start: usize, close_pos: usize) -> Result<Expression<'_>, Error> {
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
        } else {
            (Operator::Simple, 0)
        }
    } else {
        return Err(Error::EmptyExpression { pos: close_pos });
    };

    if content.is_empty() {
        return Err(Error::EmptyExpression { pos: close_pos });
    }

    let rest = &content[offset..];
    let rest_start = content_start + offset;
    let mut vars = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    let mut spec_start = 0;

    loop {
        let end = rest[spec_start..]
            .find(',')
            .map(|i| spec_start + i)
            .unwrap_or(rest.len());
        let spec = &rest[spec_start..end];

        if spec.is_empty() {
            return Err(Error::EmptyVariable {
                pos: rest_start + spec_start,
            });
        }

        let spec_bytes = spec.as_bytes();
        let first = spec_bytes[0];
        if !first.is_ascii_alphabetic() && first != b'_' {
            return Err(Error::InvalidName {
                pos: rest_start + spec_start,
            });
        }

        for (i, &b) in spec_bytes.iter().enumerate().skip(1) {
            if !b.is_ascii_alphanumeric() && b != b'_' {
                return Err(Error::InvalidName {
                    pos: rest_start + spec_start + i,
                });
            }
        }

        if seen.iter().any(|&name| name == spec) {
            return Err(Error::DuplicateVariable {
                pos: rest_start + spec_start,
            });
        }
        seen.push(spec);
        vars.push((spec, rest_start + spec_start));

        if end == rest.len() {
            break;
        }
        spec_start = end + 1;
    }

    Ok(Expression { operator, vars })
}

fn encode(value: &str, reserved: bool) -> String {
    let mut out = String::new();
    let bytes = value.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];

        if b == b'%' && i + 2 < bytes.len() && bytes[i + 1].is_ascii_hexdigit() && bytes[i + 2].is_ascii_hexdigit() {
            out.push('%');
            out.push((bytes[i + 1] as char).to_ascii_uppercase());
            out.push((bytes[i + 2] as char).to_ascii_uppercase());
            i += 3;
            continue;
        }

        let unreserved = b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~');
        let reserved_byte = reserved
            && matches!(
                b,
                b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@' | b'!' | b'$' | b'&' | b'\''
                    | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
            );

        if unreserved || reserved_byte {
            out.push(b as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0F) as usize] as char);
        }
        i += 1;
    }

    out
}

fn binding_value<'a>(name: &str, vars: &'a [(&str, &str)]) -> Option<&'a str> {
    vars.iter()
        .rev()
        .find(|(binding_name, _)| *binding_name == name)
        .map(|(_, value)| *value)
}

fn expand_expression(expr: &Expression<'_>, vars: &[(&str, &str)]) -> String {
    let mut fields: Vec<String> = Vec::new();
    let reserved = matches!(expr.operator, Operator::Reserved | Operator::Fragment);
    let named = matches!(
        expr.operator,
        Operator::Semicolon | Operator::Query | Operator::Continuation
    );

    for &(name, _) in &expr.vars {
        if let Some(value) = binding_value(name, vars) {
            let encoded = encode(value, reserved);
            let field = if named {
                if matches!(expr.operator, Operator::Semicolon) && encoded.is_empty() {
                    name.to_string()
                } else {
                    format!("{name}={encoded}")
                }
            } else {
                encoded
            };
            fields.push(field);
        }
    }

    if fields.is_empty() {
        return String::new();
    }

    let (separator, prefix) = match expr.operator {
        Operator::Simple | Operator::Reserved => (",", ""),
        Operator::Fragment => (",", "#"),
        Operator::Dot => (".", "."),
        Operator::Slash => ("/", "/"),
        Operator::Semicolon => (";", ";"),
        Operator::Query => ("&", "?"),
        Operator::Continuation => ("&", "&"),
    };

    let mut out = String::from(prefix);
    out.push_str(&fields.join(separator));
    out
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let bytes = template.as_bytes();
    let mut expressions: Vec<(usize, usize, Expression<'_>)> = Vec::new();
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

                if let Some(offset) = bytes[i + 1..close].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen { pos: i + 1 + offset });
                }

                let content = &template[i + 1..close];
                let expr = parse_expression(content, open + 1, close)?;
                expressions.push((open, close + 1, expr));
                i = close + 1;
            }
            _ => i += 1,
        }
    }

    let mut output = String::new();
    let mut cursor = 0;
    for (start, end, expr) in expressions {
        output.push_str(&template[cursor..start]);
        output.push_str(&expand_expression(&expr, vars));
        cursor = end;
    }
    output.push_str(&template[cursor..]);
    Ok(output)
}