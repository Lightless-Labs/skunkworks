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

#[derive(Debug, Clone)]
struct Expression<'a> {
    operator: Option<char>,
    specs: Vec<Specifier<'a>>,
}

#[derive(Debug, Clone)]
struct Specifier<'a> {
    name: &'a str,
    start: usize,
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

fn parse_expression<'a>(
    template: &'a str,
    open: usize,
    close: usize,
) -> Result<Expression<'a>, Error> {
    let content_start = open + 1;
    let content = &template[content_start..close];

    let operator = content
        .as_bytes()
        .first()
        .copied()
        .filter(|b| matches!(b, b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&'))
        .map(char::from);

    let body_start = content_start + usize::from(operator.is_some());
    let body = &template[body_start..close];

    if body.is_empty() {
        return Err(Error::EmptyExpression { pos: close });
    }

    if let Some(op) = operator {
        if !matches!(op, '+' | '#' | '.' | '/' | ';' | '?' | '&') {
            return Err(Error::UnsupportedOperator { pos: content_start });
        }
    }

    let mut specs = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    let mut start = body_start;

    for part in body.split(',') {
        if part.is_empty() {
            return Err(Error::EmptyVariable { pos: start });
        }

        let bytes = part.as_bytes();
        let mut invalid = None;
        if !is_name_start(bytes[0]) {
            invalid = Some(start);
        } else {
            for (i, byte) in bytes.iter().copied().enumerate().skip(1) {
                if !is_name_continue(byte) {
                    invalid = Some(start + i);
                    break;
                }
            }
        }

        if let Some(pos) = invalid {
            return Err(Error::InvalidName { pos });
        }

        if seen.contains(&part) {
            return Err(Error::DuplicateVariable { pos: start });
        }
        seen.push(part);
        specs.push(Specifier { name: part, start });
        start += part.len() + 1;
    }

    Ok(Expression { operator, specs })
}

fn encode(value: &str, reserved: bool) -> String {
    let bytes = value.as_bytes();
    let mut out = String::new();
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];

        if byte == b'%' && i + 2 < bytes.len() {
            let h1 = bytes[i + 1];
            let h2 = bytes[i + 2];
            if h1.is_ascii_hexdigit() && h2.is_ascii_hexdigit() {
                out.push('%');
                out.push((h1 as char).to_ascii_uppercase());
                out.push((h2 as char).to_ascii_uppercase());
                i += 3;
                continue;
            }
        }

        let unreserved = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~');
        let reserved_byte = reserved
            && matches!(
                byte,
                b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@' | b'!' | b'$' | b'&'
                    | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
            );

        if unreserved || reserved_byte {
            out.push(byte as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push('%');
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
        }
        i += 1;
    }

    out
}

fn lookup<'a>(name: &str, vars: &'a [(&str, &str)]) -> Option<&'a str> {
    vars.iter()
        .rev()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| *value)
}

fn expand_expression(expression: &Expression<'_>, vars: &[(&str, &str)]) -> String {
    let operator = expression.operator.unwrap_or('\0');
    let reserved = matches!(operator, '+' | '#');
    let mut fields = Vec::new();

    for spec in &expression.specs {
        if let Some(value) = lookup(spec.name, vars) {
            let encoded = encode(value, reserved);
            let field = match operator {
                ';' if encoded.is_empty() => spec.name.to_owned(),
                ';' | '?' | '&' => format!("{}={}", spec.name, encoded),
                _ => encoded,
            };
            fields.push(field);
        }
    }

    if fields.is_empty() {
        return String::new();
    }

    let (prefix, separator) = match operator {
        '#' => ("#", ","),
        '.' => (".", "."),
        '/' => ("/", "/"),
        ';' => (";", ";"),
        '?' => ("?", "&"),
        '&' => ("&", "&"),
        _ => ("", ","),
    };

    let mut out = String::from(prefix);
    out.push_str(&fields.join(separator));
    out
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
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

                let expression = parse_expression(template, open, close)?;
                expressions.push((open, close, expression));
                i = close + 1;
            }
            _ => i += 1,
        }
    }

    let mut out = String::new();
    let mut cursor = 0;
    for (open, close, expression) in &expressions {
        out.push_str(&template[cursor..*open]);
        out.push_str(&expand_expression(expression, vars));
        cursor = close + 1;
    }
    out.push_str(&template[cursor..]);
    Ok(out)
}