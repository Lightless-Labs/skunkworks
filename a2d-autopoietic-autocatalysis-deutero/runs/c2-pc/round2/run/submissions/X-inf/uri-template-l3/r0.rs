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

struct ParsedExpression<'a> {
    open: usize,
    close: usize,
    operator: Operator,
    names: Vec<&'a str>,
}

fn supported_operator(byte: u8) -> Option<Operator> {
    match byte {
        b'+' => Some(Operator::Reserved),
        b'#' => Some(Operator::Fragment),
        b'.' => Some(Operator::Dot),
        b'/' => Some(Operator::Slash),
        b';' => Some(Operator::Semicolon),
        b'?' => Some(Operator::Query),
        b'&' => Some(Operator::Continuation),
        _ => None,
    }
}

fn parse_one(template: &str, open: usize, close: usize) -> Result<ParsedExpression<'_>, Error> {
    let start = open + 1;
    let bytes = template.as_bytes();

    if let Some(inner) = bytes[start..close].iter().position(|&b| b == b'{') {
        return Err(Error::NestedOpen { pos: start + inner });
    }

    if start == close {
        return Err(Error::EmptyExpression { pos: close });
    }

    let operator = supported_operator(bytes[start]).unwrap_or(Operator::Simple);
    let names_start = if matches!(operator, Operator::Simple) {
        start
    } else {
        start + 1
    };

    if names_start == close {
        return Err(Error::EmptyExpression { pos: close });
    }

    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut segment_start = names_start;

    for end in names_start..=close {
        if end == close || bytes[end] == b',' {
            if end == segment_start {
                return Err(Error::EmptyVariable { pos: segment_start });
            }

            let name = &template[segment_start..end];
            for (offset, &byte) in name.as_bytes().iter().enumerate() {
                let valid = if offset == 0 {
                    byte.is_ascii_alphabetic() || byte == b'_'
                } else {
                    byte.is_ascii_alphanumeric() || byte == b'_'
                };
                if !valid {
                    return Err(Error::InvalidName {
                        pos: segment_start + offset,
                    });
                }
            }

            if !seen.insert(name) {
                return Err(Error::DuplicateVariable { pos: segment_start });
            }
            names.push(name);
            segment_start = end + 1;
        }
    }

    Ok(ParsedExpression {
        open,
        close,
        operator,
        names,
    })
}

fn parse(template: &str) -> Result<Vec<ParsedExpression<'_>>, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'}' {
            return Err(Error::UnexpectedClose { pos: i });
        }
        if bytes[i] != b'{' {
            i += 1;
            continue;
        }

        let open = i;
        let Some(relative) = bytes[i + 1..].iter().position(|&byte| byte == b'}') else {
            return Err(Error::Unclosed { pos: open });
        };
        let close = i + 1 + relative;
        expressions.push(parse_one(template, open, close)?);
        i = close + 1;
    }

    Ok(expressions)
}

fn reserved(byte: u8) -> bool {
    matches!(
        byte,
        b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@' | b'!' | b'$' | b'&'
            | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
    )
}

fn encode(value: &str, allow_reserved: bool) -> String {
    const HEX: &[u8] = b"0123456789ABCDEF";
    let bytes = value.as_bytes();
    let mut out = String::new();
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];
        if byte == b'%'
            && i + 2 < bytes.len()
            && bytes[i + 1].is_ascii_hexdigit()
            && bytes[i + 2].is_ascii_hexdigit()
        {
            out.push('%');
            out.push((bytes[i + 1] as char).to_ascii_uppercase());
            out.push((bytes[i + 2] as char).to_ascii_uppercase());
            i += 3;
        } else {
            let unreserved = byte.is_ascii_alphanumeric()
                || matches!(byte, b'-' | b'.' | b'_' | b'~');
            if unreserved || (allow_reserved && reserved(byte)) {
                out.push(byte as char);
            } else {
                out.push('%');
                out.push(HEX[(byte >> 4) as usize] as char);
                out.push(HEX[(byte & 15) as usize] as char);
            }
            i += 1;
        }
    }

    out
}

fn get_value<'a>(vars: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    for &(binding, value) in vars.iter().rev() {
        if binding == name {
            return Some(value);
        }
    }
    None
}

fn render(expression: &ParsedExpression<'_>, vars: &[(&str, &str)]) -> String {
    let mut fields = Vec::new();

    for name in &expression.names {
        let Some(value) = get_value(vars, name) else {
            continue;
        };

        let encoded = encode(
            value,
            matches!(
                expression.operator,
                Operator::Reserved | Operator::Fragment
            ),
        );
        let field = match expression.operator {
            Operator::Semicolon if encoded.is_empty() => (*name).to_string(),
            Operator::Semicolon | Operator::Query | Operator::Continuation => {
                format!("{name}={encoded}")
            }
            _ => encoded,
        };
        fields.push(field);
    }

    if fields.is_empty() {
        return String::new();
    }

    let (prefix, separator) = match expression.operator {
        Operator::Simple | Operator::Reserved => ("", ","),
        Operator::Fragment => ("#", ","),
        Operator::Dot => (".", "."),
        Operator::Slash => ("/", "/"),
        Operator::Semicolon => (";", ";"),
        Operator::Query => ("?", "&"),
        Operator::Continuation => ("&", "&"),
    };

    format!("{prefix}{}", fields.join(separator))
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let parsed = parse(template)?;
    let mut result = String::new();
    let mut cursor = 0;

    for expression in &parsed {
        result.push_str(&template[cursor..expression.open]);
        result.push_str(&render(expression, vars));
        cursor = expression.close + 1;
    }

    result.push_str(&template[cursor..]);
    Ok(result)
}